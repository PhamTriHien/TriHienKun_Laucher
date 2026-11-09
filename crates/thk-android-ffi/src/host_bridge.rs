use crate::bionic::{
    collect_getifaddrs_networks, get_process_env, read_android_soc_property, set_process_env,
};
use crate::jni_helpers::{
    call_void_method, clear_pending_exception, delete_global_ref, delete_local_ref, get_java_vm,
    get_method_id, is_env_valid, jval_bool, jval_int, jval_obj, new_global_ref, new_jni_string,
    AttachedThreadGuard,
};
use crate::protocol::{
    build_network_snapshot, resolve_pick_mime_type, sanitize_picked_filename, write_atomic_bytes,
    AndroidPermissionSnapshot, BridgeCommandJson, PickRequestJson, PickResponseJson,
};
use crate::runtime_installer::{
    ensure_mobile_runtime_extracted, mobile_runtime_root, stage_apk_ffi_shim, write_resolv_conf,
};
use jni_sys::{jmethodID, jobject, JNIEnv, JavaVM};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::io::FromRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BRIDGE_POLL_INTERVAL: Duration = Duration::from_millis(80);
const PERM_REFRESH_TICKS: u32 = 10;
const NET_REFRESH_TICKS: u32 = 30;
const SAF_COPY_BUF_BYTES: usize = 65_536;

#[derive(Clone, Copy)]
struct HostJniCache {
    vm: *mut JavaVM,
    activity_ref: jobject,
    on_apply_orientation: jmethodID,
    on_open_url: jmethodID,
    on_set_clipboard: jmethodID,
    on_pick_document: jmethodID,
    on_request_permission: jmethodID,
    on_set_keep_alive: jmethodID,
    on_launch_game: jmethodID,
    on_stop_game: jmethodID,
    on_refresh_host_state: jmethodID,
}

unsafe impl Send for HostJniCache {}
unsafe impl Sync for HostJniCache {}

#[derive(Default)]
struct HostState {
    jni: Option<HostJniCache>,
    files_dir: PathBuf,
    apk_source_dir: PathBuf,
    dns_servers: Vec<String>,
    active_pick_id: Option<String>,
    watcher_stop: Option<Arc<AtomicBool>>,
    watcher_handle: Option<JoinHandle<()>>,
}

static HOST_STATE: OnceLock<Mutex<HostState>> = OnceLock::new();
static PICK_SEQ: AtomicU64 = AtomicU64::new(1);

fn host_state() -> &'static Mutex<HostState> {
    HOST_STATE.get_or_init(|| Mutex::new(HostState::default()))
}

