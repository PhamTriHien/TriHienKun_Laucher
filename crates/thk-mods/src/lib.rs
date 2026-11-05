#![deny(unsafe_code)]

pub mod curseforge;
pub mod ftb;
pub mod matcher;
pub mod modrinth;
pub mod provider;
pub mod spiget;
pub mod technic;

pub use curseforge::{
    build_curseforge_cdn_fallback_url, import_curseforge_zip, install_curseforge_content,
    install_curseforge_modpack_version,
};
pub use ftb::install_ftb_modpack_version;
pub use matcher::{
    check_local_content_updates, delete_local_content, delete_local_world,
    delete_server_addon_file, enrich_unmatched_via_modrinth_sha1, import_server_addon_jar,
    list_local_content, list_local_worlds, list_server_addon_files, split_filename_title_and_version,
    toggle_local_content, toggle_server_addon_file, ContentUpdateCandidate, LocalContentItem,
    LocalWorldInfo, ServerAddonFile,
};
pub use modrinth::{
    ensure_fabric_performance_mods, import_mrpack_archive, install_modrinth_content,
    install_modrinth_modpack_version, FABRIC_PERF_MOD_IDS,
};
pub use provider::{
    fetch_provider_project, fetch_provider_versions, search_provider, DependencyRelation,
    DependencySpec, GalleryImage, ImportedPackMetadata, ProjectDetail, ProjectSummary,
    ProjectVersion, SearchPage, SearchQuery, SearchSort, VersionFile,
};
pub use spiget::install_spiget_plugin;
pub use technic::install_technic_modpack_version;

#[cfg(test)]
mod tests {
    use super::*;
    use thk_core::models::ContentKind;
    use thk_core::Database;

    #[test]
    fn curseforge_cdn_url_and_filename_splitter_work() {
        let url = build_curseforge_cdn_fallback_url(5_432_109, "jei-1.21.1-fabric-19.21.0.jar");
        assert_eq!(
            url,
            "https://edge.forgecdn.net/files/5432/109/jei-1.21.1-fabric-19.21.0.jar"
        );

        let (title, ver) = split_filename_title_and_version("sodium-fabric-0.6.5+mc1.21.4.jar");
        assert_eq!(title, "sodium");
        assert_eq!(ver, "0.6.5-mc1.21.4");
    }

    #[test]
    fn local_content_toggle_and_safe_delete_work() {
        let db = Database::open_in_memory().unwrap();
        let tmp = std::env::temp_dir().join(format!("thk-mods-{}", uuid::Uuid::new_v4()));
        let mods_dir = tmp.join("mods");
        std::fs::create_dir_all(&mods_dir).unwrap();
        std::fs::write(mods_dir.join("lithium-0.14.0.jar"), b"fakejar").unwrap();

        let items = list_local_content(&db, "p1", &tmp, ContentKind::Mod).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].enabled);

        let disabled =
            toggle_local_content(&db, "p1", &tmp, ContentKind::Mod, "lithium-0.14.0.jar", false)
                .unwrap();
        assert_eq!(disabled, "lithium-0.14.0.jar.disabled");
        assert!(mods_dir.join("lithium-0.14.0.jar.disabled").is_file());

        // Ensure path traversal attempt is rejected
        assert!(delete_local_content(&db, "p1", &tmp, ContentKind::Mod, "../outside.jar").is_err());

        delete_local_content(&db, "p1", &tmp, ContentKind::Mod, &disabled).unwrap();
        assert!(list_local_content(&db, "p1", &tmp, ContentKind::Mod)
            .unwrap()
            .is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn server_addon_jar_list_toggle_import_and_delete_work() {
        let tmp = std::env::temp_dir().join(format!("thk-srvaddon-{}", uuid::Uuid::new_v4()));
        let ext_dir = tmp.join("ext");
        let srv_dir = tmp.join("srv");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::create_dir_all(&srv_dir).unwrap();

        let sample_jar = ext_dir.join("EssentialsX-2.20.1.jar");
        std::fs::write(&sample_jar, b"fakeplugin").unwrap();

        let imported = import_server_addon_jar(&srv_dir, "plugins", &sample_jar).unwrap();
        assert_eq!(imported, "EssentialsX-2.20.1.jar");

        let list = list_server_addon_files(&srv_dir, true, true).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].enabled);
        assert_eq!(list[0].folder, "plugins");

        let disabled =
            toggle_server_addon_file(&srv_dir, "plugins", "EssentialsX-2.20.1.jar", false)
                .unwrap();
        assert_eq!(disabled, "EssentialsX-2.20.1.jar.disabled");
        assert!(delete_server_addon_file(&srv_dir, "evil", &disabled).is_err());

        delete_server_addon_file(&srv_dir, "plugins", &disabled).unwrap();
        assert!(list_server_addon_files(&srv_dir, true, true)
            .unwrap()
            .is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
