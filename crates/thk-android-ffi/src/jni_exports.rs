#![allow(clippy::missing_safety_doc, clippy::too_many_arguments)]

use crate::game_session::{
    notify_grab_state, notify_jvm_exit, start_game_jvm, stop_game_jvm, update_game_surface,
};
use crate::host_bridge::{
    complete_pick_request, init_host, shutdown_host, stage_picked_fd, sync_host_state,
    sync_network_from_java,
};
use crate::jni_helpers::{
    clear_pending_exception, is_env_valid, jobject_array_to_strings, jstring_to_string,
    new_jni_string,
};
use crate::protocol::{
    read_saved_orientation, resolve_launch_orientation, write_focus_mode_atomic,
    AndroidPermissionSnapshot,
};
use jni_sys::{jboolean, jclass, jint, jobject, jobjectArray, jstring, JNIEnv};
use std::fs::File;
use std::os::unix::io::FromRawFd;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

#[inline]
unsafe fn run_jni_void<F: FnOnce()>(env: *mut JNIEnv, f: F) {
    if !is_env_valid(env) {
        return;
    }
    let _ = clear_pending_exception(env);
    let _ = catch_unwind(AssertUnwindSafe(f));
    let _ = clear_pending_exception(env);
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeInitHost(
    env: *mut JNIEnv,
    _cls: jclass,
    activity: jobject,
    files_dir: jstring,
    apk_source_dir: jstring,
    soc_hint: jstring,
) {
    run_jni_void(env, || {
        let files = jstring_to_string(env, files_dir);
        let apk = jstring_to_string(env, apk_source_dir);
        let soc = jstring_to_string(env, soc_hint);
        init_host(env, activity, files, apk, soc);
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeShutdownHost(
    env: *mut JNIEnv,
    _cls: jclass,
) {
    run_jni_void(env, || {
        shutdown_host(env);
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeSyncHostState(
    env: *mut JNIEnv,
    _cls: jclass,
    ignore_battery: jboolean,
    can_overlay: jboolean,
    notif_enabled: jboolean,
    keep_alive: jboolean,
    overlay_showing: jboolean,
    surface_ready: jboolean,
    surface_w: jint,
    surface_h: jint,
    game_running: jboolean,
    inset_l: jint,
    inset_t: jint,
    inset_r: jint,
    inset_b: jint,
) {
    run_jni_void(env, || {
        sync_host_state(AndroidPermissionSnapshot {
            ignore_battery_optimizations: ignore_battery,
            can_draw_overlays: can_overlay,
            notifications_enabled: notif_enabled,
            keep_alive_running: keep_alive,
            overlay_showing,
            surface_ready,
            surface_width: surface_w.max(0) as u32,
            surface_height: surface_h.max(0) as u32,
            game_process_running: game_running,
            safe_inset_left_px: inset_l.max(0) as u32,
            safe_inset_top_px: inset_t.max(0) as u32,
            safe_inset_right_px: inset_r.max(0) as u32,
            safe_inset_bottom_px: inset_b.max(0) as u32,
        });
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeSyncNetworkFromJava(
    env: *mut JNIEnv,
    _cls: jclass,
    iface_pairs: jobjectArray,
    global_ipv6: jobjectArray,
    dns_servers: jobjectArray,
) {
    run_jni_void(env, || {
        let pairs = jobject_array_to_strings(env, iface_pairs);
        let ipv6 = jobject_array_to_strings(env, global_ipv6);
        let dns = jobject_array_to_strings(env, dns_servers);
        sync_network_from_java(pairs, ipv6, dns);
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeReadSavedOrientation(
    env: *mut JNIEnv,
    _cls: jclass,
    files_dir: jstring,
) -> jstring {
    if !is_env_valid(env) {
        return std::ptr::null_mut();
    }
    let _ = clear_pending_exception(env);
    let out = catch_unwind(AssertUnwindSafe(|| {
        let dir = jstring_to_string(env, files_dir);
        let saved = read_saved_orientation(Path::new(&dir));
        new_jni_string(env, &saved)
    }))
    .unwrap_or(std::ptr::null_mut());
    let _ = clear_pending_exception(env);
    out
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeResolveLaunchOrientation(
    env: *mut JNIEnv,
    _cls: jclass,
    files_dir: jstring,
    win_w: jint,
    win_h: jint,
) -> jstring {
    if !is_env_valid(env) {
        return std::ptr::null_mut();
    }
    let _ = clear_pending_exception(env);
    let out = catch_unwind(AssertUnwindSafe(|| {
        let dir = jstring_to_string(env, files_dir);
        let saved = read_saved_orientation(Path::new(&dir));
        let resolved = resolve_launch_orientation(&saved, win_w, win_h);
        new_jni_string(env, &resolved)
    }))
    .unwrap_or(std::ptr::null_mut());
    let _ = clear_pending_exception(env);
    out
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeWriteFocusMode(
    env: *mut JNIEnv,
    _cls: jclass,
    files_dir: jstring,
    mode: jstring,
) {
    run_jni_void(env, || {
        let dir = jstring_to_string(env, files_dir);
        let mode_str = jstring_to_string(env, mode);
        if !dir.trim().is_empty() {
            write_focus_mode_atomic(Path::new(dir.trim()), &mode_str);
        }
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeStagePickedFd(
    env: *mut JNIEnv,
    _cls: jclass,
    files_dir: jstring,
    detached_fd: jint,
    display_name: jstring,
) -> jstring {
    if !is_env_valid(env) {
        if detached_fd >= 0 {
            drop(File::from_raw_fd(detached_fd));
        }
        return std::ptr::null_mut();
    }
    let _ = clear_pending_exception(env);
    let out = catch_unwind(AssertUnwindSafe(|| {
        if detached_fd < 0 {
            return new_jni_string(env, "");
        }
        let dir = jstring_to_string(env, files_dir);
        let name = jstring_to_string(env, display_name);
        let staged = stage_picked_fd(Path::new(dir.trim()), detached_fd, &name);
        new_jni_string(env, &staged)
    }))
    .unwrap_or(std::ptr::null_mut());
    let _ = clear_pending_exception(env);
    out
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeCompletePickRequest(
    env: *mut JNIEnv,
    _cls: jclass,
    files_dir: jstring,
    req_id: jstring,
    staged_paths: jobjectArray,
) {
    run_jni_void(env, || {
        let dir = jstring_to_string(env, files_dir);
        let id = jstring_to_string(env, req_id);
        let paths = jobject_array_to_strings(env, staged_paths);
        complete_pick_request(Path::new(dir.trim()), &id, paths);
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeStartGameJvm(
    env: *mut JNIEnv,
    _cls: jclass,
    app_context: jobject,
    files_dir: jstring,
    apk_source_dir: jstring,
    spec_path: jstring,
    initial_surface: jobject,
    width: jint,
    height: jint,
    dns_servers: jobjectArray,
) {
    run_jni_void(env, || {
        let files = jstring_to_string(env, files_dir);
        let apk = jstring_to_string(env, apk_source_dir);
        let spec = jstring_to_string(env, spec_path);
        let dns = jobject_array_to_strings(env, dns_servers);
        start_game_jvm(
            env,
            app_context,
            files,
            apk,
            spec,
            initial_surface,
            width,
            height,
            dns,
        );
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeUpdateGameSurface(
    env: *mut JNIEnv,
    _cls: jclass,
    surface: jobject,
    width: jint,
    height: jint,
) {
    run_jni_void(env, || {
        update_game_surface(env, surface, width, height);
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeStopGameJvm(
    env: *mut JNIEnv,
    _cls: jclass,
) {
    run_jni_void(env, || {
        stop_game_jvm(env);
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeNotifyGrabState(
    env: *mut JNIEnv,
    _cls: jclass,
    grabbing: jboolean,
) {
    run_jni_void(env, || {
        notify_grab_state(grabbing);
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_trihienkun_laucher_ThkNativeBridge_nativeNotifyJvmExit(
    env: *mut JNIEnv,
    _cls: jclass,
    code: jint,
) {
    run_jni_void(env, || {
        notify_jvm_exit(Some(env), code);
    });
}
