#![deny(unsafe_code)]

pub mod distros;
pub mod mojang_jre;
pub mod scanner;

pub use distros::{
    fetch_available_distro_packages, install_distro_package, DistroReleasePackage, JavaDistroKind,
    SUPPORTED_JAVA_MAJORS,
};
pub use mojang_jre::{ensure_mojang_java, infer_java_major_from_mc_version, RequiredJavaSpec};
pub use scanner::{
    find_java_executable_in_dir, parse_java_version_output, probe_java_binary,
    scan_java_installations, JavaInstallation,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_and_modern_java_versions() {
        let v8 = "openjdk version \"1.8.0_422\"\nOpenJDK Runtime Environment (Temurin)(build 1.8.0_422-b05)";
        let (maj8, full8, vend8) = parse_java_version_output(v8).unwrap();
        assert_eq!(maj8, 8);
        assert_eq!(full8, "1.8.0_422");
        assert_eq!(vend8, "Eclipse Temurin");

        let v21 = "openjdk version \"21.0.5\" 2024-10-15 LTS\nOpenJDK Runtime Environment Zulu21.38+21-CA";
        let (maj21, full21, vend21) = parse_java_version_output(v21).unwrap();
        assert_eq!(maj21, 21);
        assert_eq!(full21, "21.0.5");
        assert_eq!(vend21, "Azul Zulu");
    }

    #[test]
    fn infers_java_for_all_mc_eras_including_26() {
        assert_eq!(infer_java_major_from_mc_version("1.12.2"), 8);
        assert_eq!(infer_java_major_from_mc_version("1.16.5"), 8);
        assert_eq!(infer_java_major_from_mc_version("1.17.1"), 16);
        assert_eq!(infer_java_major_from_mc_version("1.20.1"), 17);
        assert_eq!(infer_java_major_from_mc_version("1.20.5"), 21);
        assert_eq!(infer_java_major_from_mc_version("1.21.4"), 21);
        assert_eq!(infer_java_major_from_mc_version("26.1"), 21);
    }
}