pub(crate) unsafe fn init_host(
    env: *mut JNIEnv,
    activity: jobject,
    files_dir: String,
    apk_source_dir: String,
    soc_hint: String,
) {
    if !is_env_valid(env) || activity.is_null() || files_dir.trim().is_empty() {
        return;
    }

    shutdown_host(env);

    let files_path = PathBuf::from(files_dir.trim());
    let apk_path = PathBuf::from(apk_source_dir.trim());
    let bridge_dir = files_path.join(".thk_bridge");
    let _ = fs::create_dir_all(&bridge_dir);

    let resolved_soc = if !soc_hint.trim().is_empty() {
        soc_hint.trim().to_string()
    } else {
        read_android_soc_property()
    };

    set_process_env("THK_ANDROID_BRIDGE_DIR", &bridge_dir.to_string_lossy());
    if !apk_source_dir.trim().is_empty() {
        set_process_env("THK_ANDROID_APK_PATH", apk_source_dir.trim());
    }
    if !resolved_soc.is_empty() {
        set_process_env("THK_ANDROID_SOC", &resolved_soc);
    }

    let Some(vm) = get_java_vm(env) else {
        return;
    };
    let Some(activity_ref) = new_global_ref(env, activity) else {
        return;
    };

    let activity_cls = ((**env).v1_6.GetObjectClass)(env, activity);
    if clear_pending_exception(env) || activity_cls.is_null() {
        delete_global_ref(env, activity_ref);
        return;
    }

    let cache_opt = (|| -> Option<HostJniCache> {
        Some(HostJniCache {
            vm,
            activity_ref,
            on_apply_orientation: get_method_id(
                env,
                activity_cls,
                c"onBridgeApplyOrientation",
                c"(Ljava/lang/String;)V",
            )?,
            on_open_url: get_method_id(
                env,
                activity_cls,
                c"onBridgeOpenUrl",
                c"(Ljava/lang/String;)V",
            )?,
            on_set_clipboard: get_method_id(
                env,
                activity_cls,
                c"onBridgeSetClipboard",
                c"(Ljava/lang/String;)V",
            )?,
            on_pick_document: get_method_id(
                env,
                activity_cls,
                c"onBridgePickDocument",
                c"(Ljava/lang/String;ZLjava/lang/String;)V",
            )?,
            on_request_permission: get_method_id(
                env,
                activity_cls,
                c"onBridgeRequestPermission",
                c"(I)V",
            )?,
            on_set_keep_alive: get_method_id(
                env,
                activity_cls,
                c"onBridgeSetKeepAlive",
                c"(ZZLjava/lang/String;)V",
            )?,
            on_launch_game: get_method_id(
                env,
                activity_cls,
                c"onBridgeLaunchGame",
                c"(Ljava/lang/String;Ljava/lang/String;II)V",
            )?,
            on_stop_game: get_method_id(env, activity_cls, c"onBridgeStopGame", c"()V")?,
            on_refresh_host_state: get_method_id(
                env,
                activity_cls,
                c"onBridgeRefreshHostState",
                c"(Z)V",
            )?,
        })
    })();
    delete_local_ref(env, activity_cls);

    let Some(cache) = cache_opt else {
        delete_global_ref(env, activity_ref);
        return;
    };

    let stop_flag = Arc::new(AtomicBool::new(false));
    let dns_snapshot = {
        let mut st = host_state().lock().unwrap_or_else(|e| e.into_inner());
        st.jni = Some(cache);
        st.files_dir = files_path.clone();
        st.apk_source_dir = apk_path.clone();
        st.active_pick_id = None;
        st.watcher_stop = Some(Arc::clone(&stop_flag));
        st.dns_servers.clone()
    };

    if !apk_path.as_os_str().is_empty() {
        let unpack_files = files_path.clone();
        let unpack_apk = apk_path;
        let _ = thread::Builder::new()
            .name("thk-init-unpack".to_string())
            .spawn(move || {
                let _ = stage_apk_ffi_shim(&unpack_files, &unpack_apk);
                let _ = ensure_mobile_runtime_extracted(&unpack_files, &unpack_apk, &dns_snapshot);
            });
    }

    let watcher_stop = Arc::clone(&stop_flag);
    let watcher_bridge_dir = bridge_dir;
    if let Ok(handle) = thread::Builder::new()
        .name("thk-android-bridge".to_string())
        .spawn(move || run_host_watcher_loop(cache, watcher_bridge_dir, watcher_stop))
    {
        let mut st = host_state().lock().unwrap_or_else(|e| e.into_inner());
        if st
            .watcher_stop
            .as_ref()
            .is_some_and(|cur| Arc::ptr_eq(cur, &stop_flag))
        {
            st.watcher_handle = Some(handle);
        }
    }
}

pub(crate) unsafe fn shutdown_host(env: *mut JNIEnv) {
    let (stop_opt, handle_opt, old_jni) = {
        let mut st = host_state().lock().unwrap_or_else(|e| e.into_inner());
        st.active_pick_id = None;
        (
            st.watcher_stop.take(),
            st.watcher_handle.take(),
            st.jni.take(),
        )
    };

    if let Some(stop) = stop_opt {
        stop.store(true, Ordering::Release);
    }
    if let Some(handle) = handle_opt {
        if handle.thread().id() != thread::current().id() {
            let _ = handle.join();
        }
    }
    if let Some(jni) = old_jni {
        if is_env_valid(env) {
            delete_global_ref(env, jni.activity_ref);
        } else if let Some(guard) = AttachedThreadGuard::attach(jni.vm, c"thk-host-cleanup", true) {
            delete_global_ref(guard.env(), jni.activity_ref);
        }
    }
}

pub fn sync_host_state(snapshot: AndroidPermissionSnapshot) {
    let Some(bridge_dir) = resolve_active_bridge_dir(None) else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec(&snapshot) else {
        return;
    };
    let _ = write_atomic_bytes(&bridge_dir, "permissions.tmp", "permissions.json", &bytes);
}

