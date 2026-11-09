use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thk_core::archive::extract_zip_filtered;
use thk_core::models::DownloadProgress;
use thk_core::paths::SafePath;
use thk_core::{AppPaths, DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;

pub const MOJANG_VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const MOJANG_RESOURCES_BASE: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub release_type: String,
    pub url: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifestV2 {
    pub latest: LatestVersions,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VanillaVersionJson {
    pub id: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    pub arguments: Option<MojangArguments>,
    pub libraries: Vec<MojangLibrary>,
    pub downloads: Option<MojangVersionDownloads>,
    #[serde(rename = "assetIndex")]
    pub asset_index: Option<MojangAssetIndexRef>,
    pub assets: Option<String>,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<MojangJavaVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangArguments {
    #[serde(default)]
    pub game: Vec<Value>,
    #[serde(default)]
    pub jvm: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangJavaVersion {
    pub component: Option<String>,
    #[serde(rename = "majorVersion")]
    pub major_version: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangVersionDownloads {
    pub client: Option<MojangArtifact>,
    pub server: Option<MojangArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangArtifact {
    pub path: Option<String>,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangAssetIndexRef {
    pub id: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangLibrary {
    pub name: String,
    pub downloads: Option<MojangLibDownloads>,
    pub url: Option<String>,
    pub natives: Option<HashMap<String, String>>,
    pub rules: Option<Vec<MojangRule>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangLibDownloads {
    pub artifact: Option<MojangArtifact>,
    pub classifiers: Option<HashMap<String, MojangArtifact>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangRule {
    pub action: String,
    pub os: Option<MojangOsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangOsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreparedVanillaAssets {
    pub version_json: VanillaVersionJson,
    pub client_jar: PathBuf,
    pub libraries: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub virtual_assets_dir: Option<PathBuf>,
    pub map_to_resources: bool,
    pub asset_index_id: String,
}

pub async fn fetch_version_manifest(downloader: &DownloadManager) -> ThkResult<VersionManifestV2> {
    downloader.get_json(MOJANG_VERSION_MANIFEST_URL).await
}

pub async fn resolve_vanilla_version_json(
    downloader: &DownloadManager,
    paths: &AppPaths,
    mc_version: &str,
) -> ThkResult<VanillaVersionJson> {
    let clean_ver = SafePath::validate_single_name(mc_version)?;
    let ver_dir = SafePath::resolve_child(&paths.versions_dir, clean_ver)?;
    tokio::fs::create_dir_all(&ver_dir).await?;
    let json_path = SafePath::resolve_child(&ver_dir, format!("{clean_ver}.json"))?;

    if json_path.is_file() {
        if let Ok(raw) = tokio::fs::read_to_string(&json_path).await {
            if let Ok(parsed) = serde_json::from_str::<VanillaVersionJson>(&raw) {
                return Ok(parsed);
            }
        }
    }

    let manifest = fetch_version_manifest(downloader).await?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|v| v.id == clean_ver)
        .ok_or_else(|| {
            ThkError::Loader(format!(
                "Minecraft version '{clean_ver}' not found in Mojang manifest"
            ))
        })?;

    let raw_bytes = downloader.get_bytes(&entry.url).await?;
    let parsed: VanillaVersionJson = serde_json::from_slice(&raw_bytes)?;
    tokio::fs::write(&json_path, &raw_bytes).await?;
    Ok(parsed)
}

pub async fn prepare_vanilla_files<F>(
    downloader: &DownloadManager,
    paths: &AppPaths,
    version_json: &VanillaVersionJson,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<PreparedVanillaAssets>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    prepare_vanilla_files_with_natives(downloader, paths, version_json, None, cancel, on_progress)
        .await
}

pub async fn prepare_vanilla_files_with_natives<F>(
    downloader: &DownloadManager,
    paths: &AppPaths,
    version_json: &VanillaVersionJson,
    custom_natives_dir: Option<PathBuf>,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<PreparedVanillaAssets>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    let clean_ver = SafePath::validate_single_name(&version_json.id)?;
    let ver_dir = SafePath::resolve_child(&paths.versions_dir, clean_ver)?;
    tokio::fs::create_dir_all(&ver_dir).await?;
    let natives_dir = match custom_natives_dir {
        Some(dir) => dir,
        None => SafePath::resolve_child(&ver_dir, "natives")?,
    };
    tokio::fs::create_dir_all(&natives_dir).await?;

    let client_jar = SafePath::resolve_child(&ver_dir, format!("{clean_ver}.jar"))?;
    let mut jar_and_lib_tasks = Vec::new();

    if let Some(client_dl) = version_json
        .downloads
        .as_ref()
        .and_then(|d| d.client.as_ref())
    {
        let mut t = DownloadTask::new(client_dl.url.clone(), client_jar.clone());
        t.expected_sha1 = client_dl.sha1.clone();
        t.expected_size = client_dl.size;
        t.label = format!("client-{clean_ver}.jar");
        jar_and_lib_tasks.push(t);
    }

    let features = HashMap::new();
    let mut classpath_libs = Vec::new();
    let mut native_jars = Vec::new();

    for lib in &version_json.libraries {
        if !evaluate_rules(lib.rules.as_deref(), &features) {
            continue;
        }

        if cfg!(target_os = "android") && is_modern_native_library_coord(&lib.name) {
            continue;
        }

        if cfg!(target_os = "android") {
            if let Some((upgraded_coord, rel_path, url, sha1)) =
                android_upgraded_library_spec(&lib.name)
            {
                let dest = SafePath::resolve_child(&paths.libraries_dir, rel_path)?;
                classpath_libs.push(dest.clone());
                let mut task = DownloadTask::new(url.to_string(), dest);
                task.expected_sha1 = Some(sha1.to_string());
                task.label = upgraded_coord.to_string();
                jar_and_lib_tasks.push(task);
                continue;
            }
        }

        if let Some(art) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            let rel = art
                .path
                .clone()
                .or_else(|| maven_coord_to_rel_path(&lib.name));
            if let Some(rel_path) = rel {
                let dest = SafePath::resolve_child(&paths.libraries_dir, &rel_path)?;
                let is_native_coord = is_modern_native_library_coord(&lib.name);
                if is_native_coord {
                    if matches_current_os_native_coord(&lib.name) {
                        native_jars.push(dest.clone());
                        classpath_libs.push(dest.clone());
                    } else {
                        continue;
                    }
                } else {
                    classpath_libs.push(dest.clone());
                }

                let mut task = DownloadTask::new(art.url.clone(), dest);
                task.expected_sha1 = art.sha1.clone();
                task.expected_size = art.size;
                task.label = lib.name.clone();
                jar_and_lib_tasks.push(task);
            }
        } else if let Some(rel_path) = maven_coord_to_rel_path(&lib.name) {
            let base = lib
                .url
                .as_deref()
                .unwrap_or("https://libraries.minecraft.net/")
                .trim_end_matches('/');
            let dest = SafePath::resolve_child(&paths.libraries_dir, &rel_path)?;
            classpath_libs.push(dest.clone());
            let mut task = DownloadTask::new(format!("{base}/{rel_path}"), dest);
            task.label = lib.name.clone();
            jar_and_lib_tasks.push(task);
        }

        // Legacy (< 1.19) classifier-based natives
        if !cfg!(target_os = "android") {
            if let (Some(natives_map), Some(classifiers)) = (
                lib.natives.as_ref(),
                lib.downloads.as_ref().and_then(|d| d.classifiers.as_ref()),
            ) {
                if let Some(classifier_tpl) = natives_map.get(current_mojang_os_name()) {
                    let arch_bits = if cfg!(target_pointer_width = "64") {
                        "64"
                    } else {
                        "32"
                    };
                    let key = classifier_tpl.replace("${arch}", arch_bits);
                    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                    let chosen = classifiers.get(&key).or_else(|| {
                        classifiers
                            .get("natives-macos-arm64")
                            .or_else(|| classifiers.get("natives-osx"))
                    });
                    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
                    let chosen = classifiers
                        .get("natives-linux-arm64")
                        .or_else(|| classifiers.get("natives-linux-aarch64"))
                        .or_else(|| classifiers.get(&key));
                    #[cfg(not(any(
                        all(target_os = "macos", target_arch = "aarch64"),
                        all(target_os = "linux", target_arch = "aarch64")
                    )))]
                    let chosen = classifiers.get(&key);

                    if let Some(nat_art) = chosen {
                        if let Some(ref rel_path) = nat_art.path {
                            let dest = SafePath::resolve_child(&paths.libraries_dir, rel_path)?;
                            native_jars.push(dest.clone());
                            let mut task = DownloadTask::new(nat_art.url.clone(), dest);
                            task.expected_sha1 = nat_art.sha1.clone();
                            task.expected_size = nat_art.size;
                            task.label = format!("{} ({key})", lib.name);
                            jar_and_lib_tasks.push(task);
                        }
                    }
                }
            }
        }
    }

    downloader
        .download_batch(
            &format!("Libraries & Client ({clean_ver})"),
            jar_and_lib_tasks,
            10,
            cancel.clone(),
            on_progress.clone(),
        )
        .await?;

    if !native_jars.is_empty() {
        let nat_out = natives_dir.clone();
        tokio::task::spawn_blocking(move || -> ThkResult<()> {
            for jar in native_jars {
                if jar.is_file() {
                    let _ = extract_zip_filtered(&jar, &nat_out, |entry_name| {
                        !entry_name.starts_with("META-INF/")
                            && (entry_name.ends_with(".dll")
                                || entry_name.ends_with(".so")
                                || entry_name.ends_with(".dylib")
                                || entry_name.ends_with(".jnilib"))
                    });
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| ThkError::Process(format!("Native library extraction failed: {e}")))??;
    }

    let (asset_index_id, virtual_assets_dir, map_to_resources) =
        download_game_assets(downloader, paths, version_json, cancel, on_progress).await?;

    Ok(PreparedVanillaAssets {
        version_json: version_json.clone(),
        client_jar,
        libraries: classpath_libs,
        natives_dir,
        assets_dir: paths.assets_dir.clone(),
        virtual_assets_dir,
        map_to_resources,
        asset_index_id,
    })
}

#[derive(Deserialize)]
struct AssetIndexFile {
    objects: HashMap<String, AssetObject>,
    #[serde(default)]
    map_to_resources: bool,
    #[serde(rename = "virtual", default)]
    is_virtual: bool,
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

async fn download_game_assets<F>(
    downloader: &DownloadManager,
    paths: &AppPaths,
    version_json: &VanillaVersionJson,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<(String, Option<PathBuf>, bool)>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let Some(ref idx_ref) = version_json.asset_index else {
        return Ok((
            version_json
                .assets
                .clone()
                .unwrap_or_else(|| "legacy".into()),
            None,
            false,
        ));
    };

    let indexes_dir = SafePath::resolve_child(&paths.assets_dir, "indexes")?;
    let objects_dir = SafePath::resolve_child(&paths.assets_dir, "objects")?;
    tokio::fs::create_dir_all(&indexes_dir).await?;
    tokio::fs::create_dir_all(&objects_dir).await?;

    let clean_id = SafePath::validate_single_name(&idx_ref.id)?;
    let idx_path = SafePath::resolve_child(&indexes_dir, format!("{clean_id}.json"))?;

    let mut idx_task = DownloadTask::new(idx_ref.url.clone(), idx_path.clone());
    idx_task.expected_sha1 = idx_ref.sha1.clone();
    idx_task.expected_size = idx_ref.size;
    downloader
        .download_single(&idx_task, cancel.as_ref(), |_, _, _| {})
        .await?;

    let raw_idx = tokio::fs::read_to_string(&idx_path).await?;
    let parsed_idx: AssetIndexFile = serde_json::from_str(&raw_idx)?;

    let virtual_root = if parsed_idx.is_virtual || parsed_idx.map_to_resources {
        Some(SafePath::resolve_child(
            &paths.assets_dir,
            format!("virtual/{clean_id}"),
        )?)
    } else {
        None
    };

    let mut tasks = Vec::with_capacity(parsed_idx.objects.len());
    let mut seen_hashes = std::collections::HashSet::with_capacity(parsed_idx.objects.len());
    let mut virtual_copies = Vec::new();

    for (logical_name, obj) in parsed_idx.objects {
        if obj.hash.len() < 2 {
            continue;
        }
        let prefix = &obj.hash[0..2];
        let obj_dest = SafePath::resolve_child(&objects_dir, format!("{prefix}/{}", obj.hash))?;

        if seen_hashes.insert(obj.hash.clone()) && !obj_dest.is_file() {
            let url = format!("{MOJANG_RESOURCES_BASE}/{prefix}/{}", obj.hash);
            let mut t = DownloadTask::new(url, obj_dest.clone());
            t.expected_sha1 = Some(obj.hash.clone());
            t.expected_size = Some(obj.size);
            t.label = logical_name.clone();
            tasks.push(t);
        }

        if let Some(ref vroot) = virtual_root {
            if let Ok(vdest) = SafePath::resolve_child(vroot, &logical_name) {
                virtual_copies.push((obj_dest, vdest));
            }
        }
    }

    downloader
        .download_batch(
            &format!("Game Assets ({clean_id})"),
            tasks,
            16,
            cancel,
            on_progress,
        )
        .await?;

    if !virtual_copies.is_empty() {
        tokio::task::spawn_blocking(move || {
            for (src, dst) in virtual_copies {
                if !dst.is_file() && src.is_file() {
                    if let Some(p) = dst.parent() {
                        let _ = std::fs::create_dir_all(p);
                    }
                    let _ = std::fs::copy(&src, &dst);
                }
            }
        })
        .await
        .ok();
    }

    Ok((
        idx_ref.id.clone(),
        virtual_root,
        parsed_idx.map_to_resources,
    ))
}

pub fn evaluate_rules(rules: Option<&[MojangRule]>, features: &HashMap<String, bool>) -> bool {
    let Some(rule_list) = rules else {
        return true;
    };
    if rule_list.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rule_list {
        let mut rule_matches = true;

        if let Some(ref os) = rule.os {
            if let Some(ref req_name) = os.name {
                if req_name != current_mojang_os_name() {
                    rule_matches = false;
                }
            }
            if let Some(ref req_arch) = os.arch {
                if !arch_matches(req_arch) {
                    rule_matches = false;
                }
            }
        }

        if let Some(ref req_feats) = rule.features {
            for (k, expected_val) in req_feats {
                let actual = features.get(k).copied().unwrap_or(false);
                if actual != *expected_val {
                    rule_matches = false;
                    break;
                }
            }
        }

        if rule_matches {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

pub fn current_mojang_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn arch_matches(rule_arch: &str) -> bool {
    match rule_arch {
        "x86" => cfg!(target_arch = "x86"),
        "x86_64" | "amd64" => cfg!(target_arch = "x86_64"),
        "arm64" | "aarch64" => cfg!(target_arch = "aarch64"),
        _ => false,
    }
}

fn is_modern_native_library_coord(coord: &str) -> bool {
    coord.contains(":natives-")
}

pub fn android_upgraded_library_spec(
    coord: &str,
) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    let parts: Vec<&str> = coord.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let ver_nums: Vec<u32> = parts[2]
        .split('.')
        .map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0)
        })
        .collect();
    let major = ver_nums.first().copied().unwrap_or(0);
    let minor = ver_nums.get(1).copied().unwrap_or(0);

    if coord.starts_with("net.java.dev.jna:jna:") && (major < 5 || (major == 5 && minor < 14)) {
        return Some((
            "net.java.dev.jna:jna:5.14.0",
            "net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar",
            "https://repo1.maven.org/maven2/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar",
            "67bf3eaea4f0718cb376a181a629e5f88fa1c9dd",
        ));
    }
    if coord.starts_with("net.java.dev.jna:jna-platform:")
        && (major < 5 || (major == 5 && minor < 14))
    {
        return Some((
            "net.java.dev.jna:jna-platform:5.14.0",
            "net/java/dev/jna/jna-platform/5.14.0/jna-platform-5.14.0.jar",
            "https://repo1.maven.org/maven2/net/java/dev/jna/jna-platform/5.14.0/jna-platform-5.14.0.jar",
            "28934d48aed814f11e4c584da55c49fa7032b31b",
        ));
    }
    if coord.starts_with("com.github.oshi:oshi-core:") && major == 6 && minor == 2 {
        return Some((
            "com.github.oshi:oshi-core:6.3.0",
            "com/github/oshi/oshi-core/6.3.0/oshi-core-6.3.0.jar",
            "https://repo1.maven.org/maven2/com/github/oshi/oshi-core/6.3.0/oshi-core-6.3.0.jar",
            "9e98cf55be371cafdb9c70c35d04ec2a8c2b42ac",
        ));
    }
    if coord.starts_with("org.ow2.asm:asm-all:") && major < 5 {
        return Some((
            "org.ow2.asm:asm-all:5.0.4",
            "org/ow2/asm/asm-all/5.0.4/asm-all-5.0.4.jar",
            "https://repo1.maven.org/maven2/org/ow2/asm/asm-all/5.0.4/asm-all-5.0.4.jar",
            "e6244859997b3d4237a552669279780876228909",
        ));
    }
    None
}

fn matches_current_os_native_coord(coord: &str) -> bool {
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        coord.contains(":natives-windows-arm64")
    }
    #[cfg(all(target_os = "windows", target_arch = "x86"))]
    {
        coord.contains(":natives-windows-x86")
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        coord.contains(":natives-windows")
            && !coord.contains(":natives-windows-arm64")
            && !coord.contains(":natives-windows-x86")
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        coord.contains(":natives-macos-arm64")
    }
    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    {
        (coord.contains(":natives-macos") || coord.contains(":natives-osx"))
            && !coord.contains(":natives-macos-arm64")
    }
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        target_arch = "aarch64"
    ))]
    {
        coord.contains(":natives-linux-arm64") || coord.contains(":natives-linux-aarch64")
    }
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        not(target_arch = "aarch64")
    ))]
    {
        coord.contains(":natives-linux")
            && !coord.contains(":natives-linux-arm")
            && !coord.contains(":natives-linux-aarch64")
    }
}

pub fn maven_coord_to_rel_path(coord: &str) -> Option<String> {
    let (base, ext) = match coord.split_once('@') {
        Some((b, e)) => (b, e),
        None => (coord, "jar"),
    };
    let parts: Vec<&str> = base.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let file_name = if parts.len() >= 4 && !parts[3].is_empty() {
        format!("{artifact}-{version}-{}.{ext}", parts[3])
    } else {
        format!("{artifact}-{version}.{ext}")
    };
    Some(format!("{group}/{artifact}/{version}/{file_name}"))
}

pub fn library_dedup_key(jar_path: &Path) -> String {
    let norm = jar_path.to_string_lossy().replace('\\', "/");
    if let Some((_, after_libs)) = norm.rsplit_once("/libraries/") {
        let segs: Vec<&str> = after_libs.split('/').collect();
        if segs.len() >= 3 {
            let group_artifact = segs[..segs.len() - 2].join("/");
            let version = segs[segs.len() - 2];
            let fname = segs[segs.len() - 1];
            let artifact = segs[segs.len() - 3];
            let prefix = format!("{artifact}-{version}");
            let classifier = fname
                .strip_prefix(&prefix)
                .and_then(|s| s.strip_suffix(".jar"))
                .unwrap_or("");
            return format!("{group_artifact}{classifier}");
        }
    }
    norm
}
