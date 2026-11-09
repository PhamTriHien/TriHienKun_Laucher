use crate::bionic::{
    bionic_update_ld_library_path, get_process_env, invoke_exit_hook_init, set_process_env,
};
use crate::jni_helpers::{
    call_boolean_method, call_static_boolean_method, call_static_int_method,
    call_static_void_method, clear_pending_exception, delete_global_ref, delete_local_ref,
    find_global_class, get_java_vm, get_method_id, get_static_field_id, get_static_int_field,
    get_static_method_id, is_env_valid, jval_bool, jval_char, jval_double, jval_float, jval_int,
    jval_obj, new_global_ref, new_jni_string, set_static_float_field, set_static_int_field,
    strings_to_jobject_array, AttachedThreadGuard,
};
use crate::protocol::{
    compute_scaled_window_size, parse_input_wire_line, GlfwModifierState, MobileLaunchSpecJson,
    ParsedInputCmd,
};
use crate::runtime_installer::ensure_mobile_runtime_extracted;
use jni_sys::{jclass, jfieldID, jmethodID, jobject, JNIEnv, JavaVM};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const SURFACE_WAIT_TIMEOUT: Duration = Duration::from_millis(15_000);
const ORIENTATION_SETTLE_TIMEOUT: Duration = Duration::from_millis(2_500);

const BRIDGE_PREREQ_LIBS: [&str; 2] = ["libc++_shared.so", "libspirv-cross-c-shared.so"];

const OPENJDK_CORE_LIBS: [&str; 9] = [
    "libverify.so",
    "libjava.so",
    "libnet.so",
    "libnio.so",
    "libzip.so",
    "libjimage.so",
    "libprefs.so",
    "libmanagement.so",
    "libawt.so",
];

const LWJGL_ORDERED_LIBS: [&str; 9] = [
    "liblwjgl.so",
    "liblwjgl_opengl.so",
    "liblwjgl_stb.so",
    "liblwjgl_tinyfd.so",
    "liblwjgl_nanovg.so",
    "liblwjgl_vma.so",
    "liblwjgl_spvc.so",
    "liblwjgl_spng.so",
    "liblwjgl_freetype.so",
];

#[derive(Clone, Copy)]
struct GameClassRefs {
    vm: *mut JavaVM,
    surface_cls: jclass,
    surface_is_valid: jmethodID,
    jre_utils_cls: jclass,
    load_pojav_exec: jmethodID,
    load_pojav_awt: jmethodID,
    dlopen: jmethodID,
    chdir: jmethodID,
    setup_bridge_window: jmethodID,
    release_bridge_window: jmethodID,
    setup_exit_method: jmethodID,
    logger_cls: jclass,
    cb_cls: jclass,
    vm_launcher_cls: jclass,
    string_cls: jclass,
}

unsafe impl Send for GameClassRefs {}
unsafe impl Sync for GameClassRefs {}

#[derive(Clone, Copy)]
struct GameRuntimeHandles {
    logger_begin: jmethodID,
    logger_append: jmethodID,
    cb_attach_context: jmethodID,
    f_window_width: jfieldID,
    f_window_height: jfieldID,
    f_physical_width: jfieldID,
    f_physical_height: jfieldID,
    f_mouse_x: jfieldID,
    f_mouse_y: jfieldID,
    cb_set_use_input_stack_queue: jmethodID,
    cb_send_char: jmethodID,
    cb_send_char_mods: jmethodID,
    cb_send_key: jmethodID,
    cb_send_cursor_pos: jmethodID,
    cb_send_mouse_button: jmethodID,
    cb_send_scroll: jmethodID,
    cb_send_screen_size: jmethodID,
    launch_jvm: jmethodID,
}

unsafe impl Send for GameRuntimeHandles {}
unsafe impl Sync for GameRuntimeHandles {}

struct SurfaceSlot {
    surface_ref: jobject,
    width: i32,
    height: i32,
    bridge_window_ready: bool,
}

unsafe impl Send for SurfaceSlot {}

struct JvmBootstrapArgs {
    app_context_ref: jobject,
    files_dir: PathBuf,
    apk_source_dir: PathBuf,
    spec_path: PathBuf,
    dns_servers: Vec<String>,
}

unsafe impl Send for JvmBootstrapArgs {}

static GAME_CLASSES: OnceLock<GameClassRefs> = OnceLock::new();
static GAME_RUNTIME: OnceLock<GameRuntimeHandles> = OnceLock::new();
static SURFACE_SYNC: OnceLock<(Mutex<SurfaceSlot>, Condvar)> = OnceLock::new();
static CONTROL_WRITER: Mutex<Option<TcpStream>> = Mutex::new(None);
static BOOT_LOG_LOCK: Mutex<()> = Mutex::new(());

static JVM_STARTED: AtomicBool = AtomicBool::new(false);
static BRIDGE_LOADED: AtomicBool = AtomicBool::new(false);
static LOGGER_ACTIVE: AtomicBool = AtomicBool::new(false);
static EXITING: AtomicBool = AtomicBool::new(false);