pub fn sync_network_from_java(
    iface_pairs: Vec<String>,
    global_ipv6: Vec<String>,
    dns_servers: Vec<String>,
) {
    let (native_ifaces, native_ipv6) = collect_getifaddrs_networks();
    let net_snapshot =
        build_network_snapshot(&iface_pairs, &global_ipv6, &native_ifaces, &native_ipv6);

    let files_dir = {
        let mut st = host_state().lock().unwrap_or_else(|e| e.into_inner());
        st.dns_servers = dns_servers.clone();
        st.files_dir.clone()
    };

    if let Some(bridge_dir) = resolve_active_bridge_dir(if files_dir.as_os_str().is_empty() {
        None
    } else {
        Some(files_dir.as_path())
    }) {
        if let Ok(bytes) = serde_json::to_vec(&net_snapshot) {
            let _ = write_atomic_bytes(&bridge_dir, "networks.tmp", "networks.json", &bytes);
        }
    }

    if !files_dir.as_os_str().is_empty() {
        let rt_root = mobile_runtime_root(&files_dir);
        if rt_root.is_dir() {
            write_resolv_conf(&rt_root, &dns_servers);
        }
    }
}

pub fn stage_picked_fd(files_dir: &Path, detached_fd: i32, display_name: &str) -> String {
    if detached_fd < 0 {
        return String::new();
    }
    let mut src_file = unsafe { File::from_raw_fd(detached_fd) };
    if files_dir.as_os_str().is_empty() {
        return String::new();
    }

    let picked_dir = files_dir
        .join(".TriHienKunLaucher")
        .join("cache")
        .join("picked");
    if fs::create_dir_all(&picked_dir).is_err() {
        return String::new();
    }

    let sanitized = sanitize_picked_filename(display_name);
    let prefix = next_pick_hex_prefix();
    let dest_path = picked_dir.join(format!("{prefix:08x}_{sanitized}"));

    let copy_ok = (|| -> std::io::Result<()> {
        let mut out = File::create(&dest_path)?;
        let mut buf = vec![0u8; SAF_COPY_BUF_BYTES];
        loop {
            let n = src_file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
        }
        out.flush()
    })();

    if copy_ok.is_err() {
        let _ = fs::remove_file(&dest_path);
        return String::new();
    }

    dest_path.to_string_lossy().into_owned()
}

pub fn complete_pick_request(files_dir: &Path, req_id: &str, staged_paths: Vec<String>) {
    {
        let mut st = host_state().lock().unwrap_or_else(|e| e.into_inner());
        st.active_pick_id = None;
    }

    let trimmed_id = req_id.trim();
    if trimmed_id.is_empty()
        || trimmed_id.contains('/')
        || trimmed_id.contains('\\')
        || trimmed_id.contains("..")
        || trimmed_id.contains('\0')
    {
        return;
    }

    let Some(bridge_dir) = resolve_active_bridge_dir(if files_dir.as_os_str().is_empty() {
        None
    } else {
        Some(files_dir)
    }) else {
        return;
    };

    let resp = PickResponseJson {
        paths: staged_paths,
    };
    let Ok(bytes) = serde_json::to_vec(&resp) else {
        return;
    };
    let tmp_name = format!("pick_resp_{trimmed_id}.tmp");
    let final_name = format!("pick_resp_{trimmed_id}.json");
    let _ = write_atomic_bytes(&bridge_dir, &tmp_name, &final_name, &bytes);
}

fn run_host_watcher_loop(cache: HostJniCache, bridge_dir: PathBuf, stop: Arc<AtomicBool>) {
    let Some(guard) =
        (unsafe { AttachedThreadGuard::attach(cache.vm, c"thk-android-bridge", true) })
    else {
        return;
    };
    let env = guard.env();

    let mut perm_tick: u32 = 0;
    let mut net_tick: u32 = 0;

    while !stop.load(Ordering::Acquire) {
        unsafe {
            poll_orientation(env, &cache, &bridge_dir);
            poll_open_url(env, &cache, &bridge_dir);
            poll_clipboard(env, &cache, &bridge_dir);
            poll_pick_request(env, &cache, &bridge_dir);
            poll_commands(env, &cache, &bridge_dir);

            perm_tick += 1;
            if perm_tick >= PERM_REFRESH_TICKS {
                perm_tick = 0;
                call_refresh_host_state(env, &cache, false);
            }

            net_tick += 1;
            if net_tick >= NET_REFRESH_TICKS {
                net_tick = 0;
                call_refresh_host_state(env, &cache, true);
            }
        }

        if stop.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(BRIDGE_POLL_INTERVAL);
    }
}

