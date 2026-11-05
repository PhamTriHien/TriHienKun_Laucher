#![deny(unsafe_code)]

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/icon.ico");

    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let ico_path = std::path::Path::new("../../assets/icons/icon.ico");
            let mut res = winresource::WindowsResource::new();
            if ico_path.is_file() {
                res.set_icon("../../assets/icons/icon.ico");
            }
            res.set("ProductName", "TriHienKun Laucher");
            res.set(
                "FileDescription",
                "TriHienKun Laucher - Pure Rust 3D Minecraft Launcher",
            );
            res.set("CompanyName", "TriHienKun");
            res.set("LegalCopyright", "Copyright (c) TriHienKun");
            let _ = res.compile();
        }
    }
}