fn surface_sync() -> &'static (Mutex<SurfaceSlot>, Condvar) {
    SURFACE_SYNC.get_or_init(|| {
        (
            Mutex::new(SurfaceSlot {
                surface_ref: std::ptr::null_mut(),
                width: 0,
                height: 0,
                bridge_window_ready: false,
            }),
            Condvar::new(),
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn start_game_jvm(
    env: *mut JNIEnv,
    app_context: jobject,
    files_dir: String,
    apk_source_dir: String,
    spec_path: String,
    initial_surface: jobject,
    width: i32,
    height: i32,
    dns_servers: Vec<String>,
) {
    if !is_env_valid(env) || app_context.is_null() {
        return;
    }

    if ensure_game_class_refs(env).is_none() {
        let fallback_log = Path::new(files_dir.trim())
            .join(".thk_bridge")
            .join("mobile_game.log");
        append_bootstrap_log(
            None,
            Some(&fallback_log),
            "Failed resolving core Android/Pojav JNI classes on GameJvmService thread",
        );
        notify_jvm_exit(Some(env), 1);
        return;
    }

    update_game_surface(env, initial_surface, width, height);

    let trimmed_spec = spec_path.trim();
    if trimmed_spec.is_empty() || JVM_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }

    let Some(app_context_ref) = new_global_ref(env, app_context) else {
        JVM_STARTED.store(false, Ordering::Release);
        return;
    };

    let boot_args = JvmBootstrapArgs {
        app_context_ref,
        files_dir: PathBuf::from(files_dir.trim()),
        apk_source_dir: PathBuf::from(apk_source_dir.trim()),
        spec_path: PathBuf::from(trimmed_spec),
        dns_servers,
    };

    if thread::Builder::new()
        .name("thk-jvm-main".to_string())
        .spawn(move || run_jvm_main_thread(boot_args))
        .is_err()
    {
        delete_global_ref(env, app_context_ref);
        JVM_STARTED.store(false, Ordering::Release);
        notify_jvm_exit(Some(env), 1);
    }
}

pub(crate) unsafe fn update_game_surface(
    env: *mut JNIEnv,
    incoming: jobject,
    width: i32,
    height: i32,
) {
    if !is_env_valid(env) {
        return;
    }
    let (lock, cvar) = surface_sync();
    let mut slot = lock.lock().unwrap_or_else(|e| e.into_inner());

    if !slot.surface_ref.is_null() {
        delete_global_ref(env, slot.surface_ref);
        slot.surface_ref = std::ptr::null_mut();
    }
    if !incoming.is_null() {
        if let Some(gref) = new_global_ref(env, incoming) {
            slot.surface_ref = gref;
        }
    }
    if width > 0 && height > 0 {
        slot.width = width;
        slot.height = height;
    }
    cvar.notify_all();

    if slot.bridge_window_ready
        && BRIDGE_LOADED.load(Ordering::Acquire)
        && !slot.surface_ref.is_null()
        && is_surface_valid(env, slot.surface_ref)
    {
        if let Some(classes) = GAME_CLASSES.get() {
            let _ = call_static_void_method(
                env,
                classes.jre_utils_cls,
                classes.setup_bridge_window,
                &[jval_obj(slot.surface_ref)],
            );
            if width > 0 && height > 0 {
                if let Some(rt) = GAME_RUNTIME.get() {
                    let cur_phys_w = get_static_int_field(env, classes.cb_cls, rt.f_physical_width);
                    let cur_phys_h =
                        get_static_int_field(env, classes.cb_cls, rt.f_physical_height);
                    if cur_phys_w != width || cur_phys_h != height {
                        let cur_win_w =
                            get_static_int_field(env, classes.cb_cls, rt.f_window_width);
                        let (new_win_w, new_win_h) =
                            compute_scaled_window_size(cur_win_w, cur_phys_w, width, height);
                        set_static_int_field(env, classes.cb_cls, rt.f_physical_width, width);
                        set_static_int_field(env, classes.cb_cls, rt.f_physical_height, height);
                        set_static_int_field(env, classes.cb_cls, rt.f_window_width, new_win_w);
                        set_static_int_field(env, classes.cb_cls, rt.f_window_height, new_win_h);
                        let _ = call_static_void_method(
                            env,
                            classes.cb_cls,
                            rt.cb_send_screen_size,
                            &[jval_int(new_win_w), jval_int(new_win_h)],
                        );
                    }
                }
            }
            drop(slot);
            append_bootstrap_log(
                Some(env),
                None,
                &format!("late surface bound (valid=true, phys={width}x{height})"),
            );
        }
    }
}

pub(crate) unsafe fn stop_game_jvm(env: *mut JNIEnv) {
    notify_jvm_exit(Some(env), 0);
}

pub fn notify_grab_state(grabbing: bool) {
    send_control_line(if grabbing { "GRAB 1" } else { "GRAB 0" });
}

pub(crate) unsafe fn notify_jvm_exit(env_hint: Option<*mut JNIEnv>, code: i32) {
    if EXITING.swap(true, Ordering::AcqRel) {
        return;
    }

    send_control_line(&format!("EXIT {code}"));

    if let Ok(mut guard) = CONTROL_WRITER.lock() {
        if let Some(mut stream) = guard.take() {
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Both);
        }
    }

    if BRIDGE_LOADED.load(Ordering::Acquire) {
        if let Some(classes) = GAME_CLASSES.get() {
            if let Some(env) = env_hint.filter(|&e| is_env_valid(e)) {
                let _ = call_static_void_method(
                    env,
                    classes.jre_utils_cls,
                    classes.release_bridge_window,
                    &[],
                );
            } else if let Some(attached) =
                AttachedThreadGuard::attach(classes.vm, c"thk-jvm-exit", true)
            {
                let _ = call_static_void_method(
                    attached.env(),
                    classes.jre_utils_cls,
                    classes.release_bridge_window,
                    &[],
                );
            }
        }
    }

    let _ = libc::kill(libc::getpid(), libc::SIGKILL);
}

unsafe fn ensure_game_class_refs(env: *mut JNIEnv) -> Option<GameClassRefs> {
    if let Some(&cached) = GAME_CLASSES.get() {
        return Some(cached);
    }

    let vm = get_java_vm(env)?;
    let surface_cls = find_global_class(env, c"android/view/Surface")?;
    let surface_is_valid = get_method_id(env, surface_cls, c"isValid", c"()Z")?;

    let jre_utils_cls = find_global_class(env, c"net/kdt/pojavlaunch/utils/JREUtils")?;
    let load_pojav_exec = get_static_method_id(
        env,
        jre_utils_cls,
        c"loadPojavExec",
        c"(Ljava/lang/String;)V",
    )?;
    let load_pojav_awt = get_static_method_id(
        env,
        jre_utils_cls,
        c"loadPojavAwt",
        c"(Ljava/lang/String;)V",
    )?;
    let dlopen = get_static_method_id(env, jre_utils_cls, c"dlopen", c"(Ljava/lang/String;)Z")?;
    let chdir = get_static_method_id(env, jre_utils_cls, c"chdir", c"(Ljava/lang/String;)I")?;
    let setup_bridge_window = get_static_method_id(
        env,
        jre_utils_cls,
        c"setupBridgeWindow",
        c"(Ljava/lang/Object;)V",
    )?;
    let release_bridge_window =
        get_static_method_id(env, jre_utils_cls, c"releaseBridgeWindow", c"()V")?;
    let setup_exit_method = get_static_method_id(
        env,
        jre_utils_cls,
        c"setupExitMethod",
        c"(Landroid/content/Context;)V",
    )?;

    let logger_cls = find_global_class(env, c"net/kdt/pojavlaunch/Logger")?;
    let cb_cls = find_global_class(env, c"org/lwjgl/glfw/CallbackBridge")?;
    let vm_launcher_cls = find_global_class(env, c"com/oracle/dalvik/VMLauncher")?;
    let string_cls = find_global_class(env, c"java/lang/String")?;

    let refs = GameClassRefs {
        vm,
        surface_cls,
        surface_is_valid,
        jre_utils_cls,
        load_pojav_exec,
        load_pojav_awt,
        dlopen,
        chdir,
        setup_bridge_window,
        release_bridge_window,
        setup_exit_method,
        logger_cls,
        cb_cls,
        vm_launcher_cls,
        string_cls,
    };
    let _ = GAME_CLASSES.set(refs);
    GAME_CLASSES.get().copied()
}

unsafe fn ensure_game_runtime_handles(
    env: *mut JNIEnv,
    classes: &GameClassRefs,
) -> Option<GameRuntimeHandles> {
    if let Some(&cached) = GAME_RUNTIME.get() {
        return Some(cached);
    }

    let handles = GameRuntimeHandles {
        logger_begin: get_static_method_id(
            env,
            classes.logger_cls,
            c"begin",
            c"(Ljava/lang/String;)V",
        )?,
        logger_append: get_static_method_id(
            env,
            classes.logger_cls,
            c"appendToLog",
            c"(Ljava/lang/String;)V",
        )?,
        cb_attach_context: get_static_method_id(
            env,
            classes.cb_cls,
            c"attachContext",
            c"(Landroid/content/Context;)V",
        )?,
        f_window_width: get_static_field_id(env, classes.cb_cls, c"windowWidth", c"I")?,
        f_window_height: get_static_field_id(env, classes.cb_cls, c"windowHeight", c"I")?,
        f_physical_width: get_static_field_id(env, classes.cb_cls, c"physicalWidth", c"I")?,
        f_physical_height: get_static_field_id(env, classes.cb_cls, c"physicalHeight", c"I")?,
        f_mouse_x: get_static_field_id(env, classes.cb_cls, c"mouseX", c"F")?,
        f_mouse_y: get_static_field_id(env, classes.cb_cls, c"mouseY", c"F")?,
        cb_set_use_input_stack_queue: get_static_method_id(
            env,
            classes.cb_cls,
            c"nativeSetUseInputStackQueue",
            c"(Z)V",
        )?,
        cb_send_char: get_static_method_id(env, classes.cb_cls, c"nativeSendChar", c"(C)Z")?,
        cb_send_char_mods: get_static_method_id(
            env,
            classes.cb_cls,
            c"nativeSendCharMods",
            c"(CI)Z",
        )?,
        cb_send_key: get_static_method_id(env, classes.cb_cls, c"nativeSendKey", c"(IIII)V")?,
        cb_send_cursor_pos: get_static_method_id(
            env,
            classes.cb_cls,
            c"nativeSendCursorPos",
            c"(FF)V",
        )?,
        cb_send_mouse_button: get_static_method_id(
            env,
            classes.cb_cls,
            c"nativeSendMouseButton",
            c"(III)V",
        )?,
        cb_send_scroll: get_static_method_id(env, classes.cb_cls, c"nativeSendScroll", c"(DD)V")?,
        cb_send_screen_size: get_static_method_id(
            env,
            classes.cb_cls,
            c"nativeSendScreenSize",
            c"(II)V",
        )?,
        launch_jvm: get_static_method_id(
            env,
            classes.vm_launcher_cls,
            c"launchJVM",
            c"([Ljava/lang/String;)I",
        )?,
    };

    let _ = GAME_RUNTIME.set(handles);
    GAME_RUNTIME.get().copied()
}

fn run_jvm_main_thread(args: JvmBootstrapArgs) {
    let mut log_path = args.files_dir.join(".thk_bridge").join("mobile_game.log");
    let pid = unsafe { libc::getpid() };
    append_bootstrap_log(
        None,
        Some(&log_path),
        &format!("bootstrap start (pid={pid})"),
    );

    let Some(classes) = GAME_CLASSES.get().copied() else {
        append_bootstrap_log(
            None,
            Some(&log_path),
            "GAME_CLASSES missing in thk-jvm-main",
        );
        unsafe { notify_jvm_exit(None, 1) };
        return;
    };

    let Some(guard) = (unsafe { AttachedThreadGuard::attach(classes.vm, c"thk-jvm-main", false) })
    else {
        append_bootstrap_log(
            None,
            Some(&log_path),
            "Failed attaching thk-jvm-main thread to Android JavaVM",
        );
        unsafe { notify_jvm_exit(None, 1) };
        return;
    };
    let env = guard.env();

    let exit_code = match unsafe { execute_jvm_bootstrap(env, &classes, &args, &mut log_path, pid) }
    {
        Ok(rc) => rc,
        Err((code, err_msg)) => {
            append_bootstrap_log(
                Some(env),
                Some(&log_path),
                &format!("Android JVM bootstrap error: {err_msg}"),
            );
            code
        }
    };

    unsafe {
        notify_jvm_exit(Some(env), exit_code);
    }
}

unsafe fn execute_jvm_bootstrap(
    env: *mut JNIEnv,
    classes: &GameClassRefs,
    args: &JvmBootstrapArgs,
    log_path: &mut PathBuf,
    pid: i32,
) -> Result<i32, (i32, String)> {
    ensure_mobile_runtime_extracted(&args.files_dir, &args.apk_source_dir, &args.dns_servers)
        .map_err(|e| (1, e))?;

    let raw_spec = fs::read_to_string(&args.spec_path).unwrap_or_default();
    if raw_spec.trim().is_empty() {
        return Err((
            2,
            format!("empty mobile launch spec at {}", args.spec_path.display()),
        ));
    }

    let spec: MobileLaunchSpecJson = serde_json::from_str(&raw_spec)
        .map_err(|e| (1, format!("invalid mobile launch spec JSON: {e}")))?;

    if !spec.log_file.trim().is_empty() {
        *log_path = PathBuf::from(spec.log_file.trim());
    }

    let mut window_w = spec.window_width.max(320);
    let mut window_h = spec.window_height.max(240);
    let mut phys_w = spec.physical_width.unwrap_or(window_w).max(320);
    let mut phys_h = spec.physical_height.unwrap_or(window_h).max(240);

    let sock = TcpStream::connect(("127.0.0.1", spec.input_port)).map_err(|e| {
        (
            1,
            format!(
                "failed connecting control socket 127.0.0.1:{}: {e}",
                spec.input_port
            ),
        )
    })?;
    let _ = sock.set_nodelay(true);
    let rx_sock = sock
        .try_clone()
        .map_err(|e| (1, format!("failed cloning control socket: {e}")))?;
    {
        let mut writer = CONTROL_WRITER.lock().unwrap_or_else(|e| e.into_inner());
        *writer = Some(sock);
    }
    send_control_line(&format!("PID {pid}"));

    let mut configured_ld_path = String::new();
    for (k, v) in &spec.env {
        if k == "LD_LIBRARY_PATH" {
            configured_ld_path = v.clone();
        }
        set_process_env(k, v);
    }

    let bridge_lib_dir = Path::new(&spec.runtime_root).join("lib");
    append_bootstrap_log(
        Some(env),
        Some(log_path),
        &format!("loading bridge libraries from {}", bridge_lib_dir.display()),
    );

    let pojav_exec = bridge_lib_dir.join("libpojavexec.so");
    if !call_jre_string_void(
        env,
        classes.jre_utils_cls,
        classes.load_pojav_exec,
        &pojav_exec.to_string_lossy(),
    ) {
        return Err((
            1,
            format!("JREUtils.loadPojavExec failed for {}", pojav_exec.display()),
        ));
    }

    let base_ld = get_process_env("LD_LIBRARY_PATH")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "{}:/system/lib64:/vendor/lib64:/vendor/lib64/hw",
                bridge_lib_dir.display()
            )
        });
    set_process_env("LD_LIBRARY_PATH", &base_ld);
    let _ = bionic_update_ld_library_path(&base_ld);

    let byte_hook = bridge_lib_dir.join("libbytehook.so");
    if byte_hook.is_file() {
        let _ = call_jre_dlopen(env, classes, &byte_hook);
    }

    let mut exit_hook_path: Option<String> = None;
    let exit_hook = bridge_lib_dir.join("libexithook.so");
    if exit_hook.is_file() && call_jre_dlopen(env, classes, &exit_hook) {
        exit_hook_path = Some(exit_hook.to_string_lossy().into_owned());
    }

    let pojav_awt = bridge_lib_dir.join("libpojavexec_awt.so");
    if pojav_awt.is_file()
        && !call_jre_string_void(
            env,
            classes.jre_utils_cls,
            classes.load_pojav_awt,
            &pojav_awt.to_string_lossy(),
        )
    {
        return Err((
            1,
            format!("JREUtils.loadPojavAwt failed for {}", pojav_awt.display()),
        ));
    }

    if !configured_ld_path.is_empty() {
        let _ = bionic_update_ld_library_path(&configured_ld_path);
    }

    let rt = ensure_game_runtime_handles(env, classes).ok_or_else(|| {
        (
            1,
            "failed resolving Logger/CallbackBridge/VMLauncher JNI handles".to_string(),
        )
    })?;
    BRIDGE_LOADED.store(true, Ordering::Release);

    let _ = call_jre_string_void(
        env,
        classes.logger_cls,
        rt.logger_begin,
        &log_path.to_string_lossy(),
    );
    LOGGER_ACTIVE.store(true, Ordering::Release);
    append_bootstrap_log(
        Some(env),
        Some(log_path),
        &format!("bootstrap start (pid={pid})"),
    );
    append_bootstrap_log(Some(env), Some(log_path), "bridge loaded");

    let _ = call_static_void_method(
        env,
        classes.cb_cls,
        rt.cb_attach_context,
        &[jval_obj(args.app_context_ref)],
    );
    set_static_int_field(env, classes.cb_cls, rt.f_window_width, window_w);
    set_static_int_field(env, classes.cb_cls, rt.f_window_height, window_h);
    set_static_int_field(env, classes.cb_cls, rt.f_physical_width, phys_w);
    set_static_int_field(env, classes.cb_cls, rt.f_physical_height, phys_h);
    notify_grab_state(false);

    let rx_classes = *classes;
    let rx_rt = rt;
    let _ = thread::Builder::new()
        .name("thk-input-rx".to_string())
        .spawn(move || run_input_receiver_loop(rx_sock, rx_classes, rx_rt));

    load_openjdk_and_graphics_libraries(env, classes, &spec, &bridge_lib_dir);
    append_bootstrap_log(Some(env), Some(log_path), "openjdk loaded");

    load_optional_bridge_so(
        env,
        classes,
        &bridge_lib_dir,
        spec.egl_lib.as_deref().unwrap_or(""),
    );
    load_optional_bridge_so(env, classes, &bridge_lib_dir, &spec.renderer_lib);

    let surface_valid_now = bind_surface_with_wait(
        env,
        classes,
        &rt,
        &mut window_w,
        &mut window_h,
        &mut phys_w,
        &mut phys_h,
    );
    append_bootstrap_log(
        Some(env),
        Some(log_path),
        &format!(
            "surface bound (valid={surface_valid_now}, window={window_w}x{window_h}, phys={phys_w}x{phys_h})"
        ),
    );

    let _ = call_static_void_method(
        env,
        classes.cb_cls,
        rt.cb_set_use_input_stack_queue,
        &[jval_bool(true)],
    );
    let _ = call_static_void_method(
        env,
        classes.jre_utils_cls,
        classes.setup_exit_method,
        &[jval_obj(args.app_context_ref)],
    );
    if let Some(ref hook_path) = exit_hook_path {
        let _ = invoke_exit_hook_init(env, classes.jre_utils_cls, hook_path);
    }
    let _ = call_jre_chdir(env, classes, &spec.instance_dir);

    let argv_array = strings_to_jobject_array(env, classes.string_cls, &spec.argv);
    if argv_array.is_null() {
        return Err((1, "failed constructing JVM String[] argv".to_string()));
    }

    append_bootstrap_log(
        Some(env),
        Some(log_path),
        &format!("VMLauncher.launchJVM (argc={})", spec.argv.len()),
    );
    let rc = call_static_int_method(
        env,
        classes.vm_launcher_cls,
        rt.launch_jvm,
        &[jval_obj(argv_array)],
    )
    .unwrap_or(1);
    delete_local_ref(env, argv_array);

    append_bootstrap_log(
        Some(env),
        Some(log_path),
        &format!("VMLauncher.launchJVM exited with code {rc}"),
    );
    Ok(rc)
}

