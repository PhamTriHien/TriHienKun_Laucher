use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use thk_core::models::{ContentKind, DownloadProgress, GameProfile, LoaderType, ProviderSource};
use thk_core::{Database, DownloadManager, ThkError, ThkResult};
use thk_mods::{
    fetch_provider_versions, install_modrinth_content, search_provider, SearchQuery, SearchSort,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashCategory {
    FabricIncompatible,
    ForgeMissingDep,
    OutOfMemory,
    JavaVersionMismatch,
    JvmNativeCrash,
    JavaException,
    UnknownExitCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashFixAction {
    InstallMod,
    UpdateMod,
    UpgradeJava,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingModAdvice {
    pub mod_id: String,
    pub display_name: String,
    pub required_version: Option<String>,
    pub required_by: Vec<String>,
    pub action: CrashFixAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashAnalysisReport {
    pub category: CrashCategory,
    pub exit_code: i32,
    pub summary_message: String,
    pub root_cause_line: Option<String>,
    pub solutions: Vec<String>,
    pub missing_mods: Vec<MissingModAdvice>,
    pub relevant_lines: Vec<String>,
}

struct CrashRegexes {
    pat_install: Regex,
    pat_replace: Regex,
    pat_req_missing: Regex,
    pat_req_of: Regex,
    pat_req_current: Regex,
    pat_hard_dep: Regex,
    pat_forge_dep: Regex,
}

fn regexes() -> &'static CrashRegexes {
    static INSTANCE: OnceLock<CrashRegexes> = OnceLock::new();
    INSTANCE.get_or_init(|| CrashRegexes {
        pat_install: Regex::new(r"(?i)^[-*]\s+Install\s+([\w\-]+),\s+(.+?)\.?\s*$").unwrap(),
        pat_replace: Regex::new(
            r#"(?i)^[-*]\s+Replace\s+mod\s+['"]?([^'"(]+)['"]?\s*\(([^)]+)\)\s+([\w.+\-]+)\s+with"#,
        )
        .unwrap(),
        pat_req_missing: Regex::new(
            r#"(?i)^[-*]?\s*Mod\s+['"]?([^'"(]+)['"]?\s*\(([^)]+)\)[^r]*requires\s+(?:version\s+)?([\w.+\-\[\],\s()]+?)\s+of\s+['"]?([\w\-\s]+?)['"]?,?\s+which\s+is\s+missing"#,
        )
        .unwrap(),
        pat_req_of: Regex::new(
            r#"(?i)requires\s+version\s+([\w.+\-]+)\s+or\s+later\s+of\s+['"]?([\w\-]+)['"]?"#,
        )
        .unwrap(),
        pat_req_current: Regex::new(
            r#"(?i)Mod\s+['"]?([^'"(]+)['"]?\s*\(([^)]+)\)\s+requires\s+version\s+([\w.+\-]+)\+?\s+of\s+['"]?([\w\-]+)['"]?\s+\(currently"#,
        )
        .unwrap(),
        pat_hard_dep: Regex::new(
            r"(?i)HARD_DEP(?:_NO_CANDIDATE)?\s+([\w\-]+)\s+[\w.+\-]+\s+\{depends\s+([\w\-]+)\s+@\s+\[([^\]]+)\]",
        )
        .unwrap(),
        pat_forge_dep: Regex::new(r"(?i)Mod ID:\s*'([\w\-]+)',\s*Requested by:\s*'([\w\-]+)'").unwrap(),
    })
}

pub fn analyze_crash_logs(logs: &[String], exit_code: i32) -> CrashAnalysisReport {
    let full = logs.join("\n");
    let lower_full = full.to_ascii_lowercase();

    let is_fabric_incompat = logs.iter().any(|l| {
        let low = l.to_ascii_lowercase();
        low.contains("incompatible mod")
            || low.contains("some of your mods are incompatible")
            || (low.contains("formattedexception") && low.contains("fabricmc"))
    });

    let category = if is_fabric_incompat {
        CrashCategory::FabricIncompatible
    } else if lower_full.contains("java.lang.outofmemoryerror") {
        CrashCategory::OutOfMemory
    } else if lower_full.contains("unsupportedclassversionerror")
        || lower_full.contains("compiled by a more recent version of the java runtime")
    {
        CrashCategory::JavaVersionMismatch
    } else if lower_full.contains("missing or unsupported mandatory dependencies")
        || lower_full.contains("loadingmodlist")
    {
        CrashCategory::ForgeMissingDep
    } else if lower_full.contains("hs_err_pid")
        || lower_full.contains("exception_access_violation")
    {
        CrashCategory::JvmNativeCrash
    } else if lower_full.contains("exception in thread") || lower_full.contains("caused by:") {
        CrashCategory::JavaException
    } else {
        CrashCategory::UnknownExitCode
    };

    let (solutions, mut missing_mods) = parse_fabric_and_forge_advice(logs);
    if category == CrashCategory::JavaVersionMismatch && missing_mods.is_empty() {
        missing_mods.push(MissingModAdvice {
            mod_id: "java".into(),
            display_name: "Java Runtime".into(),
            required_version: Some("Java 17 / 21+".into()),
            required_by: Vec::new(),
            action: CrashFixAction::UpgradeJava,
        });
    }

    let summary_message = match category {
        CrashCategory::FabricIncompatible => {
            "Phát hiện xung đột hoặc thiếu thư viện phụ thuộc giữa các Mod Fabric/Quilt.".into()
        }
        CrashCategory::ForgeMissingDep => {
            "Forge/NeoForge báo thiếu Mod thư viện bắt buộc để khởi động.".into()
        }
        CrashCategory::OutOfMemory => {
            "Game bị tràn bộ nhớ RAM (OutOfMemoryError). Hãy tăng mức cấp phát RAM cho Profile."
                .into()
        }
        CrashCategory::JavaVersionMismatch => {
            "Phiên bản Java hiện tại quá thấp so với yêu cầu của Minecraft hoặc Mod.".into()
        }
        CrashCategory::JvmNativeCrash => {
            "Tiến trình JVM gặp lỗi Native/Driver đồ họa (EXCEPTION_ACCESS_VIOLATION).".into()
        }
        CrashCategory::JavaException => {
            format!("Minecraft gặp ngoại lệ Java nghiêm trọng (mã thoát {exit_code}).")
        }
        CrashCategory::UnknownExitCode => {
            format!("Minecraft dừng đột ngột với mã thoát {exit_code}.")
        }
    };

    let root_cause_line = extract_root_cause(logs);
    let relevant_lines = collect_relevant_lines(logs);

    CrashAnalysisReport {
        category,
        exit_code,
        summary_message,
        root_cause_line,
        solutions,
        missing_mods,
        relevant_lines,
    }
}

fn parse_fabric_and_forge_advice(logs: &[String]) -> (Vec<String>, Vec<MissingModAdvice>) {
    let rx = regexes();
    let mut missing: BTreeMap<String, MissingModAdvice> = BTreeMap::new();
    let mut solutions = Vec::new();
    let mut in_solutions = false;

    for line in logs {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();

        if lower.contains("a potential solution has been determined") {
            in_solutions = true;
            continue;
        }
        if lower.contains("incompatible mods") || lower.contains("unmet dependency listing") {
            in_solutions = false;
        }
        if in_solutions && (trimmed.starts_with("- ") || trimmed.starts_with("* ")) {
            solutions.push(trimmed[2..].trim().to_string());
        }

        if let Some(cap) = rx.pat_install.captures(trimmed) {
            let mod_id = cap[1].trim().to_ascii_lowercase();
            let ver_desc = cap[2].trim().to_string();
            let action = if is_java_token(&mod_id) {
                CrashFixAction::UpgradeJava
            } else {
                CrashFixAction::InstallMod
            };
            missing.entry(mod_id.clone()).or_insert_with(|| MissingModAdvice {
                display_name: cap[1].trim().to_string(),
                mod_id,
                required_version: Some(ver_desc),
                required_by: Vec::new(),
                action,
            });
            continue;
        }

        if let Some(cap) = rx.pat_replace.captures(trimmed) {
            let disp = cap[1].trim().to_string();
            let mod_id = cap[2].trim().to_ascii_lowercase();
            if !is_java_token(&mod_id) {
                missing.entry(mod_id.clone()).or_insert_with(|| MissingModAdvice {
                    mod_id,
                    display_name: disp,
                    required_version: Some(format!("current: {}", cap[3].trim())),
                    required_by: Vec::new(),
                    action: CrashFixAction::UpdateMod,
                });
            }
            continue;
        }

        if let Some(cap) = rx.pat_req_missing.captures(trimmed) {
            let req_by = cap[1].trim().to_string();
            let req_ver = cap[3].trim().to_string();
            let raw_target = cap[4].trim();
            let key = raw_target.to_ascii_lowercase().replace(' ', "-");
            let action = if is_java_token(&key) {
                CrashFixAction::UpgradeJava
            } else {
                CrashFixAction::InstallMod
            };
            let entry = missing.entry(key.clone()).or_insert_with(|| MissingModAdvice {
                mod_id: key,
                display_name: raw_target.to_string(),
                required_version: Some(req_ver),
                required_by: Vec::new(),
                action,
            });
            if !entry.required_by.contains(&req_by) {
                entry.required_by.push(req_by);
            }
            continue;
        }

        if let Some(cap) = rx.pat_req_of.captures(trimmed) {
            let ver = cap[1].trim().to_string();
            let mod_id = cap[2].trim().to_ascii_lowercase();
            if !is_java_token(&mod_id) {
                missing.entry(mod_id.clone()).or_insert_with(|| MissingModAdvice {
                    display_name: cap[2].trim().to_string(),
                    mod_id,
                    required_version: Some(format!("{ver}+")),
                    required_by: Vec::new(),
                    action: CrashFixAction::InstallMod,
                });
            }
            continue;
        }

        if let Some(cap) = rx.pat_req_current.captures(trimmed) {
            let req_by = cap[1].trim().to_string();
            let ver = cap[3].trim().to_string();
            let mod_id = cap[4].trim().to_ascii_lowercase();
            if !is_java_token(&mod_id) {
                let entry = missing.entry(mod_id.clone()).or_insert_with(|| MissingModAdvice {
                    display_name: cap[4].trim().to_string(),
                    mod_id,
                    required_version: Some(ver),
                    required_by: Vec::new(),
                    action: CrashFixAction::UpdateMod,
                });
                if !entry.required_by.contains(&req_by) {
                    entry.required_by.push(req_by);
                }
            }
            continue;
        }

        if let Some(cap) = rx.pat_hard_dep.captures(trimmed) {
            let req_by = cap[1].trim().to_string();
            let mod_id = cap[2].trim().to_ascii_lowercase();
            let range = cap[3].trim().to_string();
            if !is_java_token(&mod_id) {
                let entry = missing.entry(mod_id.clone()).or_insert_with(|| MissingModAdvice {
                    display_name: cap[2].trim().to_string(),
                    mod_id,
                    required_version: Some(range),
                    required_by: Vec::new(),
                    action: CrashFixAction::InstallMod,
                });
                if !entry.required_by.contains(&req_by) {
                    entry.required_by.push(req_by);
                }
            }
            continue;
        }

        if let Some(cap) = rx.pat_forge_dep.captures(trimmed) {
            let mod_id = cap[1].trim().to_ascii_lowercase();
            let req_by = cap[2].trim().to_string();
            if mod_id != "minecraft" && mod_id != "forge" && mod_id != "neoforge" && !is_java_token(&mod_id) {
                let entry = missing.entry(mod_id.clone()).or_insert_with(|| MissingModAdvice {
                    display_name: cap[1].trim().to_string(),
                    mod_id,
                    required_version: None,
                    required_by: Vec::new(),
                    action: CrashFixAction::InstallMod,
                });
                if !entry.required_by.contains(&req_by) {
                    entry.required_by.push(req_by);
                }
            }
        }
    }

    (solutions, missing.into_values().collect())
}

fn is_java_token(key: &str) -> bool {
    matches!(key, "java" | "openjdk" | "jdk" | "jre")
}

fn extract_root_cause(logs: &[String]) -> Option<String> {
    let mut last_caused = None;
    for l in logs {
        let t = l.trim();
        if t.starts_with("Caused by:") || t.starts_with("Exception in thread") {
            last_caused = Some(t.to_string());
        }
    }
    if last_caused.is_some() {
        return last_caused;
    }
    logs.iter()
        .find(|l| {
            let u = l.to_ascii_uppercase();
            (u.contains("FATAL") || u.contains("ERROR")) && l.trim().len() > 16
        })
        .map(|s| s.trim().to_string())
}

fn collect_relevant_lines(logs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = logs
        .iter()
        .filter(|l| {
            let low = l.to_ascii_lowercase();
            low.contains("error")
                || low.contains("fatal")
                || low.contains("exception")
                || low.contains("caused by:")
                || low.contains("requires ")
                || low.contains("install ")
                || low.contains("incompatible")
        })
        .take(60)
        .cloned()
        .collect();

    if out.is_empty() {
        let start = logs.len().saturating_sub(40);
        out.extend_from_slice(&logs[start..]);
    }
    out
}

pub async fn auto_fix_mod_from_modrinth<F>(
    downloader: &DownloadManager,
    db: &Database,
    profile: &GameProfile,
    mod_id_query: &str,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<Vec<PathBuf>>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let mapped_query = match mod_id_query {
        "fabric" | "fabricloader" => "fabric-api",
        other => other,
    };

    let loader = (profile.loader != LoaderType::Vanilla).then_some(profile.loader);
    let page = search_provider(
        downloader,
        ProviderSource::Modrinth,
        &SearchQuery {
            query: mapped_query.to_string(),
            content_kind: ContentKind::Mod,
            game_version: Some(profile.game_version.clone()),
            loader,
            sort: SearchSort::Relevance,
            offset: 0,
            limit: 5,
        },
        None,
    )
    .await?;

    let chosen_summary = page
        .hits
        .iter()
        .find(|h| {
            h.slug.eq_ignore_ascii_case(mapped_query)
                || h.slug.replace('-', "") == mapped_query.replace(['-', '_'], "")
        })
        .or_else(|| page.hits.first())
        .cloned()
        .ok_or_else(|| {
            ThkError::Provider(format!(
                "Không tìm thấy mod '{mapped_query}' tương thích với {} ({}) trên Modrinth.",
                profile.game_version,
                profile.loader.display_name()
            ))
        })?;

    let versions = fetch_provider_versions(
        downloader,
        ProviderSource::Modrinth,
        &chosen_summary.project_id,
        Some(&profile.game_version),
        loader,
        None,
    )
    .await?;

    let target_ver = versions.first().ok_or_else(|| {
        ThkError::Provider(format!(
            "Mod '{}' chưa có bản phát hành cho Minecraft {}.",
            chosen_summary.title, profile.game_version
        ))
    })?;

    install_modrinth_content(
        downloader,
        db,
        profile,
        ContentKind::Mod,
        &chosen_summary,
        target_ver,
        cancel,
        on_progress,
    )
    .await
}
