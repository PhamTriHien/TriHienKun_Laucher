use crate::crash::{analyze_crash_logs_with_instance_dir, CrashAnalysisReport};
use crate::supervisor::{InstanceKey, LauncherEvent};
use crate::vanilla::PreparedVanillaAssets;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use thk_core::models::{
    sanitize_mobile_render_scale_pct, Account, GameProfile, MobileRendererMode,
};
use thk_core::{
    android_bridge_dir, android_bridge_send_cmd, AppPaths, Database, ThkError, ThkResult,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const MAX_LOG_LINES_PER_INSTANCE: usize = 4_000;

#[derive(Debug, Clone)]
pub enum MobileInputEvent {
    Key { keycode: i32, down: bool },
    Char { ch: char, mods: i32 },
    MouseButton { button: i32, down: bool },
    CursorPos { x: f32, y: f32 },
    Scroll { dx: f64, dy: f64 },
    WindowSize { width: u32, height: u32 },
    Quit,
}

impl MobileInputEvent {
    pub fn to_wire_line(&self) -> String {
        match self {
            Self::Key { keycode, down } => {
                format!("K {keycode} {}\n", if *down { 1 } else { 0 })
            }
            Self::Char { ch, mods } => format!("C {} {mods}\n", *ch as u32),
            Self::MouseButton { button, down } => {
                format!("M {button} {}\n", if *down { 1 } else { 0 })
            }
            Self::CursorPos { x, y } => format!("P {x:.2} {y:.2}\n"),
            Self::Scroll { dx, dy } => format!("S {dx:.3} {dy:.3}\n"),
            Self::WindowSize { width, height } => format!("W {width} {height}\n"),
            Self::Quit => "Q\n".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct MobileSessionHandle {
    tx: mpsc::UnboundedSender<MobileInputEvent>,
    grabbing: Arc<AtomicBool>,
    framebuffer_size: Arc<Mutex<(u32, u32)>>,
}

impl MobileSessionHandle {
    pub fn new(
        tx: mpsc::UnboundedSender<MobileInputEvent>,
        grabbing: Arc<AtomicBool>,
        initial_size: (u32, u32),
    ) -> Self {
        Self {
            tx,
            grabbing,
            framebuffer_size: Arc::new(Mutex::new(initial_size)),
        }
    }

    pub fn send(&self, ev: MobileInputEvent) {
        if let MobileInputEvent::WindowSize { width, height } = ev {
            if let Ok(mut guard) = self.framebuffer_size.lock() {
                *guard = (width, height);
            }
        }
        let _ = self.tx.send(ev);
    }

    pub fn is_grabbing(&self) -> bool {
        self.grabbing.load(Ordering::Relaxed)
    }

    pub fn framebuffer_size(&self) -> (u32, u32) {
        self.framebuffer_size
            .lock()
            .map(|g| *g)
            .unwrap_or((1280, 720))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileLaunchSpec {
    pub session_id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub jre_home: String,
    pub java_major: u32,
    pub runtime_root: String,
    pub lwjgl_version: String,
    pub lwjgl_natives_dir: String,
    pub instance_dir: String,
    pub log_file: String,
    pub window_width: u32,
    pub window_height: u32,
    pub physical_width: u32,
    pub physical_height: u32,
    pub input_port: u16,
    pub renderer_lib: String,
    pub egl_lib: Option<String>,
    pub env: BTreeMap<String, String>,
    pub argv: Vec<String>,
}

pub struct MobileSpecInputs<'a> {
    pub paths: &'a AppPaths,
    pub profile: &'a GameProfile,
    pub account: &'a Account,
    pub access_token: &'a str,
    pub vanilla: &'a PreparedVanillaAssets,
    pub java_bin: &'a Path,
    pub java_major: u32,
    pub main_class: &'a str,
    pub include_client_jar: bool,
    pub loader_jvm_args: &'a [String],
    pub loader_game_args: &'a [String],
    pub legacy_mc_args_override: Option<&'a str>,
    pub authlib_jvm_args: &'a [String],
    pub quick_play_server: Option<&'a str>,
    pub renderer: MobileRendererMode,
    pub render_scale_pct: u32,
    pub surface_size: (u32, u32),
    pub input_port: u16,
    pub runtime_root: &'a Path,
    pub log_file: &'a Path,
}

pub fn detect_lwjgl_bundle_version(vanilla: &PreparedVanillaAssets) -> (&'static str, bool) {
    let mut parsed_num: u32 = 0;
    for lib in &vanilla.version_json.libraries {
        let suffix = lib
            .name
            .strip_prefix("org.lwjgl.lwjgl:lwjgl:")
            .or_else(|| lib.name.strip_prefix("org.lwjgl:lwjgl:"));
        if let Some(ver_str) = suffix {
            let mut acc = 0u32;
            for ch in ver_str.chars() {
                if let Some(d) = ch.to_digit(10) {
                    acc = acc.saturating_mul(10).saturating_add(d);
                } else if ch == '.' {
                    continue;
                } else {
                    break;
                }
            }
            if (200..=999).contains(&acc) {
                parsed_num = acc;
                break;
            }
        }
    }

    if parsed_num == 0 {
        let parts: Vec<u32> = vanilla
            .version_json
            .id
            .split(['.', '-'])
            .filter_map(|s| s.parse::<u32>().ok())
            .collect();
        match parts.as_slice() {
            [first, ..] if *first >= 26 => parsed_num = 341,
            [1, minor, ..] if *minor < 13 => parsed_num = 294,
            _ => parsed_num = 333,
        }
    }

    let is_legacy_lwjgl2 = parsed_num > 0 && parsed_num <= 299;
    let bundle = if parsed_num >= 341 { "3.4.1" } else { "3.3.3" };
    (bundle, is_legacy_lwjgl2)
}

fn instance_has_sodium_family(instance_dir: &Path) -> bool {
    let mods_dir = instance_dir.join("mods");
    let Ok(entries) = fs::read_dir(&mods_dir) else {
        return false;
    };
    const PATTERNS: &[&str] = &[
        "sodium",
        "embeddium",
        "rubidium",
        "xenon",
        "celeritas",
        "indium",
        "iris",
    ];
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".jar") && PATTERNS.iter().any(|p| name.contains(p)) {
            return true;
        }
    }
    false
}

fn prepare_mobile_instance_tweaks(instance_dir: &Path, has_sodium: bool) {
    let config_dir = instance_dir.join("config");
    let _ = fs::create_dir_all(&config_dir);

    let splash_file = config_dir.join("splash.properties");
    if let Ok(existing) = fs::read_to_string(&splash_file) {
        if existing.contains("enabled=true") {
            let _ = fs::write(
                &splash_file,
                existing.replace("enabled=true", "enabled=false"),
            );
        }
    } else {
        let _ = fs::write(&splash_file, "enabled=false\n");
    }

    if has_sodium {
        for mod_name in ["sodium", "embeddium", "rubidium"] {
            let prop_path = config_dir.join(format!("{mod_name}-mixins.properties"));
            let mut content = fs::read_to_string(&prop_path).unwrap_or_default();
            let mut changed = false;
            for line in [
                "mixin.features.buffer_builder.intrinsics=false",
                "mixin.features.chunk_rendering=false",
            ] {
                if !content.contains(line) {
                    if !content.is_empty() && !content.ends_with('\n') {
                        content.push('\n');
                    }
                    content.push_str(line);
                    content.push('\n');
                    changed = true;
                }
            }
            if changed {
                let _ = fs::write(&prop_path, content);
            }
        }
    }
}

fn scale_even_dimension(px: u32, scale_pct: u32) -> u32 {
    let scaled = ((px as u64) * (scale_pct as u64) / 100).max(320) as u32;
    if scaled % 2 == 1 {
        scaled - 1
    } else {
        scaled
    }
}

pub fn build_mobile_launch_spec(inp: MobileSpecInputs<'_>) -> ThkResult<MobileLaunchSpec> {
    let jre_home = inp
        .java_bin
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("/"));

    let has_sodium = instance_has_sodium_family(&inp.profile.instance_path);
    prepare_mobile_instance_tweaks(&inp.profile.instance_path, has_sodium);

    let effective_renderer = if has_sodium && inp.renderer == MobileRendererMode::HolyGl4es {
        MobileRendererMode::MobileGlues
    } else {
        inp.renderer
    };

    let scale_pct = sanitize_mobile_render_scale_pct(inp.render_scale_pct);
    let phys_w = inp.surface_size.0.max(640);
    let phys_h = inp.surface_size.1.max(360);
    let (win_w, win_h) = match (inp.profile.window_width, inp.profile.window_height) {
        (Some(w), Some(h)) if w >= 320 && h >= 240 => (w, h),
        _ => (
            scale_even_dimension(phys_w, scale_pct),
            scale_even_dimension(phys_h, scale_pct),
        ),
    };

    let (lwjgl_ver, is_legacy_lwjgl2) = detect_lwjgl_bundle_version(inp.vanilla);
    let runtime_lib_dir = inp.runtime_root.join("lib");
    let lwjgl_natives_dir = inp
        .runtime_root
        .join(format!("lwjgl-{lwjgl_ver}-natives"))
        .join("arm64-v8a");
    let lwjgl_jars_dir = inp.runtime_root.join("lwjgl3").join(lwjgl_ver);

    let lib_aarch64 = jre_home.join("lib").join("aarch64");
    let jre_arch_dir = if lib_aarch64.is_dir() {
        lib_aarch64
    } else {
        jre_home.join("lib")
    };
    let jvm_subdir = if jre_arch_dir.join("client").join("libjvm.so").is_file()
        && !jre_arch_dir.join("server").join("libjvm.so").is_file()
    {
        jre_arch_dir.join("client")
    } else {
        jre_arch_dir.join("server")
    };

    let ld_library_path = [
        jvm_subdir.display().to_string(),
        jre_arch_dir.join("jli").display().to_string(),
        jre_arch_dir.display().to_string(),
        "/system/lib64".to_string(),
        "/vendor/lib64".to_string(),
        "/vendor/lib64/hw".to_string(),
        runtime_lib_dir.display().to_string(),
        lwjgl_natives_dir.display().to_string(),
    ]
    .join(":");

    let cache_dir = inp.paths.cache_dir.clone();
    let _ = fs::create_dir_all(&cache_dir);
    let resolv_conf = inp.runtime_root.join("resolv.conf");
    if !resolv_conf.is_file() {
        let _ = fs::write(&resolv_conf, "nameserver 1.1.1.1\nnameserver 8.8.8.8\n");
    }

    let (renderer_lib, egl_lib, libgl_es, renderer_tag) = match effective_renderer {
        MobileRendererMode::HolyGl4es => ("libng_gl4es.so", None, "2", "opengles2"),
        MobileRendererMode::MobileGlues => {
            let mg_dir = inp.paths.root.join("MobileGlues");
            let _ = fs::create_dir_all(&mg_dir);
            (
                "libmobileglues.so",
                Some("libmobileglues.so".to_string()),
                "3",
                "opengles_mobileglues",
            )
        }
    };

    let mut env = BTreeMap::new();
    env.insert(
        "POJAV_NATIVEDIR".into(),
        runtime_lib_dir.display().to_string(),
    );
    env.insert("JAVA_HOME".into(), jre_home.display().to_string());
    env.insert(
        "HOME".into(),
        inp.profile.instance_path.display().to_string(),
    );
    env.insert("TMPDIR".into(), cache_dir.display().to_string());
    env.insert("LIBGL_MIPMAP".into(), "3".into());
    env.insert("LIBGL_NOERROR".into(), "1".into());
    env.insert("LIBGL_NOINTOVLHACK".into(), "1".into());
    env.insert("LIBGL_NORMALIZE".into(), "1".into());
    env.insert("LIBGL_ES".into(), libgl_es.into());
    env.insert("FORCE_VSYNC".into(), "true".into());
    env.insert(
        "MESA_GLSL_CACHE_DIR".into(),
        cache_dir.display().to_string(),
    );
    env.insert("LD_LIBRARY_PATH".into(), ld_library_path);
    env.insert(
        "PATH".into(),
        format!("{}/bin:/system/bin:/system/xbin", jre_home.display()),
    );
    env.insert("AMETHYST_RENDERER".into(), renderer_tag.into());
    env.insert("AWTSTUB_WIDTH".into(), win_w.to_string());
    env.insert("AWTSTUB_HEIGHT".into(), win_h.to_string());
    env.insert("POJAV_BIG_CORE_AFFINITY".into(), "1".into());
    env.insert("SDL_OPENGL_LIBRARY".into(), renderer_lib.into());
    if let Some(ref egl) = egl_lib {
        env.insert("POJAVEXEC_EGL".into(), egl.clone());
        env.insert(
            "SDL_EGL_LIBRARY".into(),
            runtime_lib_dir.join(egl).display().to_string(),
        );
        env.insert(
            "MG_DIR_PATH".into(),
            inp.paths.root.join("MobileGlues").display().to_string(),
        );
    }

    let mut cp_entries: Vec<String> = Vec::new();
    cp_entries.push(lwjgl_jars_dir.join("lwjgl.jar").display().to_string());
    cp_entries.push(
        lwjgl_jars_dir
            .join(format!("lwjgl-{lwjgl_ver}-merged-modules.jar"))
            .display()
            .to_string(),
    );
    if let Ok(entries) = fs::read_dir(&lwjgl_jars_dir) {
        let mut extras: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                    return false;
                };
                name.ends_with(".jar")
                    && name != "lwjgl.jar"
                    && !name.ends_with("-merged-modules.jar")
                    && !name.ends_with("lwjglx.jar")
            })
            .collect();
        extras.sort();
        for jar in extras {
            cp_entries.push(jar.display().to_string());
        }
    }

    for lib_path in &inp.vanilla.libraries {
        let norm = lib_path.to_string_lossy().replace('\\', "/");
        if norm.contains("/org/lwjgl/") {
            continue;
        }
        cp_entries.push(lib_path.display().to_string());
    }

    if inp.include_client_jar {
        cp_entries.push(inp.vanilla.client_jar.display().to_string());
    }
    if is_legacy_lwjgl2 {
        cp_entries.push(
            lwjgl_jars_dir
                .join("lwjgl-lwjglx.jar")
                .display()
                .to_string(),
        );
    }

    let mut modified_vanilla = inp.vanilla.clone();
    modified_vanilla.libraries = cp_entries.iter().map(PathBuf::from).collect();

    let mut desktop_style_args = crate::supervisor::build_full_jvm_and_game_args(
        inp.profile,
        inp.account,
        inp.access_token,
        &modified_vanilla,
        inp.main_class,
        false,
        inp.loader_jvm_args,
        inp.loader_game_args,
        inp.legacy_mc_args_override,
        inp.authlib_jvm_args,
        inp.quick_play_server,
    );

    desktop_style_args.retain(|arg| {
        !arg.starts_with("-Djava.library.path=")
            && !arg.starts_with("-XX:+UseTransparentHugePages")
            && !arg.starts_with("-XX:+UseLargePages")
            && !arg.starts_with("-Dorg.lwjgl.opengl.libname=")
            && !arg.starts_with("-Dorg.lwjgl.freetype.libname=")
            && !arg.starts_with("-XX:ActiveProcessorCount=")
            && arg != "-XstartOnFirstThread"
            && arg != "--fullscreen"
    });

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(1);
    let injector_jar = if is_legacy_lwjgl2 {
        inp.runtime_root
            .join("methods_injector_agent")
            .join("lwjgl2_methods_injector.jar")
    } else {
        inp.runtime_root
            .join("methods_injector_agent")
            .join("methods_injector_agent.jar")
    };

    let is_java_8 = inp.java_major <= 8;
    let mut mobile_prefix = vec![
        "java".to_string(),
        format!("-Djava.home={}", jre_home.display()),
        format!("-Djava.io.tmpdir={}", cache_dir.display()),
        format!("-Djna.boot.library.path={}", runtime_lib_dir.display()),
        format!("-Duser.home={}", inp.profile.instance_path.display()),
        "-Dos.name=Linux".to_string(),
        "-Dos.version=Android".to_string(),
        "-Dorg.lwjgl.vulkan.libname=libvulkan.so".to_string(),
        format!("-Dorg.lwjgl.opengl.libname={renderer_lib}"),
        format!(
            "-Dorg.lwjgl.freetype.libname={}",
            lwjgl_natives_dir.join("libfreetype.so").display()
        ),
        format!("-Dorg.lwjgl.librarypath={}", lwjgl_natives_dir.display()),
        format!(
            "-Djava.library.path={}:{}",
            lwjgl_natives_dir.display(),
            runtime_lib_dir.display()
        ),
        "-Dorg.lwjgl.spvc.libname=spirv-cross-c-shared".to_string(),
        "-Dorg.lwjgl.system.allocator=system".to_string(),
        format!("-Dglfwstub.windowWidth={win_w}"),
        format!("-Dglfwstub.windowHeight={win_h}"),
        "-Dglfwstub.initEgl=false".to_string(),
        format!("-Dext.net.resolvPath={}", resolv_conf.display()),
        "-Dlog4j2.formatMsgNoLookups=true".to_string(),
        "-Dnet.minecraft.clientmodname=TriHienKun".to_string(),
        "-Dfml.earlyprogresswindow=false".to_string(),
        "-Dfml.ignoreInvalidMinecraftCertificates=true".to_string(),
        "-Dloader.disable_forked_guis=true".to_string(),
        "-Djdk.lang.Process.launchMechanism=FORK".to_string(),
        format!(
            "-DZstdNativePath={}",
            runtime_lib_dir
                .join("libzstd-jni-1.5.7-6-dhcompat.so")
                .display()
        ),
        format!("-XX:ActiveProcessorCount={cores}"),
        "-Djava.awt.headless=false".to_string(),
        format!("-Dcacio.managed.screensize={win_w}x{win_h}"),
        "-Dcacio.font.fontmanager=sun.awt.X11FontManager".to_string(),
        "-Dcacio.font.fontscaler=sun.font.FreetypeFontScaler".to_string(),
        "-Dswing.defaultlaf=javax.swing.plaf.metal.MetalLookAndFeel".to_string(),
    ];

    let cacio_dir = inp.runtime_root.join(if is_java_8 {
        "caciocavallo"
    } else {
        "caciocavallo17"
    });
    if is_java_8 {
        mobile_prefix.push("-Dawt.toolkit=net.java.openjdk.cacio.ctc.CTCToolkit".into());
        mobile_prefix.push(
            "-Djava.awt.graphicsenv=net.java.openjdk.cacio.ctc.CTCGraphicsEnvironment".into(),
        );
    } else {
        if let Ok(entries) = fs::read_dir(&cacio_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.contains("cacio-tta") && n.ends_with(".jar"))
                {
                    mobile_prefix.push(format!("-javaagent:{}", p.display()));
                    break;
                }
            }
        }
        mobile_prefix
            .push("-Dawt.toolkit=com.github.caciocavallosilano.cacio.ctc.CTCToolkit".into());
        mobile_prefix.push(
            "-Djava.awt.graphicsenv=com.github.caciocavallosilano.cacio.ctc.CTCGraphicsEnvironment"
                .into(),
        );
        for flag in [
            "--add-exports=java.desktop/java.awt=ALL-UNNAMED",
            "--add-exports=java.desktop/java.awt.peer=ALL-UNNAMED",
            "--add-exports=java.desktop/sun.awt.image=ALL-UNNAMED",
            "--add-exports=java.desktop/sun.java2d=ALL-UNNAMED",
            "--add-exports=java.desktop/java.awt.dnd.peer=ALL-UNNAMED",
            "--add-exports=java.desktop/sun.awt=ALL-UNNAMED",
            "--add-exports=java.desktop/sun.awt.event=ALL-UNNAMED",
            "--add-exports=java.desktop/sun.awt.datatransfer=ALL-UNNAMED",
            "--add-exports=java.desktop/sun.font=ALL-UNNAMED",
            "--add-exports=java.base/sun.security.action=ALL-UNNAMED",
            "--add-opens=java.base/java.util=ALL-UNNAMED",
            "--add-opens=java.desktop/java.awt=ALL-UNNAMED",
            "--add-opens=java.desktop/sun.font=ALL-UNNAMED",
            "--add-opens=java.desktop/sun.java2d=ALL-UNNAMED",
            "--add-opens=java.base/java.lang.reflect=ALL-UNNAMED",
            "--add-opens=java.base/java.net=ALL-UNNAMED",
        ] {
            mobile_prefix.push(flag.to_string());
        }
    }

    if let Ok(entries) = fs::read_dir(&cacio_dir) {
        let mut jars: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".jar"))
            })
            .collect();
        jars.sort();
        if !jars.is_empty() {
            let joined = jars
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(":");
            mobile_prefix.push(format!(
                "-Xbootclasspath/{}:{joined}",
                if is_java_8 { "p" } else { "a" }
            ));
        }
    }

    if injector_jar.is_file() {
        mobile_prefix.push(format!("-javaagent:{}", injector_jar.display()));
    }
    if has_sodium {
        mobile_prefix.push("-Dsodium.checks.issue2561=false".to_string());
    }

    mobile_prefix.extend(desktop_style_args);

    Ok(MobileLaunchSpec {
        session_id: Uuid::new_v4().to_string(),
        profile_id: inp.profile.id.clone(),
        profile_name: inp.profile.name.clone(),
        jre_home: jre_home.display().to_string(),
        java_major: inp.java_major,
        runtime_root: inp.runtime_root.display().to_string(),
        lwjgl_version: lwjgl_ver.to_string(),
        lwjgl_natives_dir: lwjgl_natives_dir.display().to_string(),
        instance_dir: inp.profile.instance_path.display().to_string(),
        log_file: inp.log_file.display().to_string(),
        window_width: win_w,
        window_height: win_h,
        physical_width: phys_w,
        physical_height: phys_h,
        input_port: inp.input_port,
        renderer_lib: renderer_lib.to_string(),
        egl_lib,
        env,
        argv: mobile_prefix,
    })
}