unsafe fn bind_surface_with_wait(
    env: *mut JNIEnv,
    classes: &GameClassRefs,
    rt: &GameRuntimeHandles,
    window_w: &mut i32,
    window_h: &mut i32,
    phys_w: &mut i32,
    phys_h: &mut i32,
) -> bool {
    let total_deadline = Instant::now() + SURFACE_WAIT_TIMEOUT;
    let (lock, cvar) = surface_sync();
    let mut slot = lock.lock().unwrap_or_else(|e| e.into_inner());

    while slot.surface_ref.is_null() || !is_surface_valid(env, slot.surface_ref) {
        let now = Instant::now();
        if now >= total_deadline {
            break;
        }
        let step = (total_deadline - now).min(Duration::from_millis(100));
        let (next_slot, _) = cvar
            .wait_timeout(slot, step)
            .unwrap_or_else(|e| e.into_inner());
        slot = next_slot;
    }

    if !slot.surface_ref.is_null()
        && is_surface_valid(env, slot.surface_ref)
        && *window_w != *window_h
    {
        let want_landscape = *window_w > *window_h;
        let orient_deadline = total_deadline.min(Instant::now() + ORIENTATION_SETTLE_TIMEOUT);
        while slot.width > 0 && slot.height > 0 && (slot.width >= slot.height) != want_landscape {
            let now = Instant::now();
            if now >= orient_deadline {
                break;
            }
            let step = (orient_deadline - now).min(Duration::from_millis(80));
            let (next_slot, _) = cvar
                .wait_timeout(slot, step)
                .unwrap_or_else(|e| e.into_inner());
            slot = next_slot;
        }
    }

    if slot.width > 0 && slot.height > 0 {
        let (scaled_w, scaled_h) =
            compute_scaled_window_size(*window_w, *phys_w, slot.width, slot.height);
        *phys_w = slot.width;
        *phys_h = slot.height;
        *window_w = scaled_w;
        *window_h = scaled_h;
        set_static_int_field(env, classes.cb_cls, rt.f_physical_width, *phys_w);
        set_static_int_field(env, classes.cb_cls, rt.f_physical_height, *phys_h);
        set_static_int_field(env, classes.cb_cls, rt.f_window_width, *window_w);
        set_static_int_field(env, classes.cb_cls, rt.f_window_height, *window_h);
    }

    slot.bridge_window_ready = true;
    let mut valid_now = false;
    if !slot.surface_ref.is_null() && is_surface_valid(env, slot.surface_ref) {
        let _ = call_static_void_method(
            env,
            classes.jre_utils_cls,
            classes.setup_bridge_window,
            &[jval_obj(slot.surface_ref)],
        );
        valid_now = true;
    }

    valid_now
}