unsafe fn poll_orientation(env: *mut JNIEnv, cache: &HostJniCache, bridge_dir: &Path) {
    let path = bridge_dir.join("orientation.txt");
    let Some(raw) = read_utf8_and_delete(&path) else {
        return;
    };
    let mode = raw.trim();
    if !mode.is_empty() {
        call_single_string_method(env, cache.activity_ref, cache.on_apply_orientation, mode);
    }
}

unsafe fn poll_open_url(env: *mut JNIEnv, cache: &HostJniCache, bridge_dir: &Path) {
    let path = bridge_dir.join("open_url.txt");
    let Some(raw) = read_utf8_and_delete(&path) else {
        return;
    };
    let url = raw.trim();
    if url.starts_with("http://") || url.starts_with("https://") {
        call_single_string_method(env, cache.activity_ref, cache.on_open_url, url);
    }
}

unsafe fn poll_clipboard(env: *mut JNIEnv, cache: &HostJniCache, bridge_dir: &Path) {
    let path = bridge_dir.join("clipboard.txt");
    let Some(text) = read_utf8_and_delete(&path) else {
        return;
    };
    call_single_string_method(env, cache.activity_ref, cache.on_set_clipboard, &text);
}

unsafe fn poll_pick_request(env: *mut JNIEnv, cache: &HostJniCache, bridge_dir: &Path) {
    let req_path = bridge_dir.join("pick_req.json");
    if !req_path.is_file() {
        return;
    }

    let (raw, files_dir) = {
        let st = host_state().lock().unwrap_or_else(|e| e.into_inner());
        if st.active_pick_id.is_some() {
            return;
        }
        let Some(content) = read_utf8_and_delete(&req_path) else {
            return;
        };
        (content, st.files_dir.clone())
    };

    let Ok(req) = serde_json::from_str::<PickRequestJson>(&raw) else {
        return;
    };
    let req_id = req.id.trim();
    if req_id.is_empty() {
        return;
    }

    {
        let mut st = host_state().lock().unwrap_or_else(|e| e.into_inner());
        st.active_pick_id = Some(req_id.to_string());
    }

    let mime = resolve_pick_mime_type(&req.extensions);
    let j_id = new_jni_string(env, req_id);
    let j_mime = new_jni_string(env, mime);
    let invoked = if !j_id.is_null() && !j_mime.is_null() {
        call_void_method(
            env,
            cache.activity_ref,
            cache.on_pick_document,
            &[jval_obj(j_id), jval_bool(req.multiple), jval_obj(j_mime)],
        )
    } else {
        false
    };
    delete_local_ref(env, j_id);
    delete_local_ref(env, j_mime);

    if !invoked {
        complete_pick_request(&files_dir, req_id, Vec::new());
    }
}

unsafe fn poll_commands(env: *mut JNIEnv, cache: &HostJniCache, bridge_dir: &Path) {
    let Ok(entries) = fs::read_dir(bridge_dir) else {
        return;
    };
    let mut cmd_files: Vec<(SystemTime, String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("cmd_") || !name.ends_with(".json") {
                return None;
            }
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
            Some((mtime, name, entry.path()))
        })
        .collect();

    if cmd_files.is_empty() {
        return;
    }
    cmd_files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    for (_, _, cmd_path) in cmd_files {
        let Some(raw) = read_utf8_and_delete(&cmd_path) else {
            continue;
        };
        if raw.trim().is_empty() {
            continue;
        }
        let Ok(cmd) = serde_json::from_str::<BridgeCommandJson>(&raw) else {
            continue;
        };
        dispatch_bridge_command(env, cache, cmd);
    }

    call_refresh_host_state(env, cache, false);
}