fn push_log_line(
    logs_buf: &Arc<Mutex<VecDeque<String>>>,
    events_tx: &mpsc::UnboundedSender<LauncherEvent>,
    key: &InstanceKey,
    line: String,
) {
    {
        let mut guard = logs_buf.lock().unwrap_or_else(|e| e.into_inner());
        if guard.len() >= MAX_LOG_LINES_PER_INSTANCE {
            guard.pop_front();
        }
        guard.push_back(line.clone());
    }
    let _ = events_tx.send(LauncherEvent::LogLine {
        key: key.clone(),
        line,
    });
}

#[allow(clippy::too_many_arguments)]
pub async fn run_android_jvm_session<F>(
    paths: &AppPaths,
    db: &Arc<Database>,
    profile: &GameProfile,
    account: &Account,
    access_token: &str,
    vanilla: &PreparedVanillaAssets,
    java_bin: &Path,
    java_major: u32,
    main_class: &str,
    include_client_jar: bool,
    loader_jvm_args: &[String],
    loader_game_args: &[String],
    legacy_mc_args_override: Option<&str>,
    authlib_jvm_args: &[String],
    quick_play_server: Option<&str>,
    renderer: MobileRendererMode,
    render_scale_pct: u32,
    surface_size: Option<(u32, u32)>,
    key: InstanceKey,
    cancel: CancellationToken,
    logs_buf: Arc<Mutex<VecDeque<String>>>,
    events_tx: mpsc::UnboundedSender<LauncherEvent>,
    on_started: F,
) -> ThkResult<(i32, u64, Option<CrashAnalysisReport>)>
where
    F: FnOnce(u32, Instant, MobileSessionHandle),
{
    let bridge_dir = android_bridge_dir().ok_or_else(|| {
        ThkError::Process("Thư mục cầu nối Android (.thk_bridge) chưa sẵn sàng.".into())
    })?;
    let runtime_root = paths.runtimes_dir.join("mobile_runtime");
    if !runtime_root.join("version.txt").is_file() {
        let _ = android_bridge_send_cmd(&json!({ "op": "ensure_mobile_runtime" }));
        let deadline = Instant::now() + Duration::from_secs(15);
        while !runtime_root.join("version.txt").is_file() {
            if cancel.is_cancelled() {
                return Err(ThkError::Cancelled);
            }
            if Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| ThkError::Process(format!("Không thể mở cổng điều khiển nội bộ: {e}")))?;
    let input_port = listener
        .local_addr()
        .map_err(|e| ThkError::Process(format!("Không đọc được cổng điều khiển nội bộ: {e}")))?
        .port();

    let log_file = bridge_dir.join("mobile_game.log");
    let _ = fs::write(&log_file, b"");

    let perm = thk_core::read_android_permission_status();
    let resolved_surface = surface_size
        .filter(|&(w, h)| w >= 320 && h >= 240)
        .or(if perm.surface_width >= 320 && perm.surface_height >= 240 {
            Some((perm.surface_width, perm.surface_height))
        } else {
            None
        })
        .unwrap_or((1920, 1080));

    let spec = build_mobile_launch_spec(MobileSpecInputs {
        paths,
        profile,
        account,
        access_token,
        vanilla,
        java_bin,
        java_major,
        main_class,
        include_client_jar,
        loader_jvm_args,
        loader_game_args,
        legacy_mc_args_override,
        authlib_jvm_args,
        quick_play_server,
        renderer,
        render_scale_pct,
        surface_size: resolved_surface,
        input_port,
        runtime_root: &runtime_root,
        log_file: &log_file,
    })?;

    let spec_tmp = bridge_dir.join("mobile_launch_spec.tmp");
    let spec_path = bridge_dir.join("mobile_launch_spec.json");
    let spec_bytes = serde_json::to_vec_pretty(&spec)?;
    fs::write(&spec_tmp, &spec_bytes)?;
    fs::rename(&spec_tmp, &spec_path)?;

    push_log_line(
        &logs_buf,
        &events_tx,
        &key,
        format!(
            "[THK] Khởi tạo khung hình Android {}x{} (LWJGL {}, Renderer {})",
            spec.window_width, spec.window_height, spec.lwjgl_version, spec.renderer_lib
        ),
    );

    android_bridge_send_cmd(&json!({
        "op": "launch_mobile_game",
        "session_id": spec.session_id,
        "spec_path": spec_path.display().to_string(),
        "window_width": spec.window_width,
        "window_height": spec.window_height,
    }))?;

    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<MobileInputEvent>();
    let grabbing = Arc::new(AtomicBool::new(false));
    let session_handle = MobileSessionHandle::new(
        input_tx,
        Arc::clone(&grabbing),
        (spec.window_width, spec.window_height),
    );

    let log_stop = CancellationToken::new();
    let log_tail_handle = {
        let log_stop_clone = log_stop.clone();
        let log_path = log_file.clone();
        let logs_clone = Arc::clone(&logs_buf);
        let tx_clone = events_tx.clone();
        let key_clone = key.clone();
        tokio::spawn(async move {
            let mut offset = 0usize;
            let mut carry = String::new();
            loop {
                if let Ok(bytes) = tokio::fs::read(&log_path).await {
                    if bytes.len() > offset {
                        let chunk = String::from_utf8_lossy(&bytes[offset..]);
                        offset = bytes.len();
                        carry.push_str(&chunk);
                        while let Some(nl) = carry.find('\n') {
                            let line = carry[..nl].trim_end_matches('\r').to_string();
                            carry.drain(..=nl);
                            push_log_line(&logs_clone, &tx_clone, &key_clone, line);
                        }
                    }
                }
                tokio::select! {
                    _ = log_stop_clone.cancelled() => {
                        if let Ok(bytes) = tokio::fs::read(&log_path).await {
                            if bytes.len() > offset {
                                carry.push_str(&String::from_utf8_lossy(&bytes[offset..]));
                            }
                        }
                        for tail in carry.lines() {
                            let clean = tail.trim_end_matches('\r').to_string();
                            if !clean.is_empty() {
                                push_log_line(&logs_clone, &tx_clone, &key_clone, clean);
                            }
                        }
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(80)) => {}
                }
            }
        })
    };

    let accept_res = tokio::select! {
        _ = cancel.cancelled() => {
            let _ = android_bridge_send_cmd(&json!({ "op": "stop_mobile_game" }));
            log_stop.cancel();
            let _ = log_tail_handle.await;
            return Err(ThkError::Cancelled);
        }
        res = tokio::time::timeout(Duration::from_secs(45), listener.accept()) => res,
    };

    let (stream, _) = match accept_res {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => {
            let _ = android_bridge_send_cmd(&json!({ "op": "stop_mobile_game" }));
            log_stop.cancel();
            let _ = log_tail_handle.await;
            return Err(ThkError::Process(format!(
                "Lỗi kết nối tới tiến trình Game Android: {e}"
            )));
        }
        Err(_) => {
            let _ = android_bridge_send_cmd(&json!({ "op": "stop_mobile_game" }));
            log_stop.cancel();
            let _ = log_tail_handle.await;
            return Err(ThkError::Process(
                "Quá thời gian chờ tiến trình Game Android khởi tạo bề mặt đồ họa.".into(),
            ));
        }
    };

    let _ = stream.set_nodelay(true);
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

    let started_at_wall = SystemTime::now();
    let started_at = Instant::now();
    let mut reported_started = false;

    let writer_stop = CancellationToken::new();
    let writer_stop_clone = writer_stop.clone();
    let writer_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = writer_stop_clone.cancelled() => break,
                maybe_ev = input_rx.recv() => {
                    let Some(ev) = maybe_ev else {
                        break;
                    };
                    let wire = ev.to_wire_line();
                    if write_half.write_all(wire.as_bytes()).await.is_err() {
                        break;
                    }
                    if matches!(ev, MobileInputEvent::Quit) {
                        let _ = write_half.flush().await;
                        break;
                    }
                }
            }
        }
    });

    let mut on_started_opt = Some(on_started);
    let mut exit_code: i32 = 0;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                session_handle.send(MobileInputEvent::Quit);
                let _ = android_bridge_send_cmd(&json!({ "op": "stop_mobile_game" }));
                exit_code = -1;
                break;
            }
            line_res = lines.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        let trimmed = line.trim();
                        if let Some(pid_str) = trimmed.strip_prefix("PID ") {
                            let pid = pid_str.trim().parse::<u32>().unwrap_or(0);
                            if let Some(cb) = on_started_opt.take() {
                                cb(pid, started_at, session_handle.clone());
                                reported_started = true;
                                let _ = events_tx.send(LauncherEvent::Started {
                                    key: key.clone(),
                                    pid,
                                });
                            }
                        } else if let Some(grab_str) = trimmed.strip_prefix("GRAB ") {
                            let is_grab = grab_str.trim() == "1" || grab_str.trim().eq_ignore_ascii_case("true");
                            grabbing.store(is_grab, Ordering::Relaxed);
                        } else if let Some(code_str) = trimmed.strip_prefix("EXIT ") {
                            exit_code = code_str.trim().parse::<i32>().unwrap_or(0);
                            break;
                        }
                    }
                    Ok(None) => {
                        break;
                    }
                    Err(_) => {
                        exit_code = -1;
                        break;
                    }
                }
            }
        }
    }

    if !reported_started {
        if let Some(cb) = on_started_opt.take() {
            cb(0, started_at, session_handle);
        }
    }

    writer_stop.cancel();
    let _ = writer_task.await;
    let _ = android_bridge_send_cmd(&json!({ "op": "stop_mobile_game" }));

    tokio::time::sleep(Duration::from_millis(120)).await;
    log_stop.cancel();
    let _ = log_tail_handle.await;

    let elapsed_secs = started_at.elapsed().as_secs();
    let crashed = exit_code != 0 && !cancel.is_cancelled();
    let _ = db.record_play_session(&profile.id, elapsed_secs, crashed);

    let collected_logs: Vec<String> = {
        let l = logs_buf.lock().unwrap_or_else(|e| e.into_inner());
        l.iter().cloned().collect()
    };

    let crash_report = if crashed {
        Some(analyze_crash_logs_with_instance_dir(
            &collected_logs,
            exit_code,
            Some(&profile.instance_path),
            Some(started_at_wall),
        ))
    } else {
        None
    };

    Ok((exit_code, elapsed_secs, crash_report))
}