fn run_input_receiver_loop(sock: TcpStream, classes: GameClassRefs, rt: GameRuntimeHandles) {
    let Some(guard) = (unsafe { AttachedThreadGuard::attach(classes.vm, c"thk-input-rx", true) })
    else {
        return;
    };
    let env = guard.env();

    let mut reader = BufReader::new(sock);
    let mut line = String::with_capacity(64);
    let mut mod_state = GlfwModifierState::new();

    loop {
        line.clear();
        let Ok(n) = reader.read_line(&mut line) else {
            break;
        };
        if n == 0 {
            break;
        }
        let Some(cmd) = parse_input_wire_line(&line) else {
            continue;
        };

        unsafe {
            match cmd {
                ParsedInputCmd::Quit => {
                    notify_jvm_exit(Some(env), 0);
                    return;
                }
                ParsedInputCmd::Key { keycode, down } => {
                    if keycode != 0 {
                        let mods = mod_state.update_key(keycode, down);
                        if BRIDGE_LOADED.load(Ordering::Acquire) {
                            let _ = call_static_void_method(
                                env,
                                classes.cb_cls,
                                rt.cb_send_key,
                                &[
                                    jval_int(keycode),
                                    jval_int(0),
                                    jval_int(if down { 1 } else { 0 }),
                                    jval_int(mods),
                                ],
                            );
                        }
                    }
                }
                ParsedInputCmd::Char { codepoint, mods } => {
                    if let Some(ch) = char::from_u32(codepoint) {
                        if BRIDGE_LOADED.load(Ordering::Acquire) {
                            let mut utf16 = [0u16; 2];
                            for unit in ch.encode_utf16(&mut utf16) {
                                let _ = call_static_boolean_method(
                                    env,
                                    classes.cb_cls,
                                    rt.cb_send_char_mods,
                                    &[jval_char(*unit), jval_int(mods)],
                                );
                                let _ = call_static_boolean_method(
                                    env,
                                    classes.cb_cls,
                                    rt.cb_send_char,
                                    &[jval_char(*unit)],
                                );
                            }
                        }
                    }
                }
                ParsedInputCmd::MouseButton { button, down } => {
                    if BRIDGE_LOADED.load(Ordering::Acquire) {
                        let _ = call_static_void_method(
                            env,
                            classes.cb_cls,
                            rt.cb_send_mouse_button,
                            &[
                                jval_int(button),
                                jval_int(if down { 1 } else { 0 }),
                                jval_int(mod_state.bits()),
                            ],
                        );
                    }
                }
                ParsedInputCmd::CursorPos { x, y } => {
                    set_static_float_field(env, classes.cb_cls, rt.f_mouse_x, x);
                    set_static_float_field(env, classes.cb_cls, rt.f_mouse_y, y);
                    if BRIDGE_LOADED.load(Ordering::Acquire) {
                        let _ = call_static_void_method(
                            env,
                            classes.cb_cls,
                            rt.cb_send_cursor_pos,
                            &[jval_float(x), jval_float(y)],
                        );
                    }
                }
                ParsedInputCmd::Scroll { dx, dy } => {
                    if BRIDGE_LOADED.load(Ordering::Acquire) {
                        let _ = call_static_void_method(
                            env,
                            classes.cb_cls,
                            rt.cb_send_scroll,
                            &[jval_double(dx), jval_double(dy)],
                        );
                    }
                }
                ParsedInputCmd::WindowSize { width, height } => {
                    set_static_int_field(env, classes.cb_cls, rt.f_window_width, width);
                    set_static_int_field(env, classes.cb_cls, rt.f_window_height, height);
                    if BRIDGE_LOADED.load(Ordering::Acquire) {
                        let _ = call_static_void_method(
                            env,
                            classes.cb_cls,
                            rt.cb_send_screen_size,
                            &[jval_int(width), jval_int(height)],
                        );
                    }
                }
            }
        }
    }
}

