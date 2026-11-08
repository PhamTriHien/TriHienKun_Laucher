#![deny(unsafe_code)]

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=native/thk_jvm_shim.c");
    println!("cargo:rerun-if-env-changed=CC_aarch64_linux_android");
    println!("cargo:rerun-if-env-changed=CC_aarch64-linux-android");
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER");
    println!("cargo:rerun-if-env-changed=TARGET_CC");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    let out_so = out_dir.join("libthk_jvm_shim.so");
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR missing"));
    let src_c = manifest_dir.join("native").join("thk_jvm_shim.c");

    let compiler = env::var("CC_aarch64_linux_android")
        .or_else(|_| env::var("CC_aarch64-linux-android"))
        .or_else(|_| env::var("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"))
        .or_else(|_| env::var("TARGET_CC"))
        .or_else(|_| env::var("CC"))
        .unwrap_or_else(|_| "aarch64-linux-android28-clang".to_string());

    let status = Command::new(&compiler)
        .arg("-shared")
        .arg("-fPIC")
        .arg("-O2")
        .arg("-Wl,-z,max-page-size=16384")
        .arg("-o")
        .arg(&out_so)
        .arg(&src_c)
        .status()
        .unwrap_or_else(|err| {
            panic!("Failed invoking Android C compiler '{compiler}' for libthk_jvm_shim.so: {err}")
        });

    assert!(
        status.success(),
        "Android C compiler '{compiler}' failed compiling libthk_jvm_shim.so with {status}"
    );
}