unsafe fn dispatch_bridge_command(env: *mut JNIEnv, cache: &HostJniCache, cmd: BridgeCommandJson) {
    match cmd.op.trim() {
        "ensure_mobile_runtime" => {
            let (files_dir, apk_dir, dns) = {
                let st = host_state().lock().unwrap_or_else(|e| e.into_inner());
                (
                    st.files_dir.clone(),
                    st.apk_source_dir.clone(),
                    st.dns_servers.clone(),
                )
            };
            if !files_dir.as_os_str().is_empty() && !apk_dir.as_os_str().is_empty() {
                let _ = thread::Builder::new()
                    .name("thk-cmd-unpack".to_string())
                    .spawn(move || {
                        let _ = stage_apk_ffi_shim(&files_dir, &apk_dir);
                        let _ = ensure_mobile_runtime_extracted(&files_dir, &apk_dir, &dns);
                    });
            }
        }
        "request_ignore_battery" => {
            let _ = call_void_method(
                env,
                cache.activity_ref,
                cache.on_request_permission,
                &[jval_int(1)],
            );
        }
        "request_overlay_permission" => {
            let _ = call_void_method(
                env,
                cache.activity_ref,
                cache.on_request_permission,
                &[jval_int(2)],
            );
        }
        "request_notification_permission" => {
            let _ = call_void_method(
                env,
                cache.activity_ref,
                cache.on_request_permission,
                &[jval_int(3)],
            );
        }
        "set_keep_alive" => {
            let j_status = new_jni_string(env, &cmd.status);
            if !j_status.is_null() {
                let _ = call_void_method(
                    env,
                    cache.activity_ref,
                    cache.on_set_keep_alive,
                    &[
                        jval_bool(cmd.enabled),
                        jval_bool(cmd.overlay),
                        jval_obj(j_status),
                    ],
                );
                delete_local_ref(env, j_status);
            }
        }
        "launch_mobile_game" => {
            let spec = cmd.spec_path.trim();
            if !spec.is_empty() {
                let j_session = new_jni_string(env, &cmd.session_id);
                let j_spec = new_jni_string(env, spec);
                if !j_session.is_null() && !j_spec.is_null() {
                    let _ = call_void_method(
                        env,
                        cache.activity_ref,
                        cache.on_launch_game,
                        &[
                            jval_obj(j_session),
                            jval_obj(j_spec),
                            jval_int(cmd.window_width),
                            jval_int(cmd.window_height),
                        ],
                    );
                }
                delete_local_ref(env, j_session);
                delete_local_ref(env, j_spec);
            }
        }
        "stop_mobile_game" => {
            let _ = call_void_method(env, cache.activity_ref, cache.on_stop_game, &[]);
        }
        _ => {}
    }
}

#[inline]
unsafe fn call_refresh_host_state(env: *mut JNIEnv, cache: &HostJniCache, include_networks: bool) {
    let _ = call_void_method(
        env,
        cache.activity_ref,
        cache.on_refresh_host_state,
        &[jval_bool(include_networks)],
    );
}

unsafe fn call_single_string_method(
    env: *mut JNIEnv,
    target: jobject,
    method: jmethodID,
    value: &str,
) {
    let j_str = new_jni_string(env, value);
    if !j_str.is_null() {
        let _ = call_void_method(env, target, method, &[jval_obj(j_str)]);
        delete_local_ref(env, j_str);
    }
}

fn read_utf8_and_delete(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let res = fs::read(path);
    let _ = fs::remove_file(path);
    let bytes = res.ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn resolve_active_bridge_dir(explicit_files_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = explicit_files_dir {
        if !dir.as_os_str().is_empty() {
            return Some(dir.join(".thk_bridge"));
        }
    }
    {
        let st = host_state().lock().unwrap_or_else(|e| e.into_inner());
        if !st.files_dir.as_os_str().is_empty() {
            return Some(st.files_dir.join(".thk_bridge"));
        }
    }
    get_process_env("THK_ANDROID_BRIDGE_DIR")
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

fn next_pick_hex_prefix() -> u32 {
    let seq = PICK_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut z = nanos
        ^ ((std::process::id() as u64).rotate_left(17))
        ^ seq.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (z ^ (z >> 31)) as u32
}