unsafe fn load_openjdk_and_graphics_libraries(
    env: *mut JNIEnv,
    classes: &GameClassRefs,
    spec: &MobileLaunchSpecJson,
    bridge_lib_dir: &Path,
) {
    for name in BRIDGE_PREREQ_LIBS {
        let candidate = bridge_lib_dir.join(name);
        if candidate.is_file() {
            let _ = call_jre_dlopen(env, classes, &candidate);
        }
    }

    let jre_home = Path::new(&spec.jre_home);
    let arch_lib_dir = resolve_jre_arch_lib_dir(jre_home);
    let server_dir = arch_lib_dir.join("server");
    let jvm_dir = if server_dir.is_dir() {
        server_dir
    } else {
        arch_lib_dir.join("client")
    };

    let jli_in_sub = arch_lib_dir.join("jli").join("libjli.so");
    let jli_path = if jli_in_sub.is_file() {
        jli_in_sub
    } else {
        arch_lib_dir.join("libjli.so")
    };
    if jli_path.is_file() {
        let _ = call_jre_dlopen(env, classes, &jli_path);
    }

    let jvm_path = jvm_dir.join("libjvm.so");
    if jvm_path.is_file() {
        let _ = call_jre_dlopen(env, classes, &jvm_path);
    }

    for name in OPENJDK_CORE_LIBS {
        let f = arch_lib_dir.join(name);
        if f.is_file() {
            let _ = call_jre_dlopen(env, classes, &f);
        }
    }

    let lwjgl_natives_dir = Path::new(&spec.lwjgl_natives_dir);
    let lwjgl_freetype = lwjgl_natives_dir.join("libfreetype.so");
    let has_lwjgl_freetype = !spec.lwjgl_natives_dir.trim().is_empty() && lwjgl_freetype.is_file();
    if has_lwjgl_freetype {
        let _ = call_jre_dlopen(env, classes, &lwjgl_freetype);
    } else {
        let jre_freetype = arch_lib_dir.join("libfreetype.so");
        if jre_freetype.is_file() {
            let _ = call_jre_dlopen(env, classes, &jre_freetype);
        }
    }

    let jre_headless = arch_lib_dir.join("libawt_headless.so");
    if jre_headless.is_file() {
        let _ = call_jre_dlopen(env, classes, &jre_headless);
    } else {
        let custom_headless = bridge_lib_dir.join("libawt_headless.so");
        if custom_headless.is_file() {
            let _ = call_jre_dlopen(env, classes, &custom_headless);
        }
    }

    let custom_xawt = bridge_lib_dir.join("libawt_xawt.so");
    if custom_xawt.is_file() {
        ensure_jre_awt_xawt_stub(&arch_lib_dir, &custom_xawt);
        let _ = call_jre_dlopen(env, classes, &custom_xawt);
    }

    let font_manager = arch_lib_dir.join("libfontmanager.so");
    if font_manager.is_file() {
        let _ = call_jre_dlopen(env, classes, &font_manager);
    }

    let mut jre_shared = locate_shared_libs(&arch_lib_dir);
    jre_shared.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    for so_path in jre_shared {
        let lower_name = so_path
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if lower_name == "libawt_headless.so"
            || lower_name == "libawt_xawt.so"
            || lower_name == "libjsig.so"
            || (has_lwjgl_freetype && lower_name == "libfreetype.so")
        {
            continue;
        }
        let _ = call_jre_dlopen(env, classes, &so_path);
    }

    let openal = bridge_lib_dir.join("libopenal.so");
    if openal.is_file() {
        let _ = call_jre_dlopen(env, classes, &openal);
    }

    if !spec.lwjgl_natives_dir.trim().is_empty() && lwjgl_natives_dir.is_dir() {
        for name in LWJGL_ORDERED_LIBS {
            let f = lwjgl_natives_dir.join(name);
            if f.is_file() {
                let _ = call_jre_dlopen(env, classes, &f);
            }
        }
        for so_path in locate_shared_libs(lwjgl_natives_dir) {
            let is_ft = so_path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("libfreetype.so"));
            if is_ft {
                continue;
            }
            let _ = call_jre_dlopen(env, classes, &so_path);
        }
    }
}

