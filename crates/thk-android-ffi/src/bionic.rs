use crate::jni_helpers::{clear_pending_exception, is_env_valid};
use crate::protocol::{is_global_unicast_ipv6, is_valid_lan_ipv4};
use jni_sys::{jclass, JNIEnv};
use std::ffi::{CStr, CString};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Mutex;

extern "C" {
    fn mallopt(param: libc::c_int, value: libc::c_int) -> libc::c_int;
}

const M_BIONIC_SET_HEAP_TAGGING_LEVEL: libc::c_int = -204;
const M_HEAP_TAGGING_LEVEL_NONE: libc::c_int = 0;
const ANDROID_PROP_VALUE_MAX: usize = 92;

static ENV_MUTEX: Mutex<()> = Mutex::new(());

unsafe extern "C" fn thk_init_bionic_jvm_compat() {
    let _ = mallopt(M_BIONIC_SET_HEAP_TAGGING_LEVEL, M_HEAP_TAGGING_LEVEL_NONE);
}

#[used]
#[link_section = ".init_array"]
static THK_INIT_BIONIC_JVM_COMPAT: unsafe extern "C" fn() = thk_init_bionic_jvm_compat;

type UpdateLdLibraryPathFn = unsafe extern "C" fn(*const libc::c_char);
type ExitHookInitFn = unsafe extern "system" fn(*mut JNIEnv, jclass);

pub fn bionic_update_ld_library_path(ld_path: &str) -> bool {
    if ld_path.is_empty() {
        return false;
    }
    let Ok(c_path) = CString::new(ld_path) else {
        return false;
    };

    unsafe {
        let mut sym: *mut libc::c_void = std::ptr::null_mut();
        let libdl = libc::dlopen(c"libdl.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if !libdl.is_null() {
            sym = libc::dlsym(libdl, c"android_update_LD_LIBRARY_PATH".as_ptr());
            if sym.is_null() {
                sym = libc::dlsym(libdl, c"__loader_android_update_LD_LIBRARY_PATH".as_ptr());
            }
        }
        if sym.is_null() {
            sym = libc::dlsym(
                libc::RTLD_DEFAULT,
                c"android_update_LD_LIBRARY_PATH".as_ptr(),
            );
        }
        if sym.is_null() {
            sym = libc::dlsym(
                libc::RTLD_DEFAULT,
                c"__loader_android_update_LD_LIBRARY_PATH".as_ptr(),
            );
        }
        if sym.is_null() {
            return false;
        }

        let update_fn: UpdateLdLibraryPathFn = std::mem::transmute(sym);
        update_fn(c_path.as_ptr());
        true
    }
}

pub(crate) unsafe fn invoke_exit_hook_init(
    env: *mut JNIEnv,
    jre_utils_cls: jclass,
    exit_hook_path: &str,
) -> bool {
    if !is_env_valid(env) || jre_utils_cls.is_null() || exit_hook_path.is_empty() {
        return false;
    }
    let Ok(c_path) = CString::new(exit_hook_path) else {
        return false;
    };

    let _ = clear_pending_exception(env);
    let handle = libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
    if handle.is_null() {
        return false;
    }
    let sym = libc::dlsym(
        handle,
        c"Java_net_kdt_pojavlaunch_utils_JREUtils_initializeHooks".as_ptr(),
    );
    if sym.is_null() {
        return false;
    }

    let init_fn: ExitHookInitFn = std::mem::transmute(sym);
    init_fn(env, jre_utils_cls);
    !clear_pending_exception(env)
}

pub fn read_android_soc_property() -> String {
    let mfg = read_system_property(c"ro.soc.manufacturer");
    let model = read_system_property(c"ro.soc.model");
    let combined = match (mfg.is_empty(), model.is_empty()) {
        (false, false) => format!("{mfg} {model}"),
        (false, true) => mfg,
        (true, false) => model,
        (true, true) => String::new(),
    };
    if !combined.is_empty() {
        return combined;
    }
    let hw = read_system_property(c"ro.hardware");
    if !hw.is_empty() {
        return hw;
    }
    read_system_property(c"ro.board.platform")
}

fn read_system_property(name: &CStr) -> String {
    let mut buf = [0 as libc::c_char; ANDROID_PROP_VALUE_MAX];
    let len = unsafe { libc::__system_property_get(name.as_ptr(), buf.as_mut_ptr()) };
    if len <= 0 {
        return String::new();
    }
    let capped = (len as usize).min(ANDROID_PROP_VALUE_MAX - 1);
    let val = String::from_utf8_lossy(&buf[..capped]).trim().to_string();
    if val.eq_ignore_ascii_case("unknown") {
        String::new()
    } else {
        val
    }
}

pub fn collect_getifaddrs_networks() -> (Vec<(String, String)>, Vec<String>) {
    let mut v4_out = Vec::new();
    let mut v6_out = Vec::new();

    unsafe {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 || head.is_null() {
            return (v4_out, v6_out);
        }

        let mut cur = head;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;

            if ifa.ifa_addr.is_null() {
                continue;
            }
            let flags = ifa.ifa_flags as libc::c_int;
            if (flags & libc::IFF_UP) == 0 || (flags & libc::IFF_LOOPBACK) != 0 {
                continue;
            }

            let iface_name = if ifa.ifa_name.is_null() {
                "net".to_string()
            } else {
                let s = CStr::from_ptr(ifa.ifa_name).to_string_lossy();
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    "net".to_string()
                } else {
                    trimmed.to_string()
                }
            };

            let family = (*ifa.ifa_addr).sa_family as libc::c_int;
            if family == libc::AF_INET {
                let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                if is_valid_lan_ipv4(ip) {
                    v4_out.push((iface_name, ip.to_string()));
                }
            } else if family == libc::AF_INET6 {
                let sin6 = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                let ip = Ipv6Addr::from(sin6.sin6_addr.s6_addr);
                if is_global_unicast_ipv6(ip) {
                    v6_out.push(ip.to_string());
                }
            }
        }

        libc::freeifaddrs(head);
    }

    (v4_out, v6_out)
}

pub fn set_process_env(key: &str, value: &str) {
    if key.is_empty() || key.contains('=') {
        return;
    }
    let (Ok(c_key), Ok(c_val)) = (CString::new(key), CString::new(value)) else {
        return;
    };
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let _ = libc::setenv(c_key.as_ptr(), c_val.as_ptr(), 1);
    }
}

pub fn get_process_env(key: &str) -> Option<String> {
    if key.is_empty() || key.contains('=') {
        return None;
    }
    let Ok(c_key) = CString::new(key) else {
        return None;
    };
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let raw = libc::getenv(c_key.as_ptr());
        if raw.is_null() {
            None
        } else {
            Some(CStr::from_ptr(raw).to_string_lossy().into_owned())
        }
    }
}
