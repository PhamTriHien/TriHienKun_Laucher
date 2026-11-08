use regex::Regex;
use std::sync::OnceLock;
use thk_core::models::ServerEngine;

pub fn supports_vanilla_tick_query(game_version: &str) -> bool {
    let trimmed = game_version.trim();
    let core = trimmed
        .split(['-', '+', ' '])
        .next()
        .unwrap_or(trimmed)
        .trim();
    let mut parts = core.split('.');
    let Some(major) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    if major >= 26 {
        return true;
    }
    if major != 1 {
        return false;
    }
    let Some(minor) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    if minor > 20 {
        return true;
    }
    if minor < 20 {
        return false;
    }
    let patch = parts
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    patch >= 3
}

pub fn tps_probe_command_for_server(
    engine: ServerEngine,
    game_version: &str,
) -> Option<&'static str> {
    match engine {
        ServerEngine::Paper
        | ServerEngine::Purpur
        | ServerEngine::Folia
        | ServerEngine::Arclight
        | ServerEngine::Mohist => Some("tps"),
        ServerEngine::Forge => Some("forge tps"),
        ServerEngine::NeoForge => Some("neoforge tps"),
        ServerEngine::Vanilla | ServerEngine::Fabric | ServerEngine::Sponge => {
            supports_vanilla_tick_query(game_version).then_some("tick query")
        }
    }
}

pub fn fallback_tps_probe_command(current_cmd: &str, game_version: &str) -> Option<&'static str> {
    if matches!(current_cmd, "forge tps" | "neoforge tps")
        && supports_vanilla_tick_query(game_version)
    {
        Some("tick query")
    } else {
        None
    }
}

pub fn parse_tps_or_mspt_line(line: &str) -> Option<(Option<f32>, Option<f32>)> {
    static PAPER_TPS_RE: OnceLock<Regex> = OnceLock::new();
    static FORGE_OVERALL_RE: OnceLock<Regex> = OnceLock::new();
    static TICK_QUERY_RE: OnceLock<Regex> = OnceLock::new();

    let paper_re = PAPER_TPS_RE.get_or_init(|| {
        Regex::new(r"(?i)TPS from last [^\r\n:]+:\s*\*?([0-9]+(?:\.[0-9]+)?)")
            .expect("valid paper tps regex")
    });
    if let Some(caps) = paper_re.captures(line) {
        if let Ok(tps) = caps[1].parse::<f32>() {
            if tps.is_finite() && (0.0..=200.0).contains(&tps) {
                return Some((Some(tps), None));
            }
        }
    }

    let forge_re = FORGE_OVERALL_RE.get_or_init(|| {
        Regex::new(
            r"(?i)Overall:\s*Mean tick time:\s*([0-9]+(?:\.[0-9]+)?)\s*ms\.?\s*Mean TPS:\s*([0-9]+(?:\.[0-9]+)?)",
        )
        .expect("valid forge tps regex")
    });
    if let Some(caps) = forge_re.captures(line) {
        if let (Ok(mspt), Ok(tps)) = (caps[1].parse::<f32>(), caps[2].parse::<f32>()) {
            if mspt.is_finite() && mspt >= 0.0 && tps.is_finite() && (0.0..=200.0).contains(&tps) {
                return Some((Some(tps), Some(mspt)));
            }
        }
    }

    let tick_re = TICK_QUERY_RE.get_or_init(|| {
        Regex::new(
            r"(?i)Average time per tick:\s*([0-9]+(?:\.[0-9]+)?)\s*ms(?:\s*\(Target:\s*([0-9]+(?:\.[0-9]+)?)\s*ms\))?",
        )
        .expect("valid tick query regex")
    });
    if let Some(caps) = tick_re.captures(line) {
        if let Ok(mspt) = caps[1].parse::<f32>() {
            if mspt.is_finite() && mspt >= 0.0 {
                let target_ms = caps
                    .get(2)
                    .and_then(|m| m.as_str().parse::<f32>().ok())
                    .filter(|&v| v.is_finite() && v > 0.0)
                    .unwrap_or(50.0);
                let target_tps = 1000.0 / target_ms;
                let effective_tps = if mspt <= 0.001 {
                    target_tps
                } else {
                    (1000.0 / mspt).min(target_tps)
                };
                return Some((Some(effective_tps), Some(mspt)));
            }
        }
    }

    None
}

pub fn is_unknown_command_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("unknown or incomplete command")
        || lower.contains("unknown command")
        || lower.contains("<--[here]")
}

pub fn is_automated_tps_probe_noise(line: &str) -> bool {
    static FORGE_DIM_RE: OnceLock<Regex> = OnceLock::new();
    let dim_re = FORGE_DIM_RE.get_or_init(|| {
        Regex::new(r"Mean tick time:\s*[0-9]+(?:\.[0-9]+)?\s*ms\.\s*Mean TPS:")
            .expect("valid forge dim tps regex")
    });

    if parse_tps_or_mspt_line(line).is_some() || dim_re.is_match(line) {
        return true;
    }

    let payload = strip_log_prefix(line);
    payload.starts_with("Current Memory Usage:")
        || payload.starts_with("Server is running normally")
        || payload.starts_with("Server is frozen")
        || payload.starts_with("Server is sprinting")
        || payload.starts_with("Target tick rate:")
        || payload.starts_with("Percentiles: P50:")
}

pub fn extract_player_join_or_leave(line: &str) -> Option<(String, bool)> {
    let payload = strip_log_prefix(line);
    if let Some(name) = payload.strip_suffix(" joined the game") {
        let clean = name.trim();
        if is_plausible_mc_username(clean) {
            return Some((clean.to_string(), true));
        }
    }
    if let Some(name) = payload.strip_suffix(" left the game") {
        let clean = name.trim();
        if is_plausible_mc_username(clean) {
            return Some((clean.to_string(), false));
        }
    }
    None
}

pub fn strip_log_prefix(line: &str) -> &str {
    if let Some((_, tail)) = line.rsplit_once("]: ") {
        tail.trim()
    } else {
        line.trim()
    }
}

fn is_plausible_mc_username(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && !name.contains(char::is_whitespace)
        && !name.starts_with('<')
        && !name.starts_with('[')
}