unsafe fn load_optional_bridge_so(
    env: *mut JNIEnv,
    classes: &GameClassRefs,
    bridge_lib_dir: &Path,
    lib_spec: &str,
) {
    let trimmed = lib_spec.trim();
    if trimmed.is_empty() {
        return;
    }
    let in_bridge = bridge_lib_dir.join(trimmed);
    if in_bridge.is_file() {
        let _ = call_jre_dlopen(env, classes, &in_bridge);
        return;
    }
    let direct = Path::new(trimmed);
    if direct.is_absolute() && direct.is_file() {
        let _ = call_jre_dlopen(env, classes, direct);
    }
}

fn resolve_jre_arch_lib_dir(jre_home: &Path) -> PathBuf {
    let aarch64 = jre_home.join("lib").join("aarch64");
    if aarch64.is_dir() {
        aarch64
    } else {
        jre_home.join("lib")
    }
}

fn locate_shared_libs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut children: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    children.sort_by(|a, b| {
        let na = a
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        let nb = b
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        na.cmp(&nb)
    });

    let mut out = Vec::new();
    for path in children {
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_file() {
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".so"))
            {
                out.push(path);
            }
        } else if meta.is_dir() {
            out.extend(locate_shared_libs(&path));
        }
    }
    out
}

fn ensure_jre_awt_xawt_stub(arch_lib_dir: &Path, custom_xawt: &Path) {
    if !arch_lib_dir.is_dir() || !custom_xawt.is_file() {
        return;
    }
    let jre_xawt = arch_lib_dir.join("libawt_xawt.so");
    if fs::symlink_metadata(&jre_xawt).is_ok() {
        if let (Ok(canon_target), Ok(canon_existing)) =
            (custom_xawt.canonicalize(), jre_xawt.canonicalize())
        {
            if canon_target == canon_existing {
                return;
            }
        }
        let _ = fs::set_permissions(&jre_xawt, fs::Permissions::from_mode(0o600));
        let _ = fs::remove_file(&jre_xawt);
    }

    if symlink(custom_xawt, &jre_xawt).is_ok() {
        return;
    }

    let tmp = arch_lib_dir.join("libawt_xawt.so.tmp");
    let copy_res = (|| -> io::Result<()> {
        let mut src = File::open(custom_xawt)?;
        let mut dst = File::create(&tmp)?;
        io::copy(&mut src, &mut dst)?;
        dst.flush()?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o500))?;
        fs::rename(&tmp, &jre_xawt)
    })();
    if copy_res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

#[inline]
unsafe fn is_surface_valid(env: *mut JNIEnv, surface: jobject) -> bool {
    let Some(classes) = GAME_CLASSES.get() else {
        return false;
    };
    if surface.is_null() || classes.surface_cls.is_null() {
        return false;
    }
    if !((**env).v1_6.IsInstanceOf)(env, surface, classes.surface_cls) {
        let _ = clear_pending_exception(env);
        return false;
    }
    if clear_pending_exception(env) {
        return false;
    }
    call_boolean_method(env, surface, classes.surface_is_valid, &[]).unwrap_or(false)
}

unsafe fn call_jre_string_void(
    env: *mut JNIEnv,
    cls: jclass,
    method: jmethodID,
    text: &str,
) -> bool {
    let j_str = new_jni_string(env, text);
    if j_str.is_null() {
        return false;
    }
    let ok = call_static_void_method(env, cls, method, &[jval_obj(j_str)]);
    delete_local_ref(env, j_str);
    ok
}

unsafe fn call_jre_dlopen(env: *mut JNIEnv, classes: &GameClassRefs, path: &Path) -> bool {
    let j_str = new_jni_string(env, &path.to_string_lossy());
    if j_str.is_null() {
        return false;
    }
    let ok = call_static_boolean_method(
        env,
        classes.jre_utils_cls,
        classes.dlopen,
        &[jval_obj(j_str)],
    )
    .unwrap_or(false);
    delete_local_ref(env, j_str);
    ok
}

unsafe fn call_jre_chdir(env: *mut JNIEnv, classes: &GameClassRefs, dir: &str) -> i32 {
    let j_str = new_jni_string(env, dir);
    if j_str.is_null() {
        return -1;
    }
    let rc = call_static_int_method(
        env,
        classes.jre_utils_cls,
        classes.chdir,
        &[jval_obj(j_str)],
    )
    .unwrap_or(-1);
    delete_local_ref(env, j_str);
    rc
}

fn send_control_line(line: &str) {
    let Ok(mut guard) = CONTROL_WRITER.lock() else {
        return;
    };
    let Some(ref mut stream) = *guard else {
        return;
    };
    let _ = stream.write_all(line.as_bytes());
    let _ = stream.write_all(b"\n");
    let _ = stream.flush();
}

fn append_bootstrap_log(env_opt: Option<*mut JNIEnv>, log_file: Option<&Path>, msg: &str) {
    if log_file.is_none() && !LOGGER_ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let _guard = BOOT_LOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let formatted = format!("[THK-BOOT] {msg}");

    if LOGGER_ACTIVE.load(Ordering::Acquire) {
        if let (Some(env), Some(classes), Some(rt)) =
            (env_opt, GAME_CLASSES.get(), GAME_RUNTIME.get())
        {
            unsafe {
                if is_env_valid(env) {
                    let _ =
                        call_jre_string_void(env, classes.logger_cls, rt.logger_append, &formatted);
                    return;
                }
            }
        }
    }

    if let Some(path) = log_file {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{formatted}");
            let _ = f.flush();
        }
    }
}
