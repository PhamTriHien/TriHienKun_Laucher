use crate::fx3d::{
    gaming_card_3d, paint_3d_tunnel_conduit, paint_glowing_progress_bar, paint_icon_3d,
    sliding_pill_tabs, tactile_button_3d, ButtonStyle3d, Icon3dKind, VectorIcon,
};
use crate::state::{LauncherAppState, ServerTab};
use crate::theme::{
    readable_accent_ink, ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_RED,
    ACCENT_VIOLET, BG_SLOT_LIGHT, BORDER_GAME_OUTER_LIGHT, TEXT_INK_LIGHT,
};
use crate::views::{
    badge, danger_icon_button, format_bytes, format_duration_hms, network_interface_selector_ui,
    primary_icon_button, render_direct_host_endpoints_ui, secondary_button, secondary_icon_button,
};
use egui::{
    pos2, vec2, Color32, CornerRadius, Frame, Margin, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind,
};
use std::collections::{BTreeMap, HashSet};
use thk_core::models::{ContentKind, Language, ManagedServer, ProviderSource};
use thk_core::I18n;
use thk_network::{DirectHostStatus, TunnelStatus};
use thk_server::{
    resolve_effective_server_port, CpuHardwareInfo, ServerResourceStats, ServerRuntimeState,
};

const MC_SERVER_AUTOCOMPLETE_CMDS: &[&str] = &[
    "help",
    "list",
    "tps",
    "tick query",
    "stop",
    "save-all",
    "save-on",
    "save-off",
    "whitelist add ",
    "whitelist remove ",
    "whitelist list",
    "whitelist on",
    "whitelist off",
    "op ",
    "deop ",
    "gamemode creative ",
    "gamemode survival ",
    "gamemode adventure ",
    "gamemode spectator ",
    "clear ",
    "tp ",
    "give ",
    "time set day",
    "time set night",
    "weather clear",
    "ban ",
    "pardon ",
    "ban-ip ",
    "pardon-ip ",
    "kick ",
    "setidletimeout ",
    "difficulty normal",
    "gamerule keepInventory true",
];

#[cfg(all(test, target_os = "windows"))]
#[path = "visual_tests.rs"]
mod visual_tests;

use crate::views::bounded_wrap_label;

fn paint_addon_thumbnail(
    ui: &mut egui::Ui,
    tex_opt: Option<&egui::TextureHandle>,
    size: f32,
    accent: Color32,
) {
    if let Some(tex) = tex_opt {
        let radius = CornerRadius::same(8);
        let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
        let slot_bg = if ui.visuals().dark_mode {
            Color32::from_rgb(20, 29, 44)
        } else {
            BG_SLOT_LIGHT
        };
        ui.painter().rect_filled(rect, radius, slot_bg);

        let sz = tex.size_vec2();
        if sz.x > 0.0 && sz.y > 0.0 && rect.width() > 0.0 && rect.height() > 0.0 {
            let src_aspect = sz.x / sz.y;
            let dst_aspect = rect.width() / rect.height();
            let uv = if src_aspect > dst_aspect {
                let vis_w = dst_aspect / src_aspect;
                let pad = (1.0 - vis_w) * 0.5;
                Rect::from_min_max(pos2(pad, 0.0), pos2(1.0 - pad, 1.0))
            } else {
                let vis_h = src_aspect / dst_aspect;
                let pad = (1.0 - vis_h) * 0.5;
                Rect::from_min_max(pos2(0.0, pad), pos2(1.0, 1.0 - pad))
            };
            ui.painter()
                .with_clip_rect(rect)
                .image(tex.id(), rect, uv, Color32::WHITE);
        }

        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 145),
            ),
            StrokeKind::Inside,
        );
    } else {
        paint_icon_3d(ui, size, Icon3dKind::ModpackCrate, false);
    }
}

pub fn render_servers_view(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let compact = crate::views::is_compact_portrait(ui.max_rect());
    if !compact {
        let avail_w = ui.available_width();
        let min_side_w = 176.0_f32.min((avail_w * 0.38).max(140.0));
        let max_side_w = (avail_w - 260.0).min(avail_w * 0.55).max(min_side_w);
        let def_side_w =
            if avail_w < 640.0 || (cfg!(target_os = "android") && ui.available_height() < 540.0) {
                212.0_f32
            } else {
                262.0_f32
            }
            .clamp(min_side_w, max_side_w);
        let sidebar_id = if ui.ctx().viewport_id() == egui::ViewportId::ROOT {
            egui::Id::unique("thk_servers_sidebar")
        } else {
            ui.scope_id().with("thk_servers_sidebar")
        };
        egui::Panel::left(sidebar_id)
            .resizable(!cfg!(target_os = "android"))
            .default_size(def_side_w)
            .size_range(min_side_w..=max_side_w)
            .frame(Frame::NONE.inner_margin(Margin {
                left: 0,
                right: 12,
                top: 0,
                bottom: 0,
            }))
            .show(ui, |ui| {
                crate::views::locked_panel_body(ui, |ui| {
                    ui.heading(RichText::new(app.i18n.t("nav.servers")).size(17.5).strong());
                    ui.add_space(6.0);

                    let btn_w = ui.available_width();
                    if tactile_button_3d(
                        ui,
                        Some(VectorIcon::Plus),
                        app.i18n.t("server.create"),
                        ButtonStyle3d::PrimaryEmerald,
                        vec2(btn_w, 34.0),
                    )
                    .clicked()
                    {
                        app.click_sfx();
                        let eng = app.servers.new_draft.engine;
                        app.ensure_engine_versions(eng);
                        app.show_new_server_modal = true;
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    let servers_list = app.servers.servers.clone();
                    let sel_id = app.servers.selected_server_id.clone();

                    let sidebar_scroll_w = ui.available_width();
                    ScrollArea::vertical()
                        .id_salt("servers_sidebar_scroll")
                        .show(ui, |ui| {
                            ui.set_max_width(sidebar_scroll_w);
                            if servers_list.is_empty() {
                                bounded_wrap_label(
                                    ui,
                                    RichText::new(app.i18n.tr(
                                        "Chưa có máy chủ cục bộ nào. Nhấn 'Tạo Máy chủ mới' để bắt đầu.",
                                        "No local servers yet. Click 'Create Server' to get started.",
                                    ))
                                    .weak(),
                                    sidebar_scroll_w.max(48.0),
                                );
                            }

                            for srv in servers_list {
                                let is_sel = sel_id.as_deref() == Some(&srv.id);
                                let rt_state = app
                                    .servers
                                    .server_states
                                    .get(&srv.id)
                                    .copied()
                                    .unwrap_or_else(|| {
                                        app.server_supervisor.runtime_state(&srv.id)
                                    });
                                let (st_label, st_color) =
                                    runtime_state_badge_info(rt_state, &app.i18n);
                                let card_accent = if rt_state == ServerRuntimeState::Online {
                                    ACCENT_EMERALD
                                } else if is_sel {
                                    ACCENT_CYAN
                                } else {
                                    st_color
                                };

                                let (_, resp) = gaming_card_3d(
                                    ui,
                                    ("srv_sidebar_card", &srv.id),
                                    is_sel,
                                    card_accent,
                                    Margin::symmetric(10, 8),
                                    |ui| {
                                        let card_w = ui.available_width();
                                        ui.set_width(card_w);
                                        ui.horizontal_wrapped(|ui| {
                                            bounded_wrap_label(
                                                ui,
                                                RichText::new(&srv.name).strong().size(13.6),
                                                (card_w - 4.0).max(48.0),
                                            );
                                            badge(ui, st_label, st_color);
                                        });
                                        ui.add_space(2.0);
                                        ui.horizontal_wrapped(|ui| {
                                            badge(ui, srv.engine.display_name(), ACCENT_BLUE);
                                            badge(ui, &srv.game_version, ACCENT_AMBER);
                                            ui.label(
                                                RichText::new(format!(":{}", srv.port))
                                                    .size(11.0)
                                                    .weak(),
                                            );
                                        });
                                    },
                                );

                                if resp.interact(Sense::click()).clicked() {
                                    app.click_sfx();
                                    app.select_server(srv.id);
                                }
                                ui.add_space(5.0);
                            }
                        });
                });
            });
    }

    let viewport_h = ui.available_height();
    let scroll_w = ui.available_width();
    ScrollArea::vertical()
        .id_salt("servers_workspace_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(scroll_w);
            if compact {
                render_compact_server_bar(app, ui);
                ui.add_space(6.0);
            }

            let Some(srv) = app.selected_server().cloned() else {
                let top_pad = if viewport_h < 460.0 { 16.0 } else { 56.0 };
                ui.vertical_centered(|ui| {
                    ui.set_max_width(scroll_w);
                    ui.add_space(top_pad);
                    paint_icon_3d(ui, 76.0, Icon3dKind::ServerBladeTower, true);
                    ui.add_space(10.0);
                    ui.heading(
                        RichText::new(app.i18n.tr(
                            "Trình Quản lý Máy chủ Minecraft Chuyên dụng",
                            "Dedicated Minecraft Server Manager",
                        ))
                        .size(21.0)
                        .strong(),
                    );
                    ui.add_space(14.0);
                    if primary_icon_button(ui, VectorIcon::Plus, app.i18n.t("server.create"))
                        .clicked()
                    {
                        let eng = app.servers.new_draft.engine;
                        app.ensure_engine_versions(eng);
                        app.show_new_server_modal = true;
                    }
                });
                return;
            };

            render_server_header(app, ui, &srv);
            ui.add_space(8.0);
            render_server_tabs_bar(app, ui);
            ui.add_space(8.0);
            render_active_server_tab(app, ui, &srv, true, viewport_h);
        });
}

fn render_compact_server_bar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let servers_list = app.servers.servers.clone();
    let sel_id = app.servers.selected_server_id.clone();
    let current_txt = servers_list
        .iter()
        .find(|s| sel_id.as_deref() == Some(s.id.as_str()))
        .map(|s| format!("{} ({} • :{})", s.name, s.engine.display_name(), s.port))
        .unwrap_or_else(|| app.i18n.tr("Chưa có máy chủ", "No server").to_string());

    gaming_card_3d(
        ui,
        "servers_compact_switcher",
        false,
        ACCENT_CYAN,
        Margin::symmetric(10, 7),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let cap_w = (ui.max_rect().width() - 12.0).max(84.0);
                let combo_w = (ui.available_width() - 92.0)
                    .clamp(140.0_f32.min(cap_w), 340.0)
                    .min(cap_w);
                let mut picked_id: Option<String> = None;
                let a11y_srv_lbl = app.i18n.t("nav.servers").to_string();
                let combo_resp = egui::ComboBox::from_id_salt("servers_compact_combo")
                    .width(combo_w)
                    .truncate()
                    .selected_text(&current_txt)
                    .show_ui(ui, |ui| {
                        for srv in &servers_list {
                            let is_sel = sel_id.as_deref() == Some(srv.id.as_str());
                            let lbl = format!(
                                "{} ({} • MC {} • :{})",
                                srv.name,
                                srv.engine.display_name(),
                                srv.game_version,
                                srv.port
                            );
                            if ui.selectable_label(is_sel, lbl).clicked() {
                                picked_id = Some(srv.id.clone());
                            }
                        }
                    });
                ui.ctx()
                    .accesskit_node_builder(combo_resp.response.id, |node| {
                        node.set_label(a11y_srv_lbl);
                    });
                if let Some(sid) = picked_id {
                    app.click_sfx();
                    app.select_server(sid);
                }

                if primary_icon_button(ui, VectorIcon::Plus, app.i18n.tr("Tạo", "New")).clicked()
                {
                    app.click_sfx();
                    let eng = app.servers.new_draft.engine;
                    app.ensure_engine_versions(eng);
                    app.show_new_server_modal = true;
                }
            });
        },
    );
}

fn maybe_vertical_scroll(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    enabled: bool,
    auto_shrink_y: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let w = ui.available_width();
    if enabled {
        ScrollArea::vertical()
            .id_salt(id_salt)
            .auto_shrink([false, auto_shrink_y])
            .show(ui, |ui| {
                ui.set_max_width(w);
                add_contents(ui);
            });
    } else {
        ui.set_max_width(w);
        add_contents(ui);
    }
}

fn render_active_server_tab(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    srv: &ManagedServer,
    unified_scroll: bool,
    viewport_h: f32,
) {
    match app.servers.active_tab {
        ServerTab::Console => render_tab_console(app, ui, srv, unified_scroll, viewport_h, true),
        ServerTab::Files => render_tab_files(app, ui, srv, unified_scroll),
        ServerTab::PluginsMods => render_tab_plugins_mods(app, ui, srv, unified_scroll),
        ServerTab::Properties => render_tab_properties(app, ui, srv, unified_scroll),
        ServerTab::AccessControl => render_tab_access_control(app, ui, srv, unified_scroll),
        ServerTab::Network => render_tab_network(app, ui, srv, unified_scroll),
    }
}

fn tps_badge_info(stats: &ServerResourceStats) -> Option<(String, Color32)> {
    if let Some(tps) = stats.tps {
        let color = if tps >= 19.0 {
            ACCENT_EMERALD
        } else if tps >= 15.0 {
            ACCENT_AMBER
        } else {
            ACCENT_RED
        };
        let text = match stats.mspt {
            Some(mspt) => format!("TPS {tps:.1} ({mspt:.1}ms)"),
            None => format!("TPS {tps:.1}"),
        };
        Some((text, color))
    } else {
        stats.mspt.map(|mspt| {
            let color = if mspt <= 50.0 {
                ACCENT_EMERALD
            } else if mspt <= 75.0 {
                ACCENT_AMBER
            } else {
                ACCENT_RED
            };
            (format!("MSPT {mspt:.1}ms"), color)
        })
    }
}

fn ping_badge_color(ms: u32, is_public: bool) -> Color32 {
    let (good, warn) = if is_public { (90, 200) } else { (50, 150) };
    if ms <= good {
        ACCENT_EMERALD
    } else if ms <= warn {
        ACCENT_AMBER
    } else {
        ACCENT_RED
    }
}

fn runtime_state_badge_info(state: ServerRuntimeState, i18n: &I18n) -> (&'static str, Color32) {
    match state {
        ServerRuntimeState::Offline => ("Offline", Color32::GRAY),
        ServerRuntimeState::Starting => (i18n.tr("Đang khởi động", "Starting"), ACCENT_AMBER),
        ServerRuntimeState::Online => (i18n.tr("Trực tuyến", "Online"), ACCENT_EMERALD),
        ServerRuntimeState::Stopping => (i18n.tr("Đang dừng", "Stopping"), ACCENT_RED),
    }
}

fn format_used_cores(used_cores: f32) -> String {
    let clamped = used_cores.max(0.0);
    if clamped > 0.0 && clamped < 0.1 {
        format!("{clamped:.2}")
    } else {
        format!("{clamped:.1}")
    }
}

fn format_cpu_usage_label(stats: &ServerResourceStats, i18n: &I18n) -> String {
    if stats.logical_cores > 0 {
        let cores_word = i18n.tr("lõi", "cores");
        format!(
            "CPU {:.1}% ({}/{} {cores_word})",
            stats.cpu_percent,
            format_used_cores(stats.used_cores),
            stats.logical_cores
        )
    } else {
        format!("CPU {:.1}%", stats.cpu_percent)
    }
}

fn format_cpu_tooltip(
    stats: &ServerResourceStats,
    full_model_fallback: Option<&str>,
    i18n: &I18n,
) -> Option<String> {
    if stats.cpu_model.is_empty() && stats.logical_cores == 0 {
        return None;
    }
    let model = full_model_fallback
        .filter(|s| !s.is_empty())
        .unwrap_or(stats.cpu_model.as_str());
    let mut lines = Vec::new();
    if !model.is_empty() {
        lines.push(format!("CPU: {model}"));
    }
    if stats.logical_cores > 0 {
        lines.push(format!(
            "{}: {:.2} / {} {}",
            i18n.tr("Số lõi đang dùng", "Active cores used"),
            stats.used_cores.max(0.0),
            stats.logical_cores,
            i18n.tr("lõi", "cores")
        ));
        if stats.allowed_cores > 0 && stats.allowed_cores < stats.logical_cores {
            lines.push(format!(
                "{}: {}/{} {}",
                i18n.tr("Lõi được hệ điều hành cấp", "OS-allowed cores"),
                stats.allowed_cores,
                stats.logical_cores,
                i18n.tr("lõi", "cores")
            ));
        }
    }
    Some(lines.join("\n"))
}

fn server_cpu_details_wrapped(
    ui: &mut egui::Ui,
    host_cpu: &CpuHardwareInfo,
    rt_state: ServerRuntimeState,
    live_stats: Option<&ServerResourceStats>,
    i18n: &I18n,
) {
    let cap_w = ui.max_rect().width().max(48.0);
    ui.horizontal_wrapped(|ui| {
        bounded_wrap_label(ui, RichText::new(&host_cpu.model_name).strong(), cap_w);
        let topo_label = match host_cpu
            .physical_cores
            .filter(|&p| p != host_cpu.logical_cores)
        {
            Some(phys) => format!(
                "{phys} {} / {} {}",
                i18n.tr("nhân", "cores"),
                host_cpu.logical_cores,
                i18n.tr("luồng", "threads")
            ),
            None => format!("{} {}", host_cpu.logical_cores, i18n.tr("lõi", "cores")),
        };
        badge(ui, &topo_label, ACCENT_CYAN);

        if rt_state != ServerRuntimeState::Offline {
            let mut live = live_stats.cloned().unwrap_or_default();
            if live.logical_cores == 0 {
                live.logical_cores = host_cpu.logical_cores;
            }
            badge(ui, &format_cpu_usage_label(&live, i18n), ACCENT_EMERALD);
            if live.allowed_cores > 0 && live.allowed_cores < host_cpu.logical_cores {
                badge(
                    ui,
                    &format!(
                        "{}: {}/{}",
                        i18n.tr("Lõi cấp phép", "Allowed"),
                        live.allowed_cores,
                        host_cpu.logical_cores
                    ),
                    ACCENT_AMBER,
                );
            }
        }
    });
}

#[cfg(all(test, target_os = "windows"))]
fn server_system_cpu_row_ui(
    ui: &mut egui::Ui,
    host_cpu: &CpuHardwareInfo,
    rt_state: ServerRuntimeState,
    live_stats: Option<&ServerResourceStats>,
    i18n: &I18n,
) {
    ui.label(i18n.tr("CPU hệ thống:", "System CPU:"));
    server_cpu_details_wrapped(ui, host_cpu, rt_state, live_stats, i18n);
}

fn render_server_header(app: &mut LauncherAppState, ui: &mut egui::Ui, srv: &ManagedServer) {
    let rt_state = app
        .servers
        .server_states
        .get(&srv.id)
        .copied()
        .unwrap_or_else(|| app.server_supervisor.runtime_state(&srv.id));
    let mut stats = app
        .servers
        .server_stats
        .get(&srv.id)
        .cloned()
        .unwrap_or_default();
    let host_cpu = CpuHardwareInfo::host();
    if stats.logical_cores == 0 {
        stats.logical_cores = host_cpu.logical_cores;
    }
    if stats.allowed_cores == 0 {
        stats.allowed_cores = host_cpu.logical_cores;
    }
    if stats.cpu_model.is_empty() {
        stats.cpu_model = host_cpu.short_model.clone();
    }

    match server_header_ui(ui, srv, rt_state, &stats, &app.i18n) {
        Some(ServerHeaderAction::Delete) => app.delete_selected_server(),
        Some(ServerHeaderAction::Folder) => {
            let _ = std::fs::create_dir_all(&srv.server_dir);
            app.open_local_folder_or_toast(&srv.server_dir);
        }
        Some(ServerHeaderAction::Install) => app.install_selected_server_jar(),
        Some(ServerHeaderAction::Start) => app.start_selected_server(),
        Some(ServerHeaderAction::Stop) => app.stop_selected_server(),
        Some(ServerHeaderAction::Restart) => app.restart_selected_server(),
        None => {}
    }
}

#[derive(Debug, PartialEq)]
enum ServerHeaderAction {
    Delete,
    Folder,
    Install,
    Start,
    Stop,
    Restart,
}

fn server_header_ui(
    ui: &mut egui::Ui,
    srv: &ManagedServer,
    rt_state: ServerRuntimeState,
    stats: &ServerResourceStats,
    i18n: &I18n,
) -> Option<ServerHeaderAction> {
    let (st_label, st_color) = runtime_state_badge_info(rt_state, i18n);
    let mut action = None;

    let header_accent = if rt_state == ServerRuntimeState::Online {
        ACCENT_EMERALD
    } else {
        ACCENT_CYAN
    };

    gaming_card_3d(
        ui,
        ("srv_header_card", &srv.id),
        rt_state == ServerRuntimeState::Online,
        header_accent,
        Margin::symmetric(16, 12),
        |ui| {
            ui.set_width(ui.available_width());
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.horizontal(|ui| {
                paint_icon_3d(
                    ui,
                    40.0,
                    Icon3dKind::ServerBladeTower,
                    rt_state == ServerRuntimeState::Online,
                );
                ui.add_space(4.0);

                let right_w = ui.available_width().max(48.0);
                ui.vertical(|ui| {
                    ui.set_max_width(right_w);
                    bounded_wrap_label(ui, RichText::new(&srv.name).size(20.0).strong(), right_w);
                    ui.horizontal_wrapped(|ui| {
                        badge(ui, st_label, st_color);
                        badge(ui, srv.engine.display_name(), ACCENT_BLUE);
                        badge(ui, &format!("MC {}", srv.game_version), ACCENT_EMERALD);
                        badge(ui, &format!("Port {}", srv.port), ACCENT_AMBER);
                    });

                    ui.horizontal_wrapped(|ui| {
                        let jar_info = if srv.jar_file.is_empty() {
                            i18n.tr("Chưa tải lõi JAR", "JAR not installed")
                        } else {
                            srv.jar_file.as_str()
                        };
                        bounded_wrap_label(
                            ui,
                            RichText::new(format!("{}: {jar_info}", i18n.tr("Lõi", "Core")))
                                .size(12.0)
                                .weak(),
                            (right_w - 4.0).max(40.0),
                        );
                        if rt_state == ServerRuntimeState::Offline {
                            if !stats.cpu_model.is_empty() || stats.logical_cores > 0 {
                                ui.separator();
                                if !stats.cpu_model.is_empty() {
                                    let short_cpu = crate::views::modpacks::truncate_chars(
                                        &stats.cpu_model,
                                        26,
                                    );
                                    let lbl = bounded_wrap_label(
                                        ui,
                                        RichText::new(format!("CPU: {short_cpu}"))
                                            .size(12.0)
                                            .weak(),
                                        (right_w - 4.0).max(40.0),
                                    );
                                    if stats.cpu_model.chars().count() > 26 {
                                        lbl.on_hover_text(&stats.cpu_model);
                                    }
                                }
                                if stats.logical_cores > 0 {
                                    badge(
                                        ui,
                                        &format!(
                                            "{} {}",
                                            stats.logical_cores,
                                            i18n.tr("lõi", "cores")
                                        ),
                                        ACCENT_CYAN,
                                    );
                                }
                            }
                        } else {
                            ui.separator();
                            if !stats.cpu_model.is_empty() {
                                let short_cpu =
                                    crate::views::modpacks::truncate_chars(&stats.cpu_model, 24);
                                badge(ui, &short_cpu, ACCENT_CYAN);
                            }
                            let cpu_frac = (stats.cpu_percent / 100.0).clamp(0.0, 1.0);
                            let ram_frac = (stats.ram_mb as f32 / srv.ram_max_mb.max(1) as f32)
                                .clamp(0.0, 1.0);
                            let cpu_resp = bounded_wrap_label(
                                ui,
                                RichText::new(format_cpu_usage_label(stats, i18n))
                                    .size(11.2)
                                    .strong(),
                                (right_w - 4.0).max(40.0),
                            );
                            if let Some(tip) = format_cpu_tooltip(stats, None, i18n) {
                                cpu_resp.on_hover_text(tip);
                            }
                            paint_glowing_progress_bar(ui, cpu_frac, 68.0_f32.min(right_w), 14.0);
                            bounded_wrap_label(
                                ui,
                                RichText::new(format!("RAM {}/{}MB", stats.ram_mb, srv.ram_max_mb))
                                    .size(11.2)
                                    .strong(),
                                (right_w - 4.0).max(40.0),
                            );
                            paint_glowing_progress_bar(ui, ram_frac, 76.0_f32.min(right_w), 14.0);
                            badge(
                                ui,
                                &format!(
                                    "{}/{} {} • {}",
                                    stats.online_players,
                                    stats.max_players.max(20),
                                    i18n.tr("người chơi", "players"),
                                    format_duration_hms(stats.uptime_secs)
                                ),
                                ACCENT_EMERALD,
                            );
                            if let Some((tps_lbl, tps_col)) = tps_badge_info(stats) {
                                badge(ui, &tps_lbl, tps_col);
                            }
                            if let Some(ping_ms) = stats.local_ping_ms {
                                badge(
                                    ui,
                                    &format!("Ping {ping_ms}ms"),
                                    ping_badge_color(ping_ms, false),
                                );
                            }
                            if let Some(pub_ms) = stats.public_ping_ms {
                                badge(
                                    ui,
                                    &format!("Internet {pub_ms}ms"),
                                    ping_badge_color(pub_ms, true),
                                );
                            }
                        }
                    });
                });
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if rt_state == ServerRuntimeState::Offline
                    && danger_icon_button(ui, VectorIcon::Trash, i18n.tr("Xóa", "Delete")).clicked()
                {
                    action = Some(ServerHeaderAction::Delete);
                }

                if secondary_icon_button(ui, VectorIcon::Folder, i18n.tr("Thư mục", "Folder"))
                    .clicked()
                {
                    action = Some(ServerHeaderAction::Folder);
                }

                if rt_state == ServerRuntimeState::Offline
                    && secondary_icon_button(
                        ui,
                        VectorIcon::Download,
                        i18n.tr("Tải / Cài lại Lõi", "Install / Repair JAR"),
                    )
                    .clicked()
                {
                    action = Some(ServerHeaderAction::Install);
                }

                match rt_state {
                    ServerRuntimeState::Offline => {
                        if primary_icon_button(ui, VectorIcon::Play, i18n.t("server.start"))
                            .clicked()
                        {
                            action = Some(ServerHeaderAction::Start);
                        }
                    }
                    ServerRuntimeState::Online => {
                        if danger_icon_button(ui, VectorIcon::Stop, i18n.t("server.stop")).clicked()
                        {
                            action = Some(ServerHeaderAction::Stop);
                        }
                        if secondary_icon_button(ui, VectorIcon::Refresh, i18n.t("server.restart"))
                            .clicked()
                        {
                            action = Some(ServerHeaderAction::Restart);
                        }
                    }
                    ServerRuntimeState::Starting => {
                        if danger_icon_button(
                            ui,
                            VectorIcon::Stop,
                            i18n.tr("Buộc dừng", "Force Stop"),
                        )
                        .clicked()
                        {
                            action = Some(ServerHeaderAction::Stop);
                        }
                        if secondary_icon_button(ui, VectorIcon::Refresh, i18n.t("server.restart"))
                            .clicked()
                        {
                            action = Some(ServerHeaderAction::Restart);
                        }
                    }
                    ServerRuntimeState::Stopping => {
                        if danger_icon_button(
                            ui,
                            VectorIcon::Stop,
                            i18n.tr("Buộc dừng", "Force Stop"),
                        )
                        .clicked()
                        {
                            action = Some(ServerHeaderAction::Stop);
                        }
                    }
                }
            });
        },
    );
    action
}

fn render_server_tabs_bar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let tabs = [
        (
            ServerTab::Console,
            VectorIcon::Terminal,
            app.i18n.t("server.console").to_string(),
        ),
        (
            ServerTab::Files,
            VectorIcon::Folder,
            app.i18n.t("server.files").to_string(),
        ),
        (
            ServerTab::PluginsMods,
            VectorIcon::Wrench,
            app.i18n.t("server.plugins").to_string(),
        ),
        (
            ServerTab::Properties,
            VectorIcon::Sliders,
            app.i18n.t("server.properties").to_string(),
        ),
        (
            ServerTab::AccessControl,
            VectorIcon::Shield,
            app.i18n
                .tr("Người chơi & Quyền", "Players & Access")
                .to_string(),
        ),
        (
            ServerTab::Network,
            VectorIcon::Globe,
            app.i18n.t("server.network").to_string(),
        ),
    ];

    if let Some(next_tab) =
        sliding_pill_tabs(ui, "server_tabs_pill_bar", &tabs, app.servers.active_tab)
    {
        app.click_sfx();
        app.servers.active_tab = next_tab;
        app.refresh_selected_server_tabs();
        if next_tab == ServerTab::Network {
            app.refresh_connected_network_snapshot();
        }
    }
}

#[cfg(not(target_os = "android"))]
pub(super) fn render_detached_server_console_panel(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let Some(srv) = app.selected_server().cloned() else {
        render_compact_server_bar(app, ui);
        ui.add_space(8.0);
        ui.label(
            RichText::new(app.i18n.tr(
                "Chưa có máy chủ cục bộ nào để hiển thị bảng điều khiển.",
                "No local server available.",
            ))
            .weak(),
        );
        return;
    };

    let footer_id = ui.scope_id().with("thk_detached_srv_cmd_footer");
    egui::Panel::bottom(footer_id)
        .resizable(false)
        .frame(Frame::NONE.inner_margin(Margin {
            left: 0,
            right: 0,
            top: 6,
            bottom: 0,
        }))
        .show(ui, |ui| {
            render_server_command_controls(app, ui, &srv.id);
        });

    render_compact_server_bar(app, ui);
    ui.add_space(8.0);
    render_server_header(app, ui, &srv);
    ui.add_space(8.0);
    let viewport_h = ui.available_height();
    render_tab_console(app, ui, &srv, false, viewport_h, false);
}

fn render_server_command_controls(app: &mut LauncherAppState, ui: &mut egui::Ui, srv_id: &str) {
    let input_id = if ui.ctx().viewport_id() == egui::ViewportId::ROOT {
        egui::Id::unique("server_console_cmd_input")
    } else {
        ui.make_persistent_id("server_console_cmd_input")
    };
    let is_focused = ui.memory(|m| m.has_focus(input_id));

    if is_focused {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
            let trimmed = app.servers.console_input.trim_start_matches('/');
            if let Some(matched) = MC_SERVER_AUTOCOMPLETE_CMDS
                .iter()
                .find(|cand| cand.starts_with(trimmed) && **cand != trimmed)
            {
                app.servers.console_input = (*matched).to_string();
            }
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
            app.navigate_server_cmd_history(true);
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
            app.navigate_server_cmd_history(false);
        }
    }

    let (send, clear) = server_command_ui(ui, &mut app.servers.console_input, &app.i18n);
    if send {
        app.send_selected_server_command();
    }
    if clear {
        app.servers.server_logs.remove(srv_id);
    }
}

fn render_tab_console(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    srv: &ManagedServer,
    unified_scroll: bool,
    viewport_h: f32,
    inline_command_bar: bool,
) {
    let lines = app
        .servers
        .server_logs
        .get(&srv.id)
        .cloned()
        .unwrap_or_default();

    #[cfg(not(target_os = "android"))]
    if ui.ctx().viewport_id() == egui::ViewportId::ROOT {
        ui.horizontal_wrapped(|ui| {
            if secondary_icon_button(
                ui,
                VectorIcon::MultiWindow,
                app.i18n.tr("Tách cửa sổ Console", "Pop-out Server Console"),
            )
            .clicked()
            {
                app.click_sfx();
                app.detached.server_console_window_open = true;
            }
        });
        ui.add_space(4.0);
    }

    let dark = ui.visuals().dark_mode;
    let console_bg = if dark {
        Color32::from_rgb(18, 26, 40)
    } else {
        Color32::from_rgb(225, 236, 248)
    };
    let console_stroke = if dark {
        Stroke::new(1.0, Color32::from_rgb(56, 78, 110))
    } else {
        Stroke::new(1.2, BORDER_GAME_OUTER_LIGHT)
    };
    let default_log_fg = if dark {
        Color32::from_rgb(224, 234, 246)
    } else {
        TEXT_INK_LIGHT
    };

    Frame::group(ui.style())
        .fill(console_bg)
        .stroke(console_stroke)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let console_h = if unified_scroll {
                (viewport_h - 110.0).clamp(200.0, 620.0)
            } else if inline_command_bar {
                (ui.available_height() - 56.0).max(96.0)
            } else {
                ui.available_height().max(72.0)
            };
            let console_scroll_w = ui.available_width();
            let row_h = ui.text_style_height(&egui::TextStyle::Monospace).max(14.0);
            ScrollArea::vertical()
                .id_salt("server_console_scroll")
                .max_height(console_h)
                .min_scrolled_height(console_h)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show_rows(ui, row_h, lines.len().max(1), |ui, range| {
                    ui.set_max_width(console_scroll_w);
                    if lines.is_empty() {
                        ui.label(
                            RichText::new(app.i18n.tr(
                                "Máy chủ chưa có nhật ký hoạt động.",
                                "No server console output yet.",
                            ))
                            .monospace()
                            .weak(),
                        );
                        return;
                    }
                    for idx in range {
                        if let Some(line) = lines.get(idx) {
                            let col = if line.contains("ERROR") || line.contains("SEVERE") {
                                readable_accent_ink(ACCENT_RED, dark)
                            } else if line.contains("WARN") {
                                readable_accent_ink(ACCENT_AMBER, dark)
                            } else if line.starts_with("> ") {
                                readable_accent_ink(ACCENT_EMERALD, dark)
                            } else {
                                default_log_fg
                            };
                            ui.add(
                                egui::Label::new(
                                    RichText::new(line).monospace().size(12.0).color(col),
                                )
                                .truncate(),
                            );
                        }
                    }
                });
        });

    if inline_command_bar {
        ui.add_space(6.0);
        render_server_command_controls(app, ui, &srv.id);
    }
}

fn server_command_ui(ui: &mut egui::Ui, input: &mut String, i18n: &I18n) -> (bool, bool) {
    let input_id = if ui.ctx().viewport_id() == egui::ViewportId::ROOT {
        egui::Id::unique("server_console_cmd_input")
    } else {
        ui.make_persistent_id("server_console_cmd_input")
    };
    let resp = ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        egui::TextEdit::singleline(input)
            .id(input_id)
            .hint_text(i18n.tr(
                "Nhập lệnh máy chủ (↑/↓ lịch sử, Tab gợi ý lệnh, Enter gửi)...",
                "Enter server command (↑/↓ history, Tab autocomplete, Enter send)...",
            ))
            .desired_width(f32::INFINITY),
    );

    let enter_send = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let mut send = enter_send;
    let mut clear = false;
    ui.horizontal_wrapped(|ui| {
        send |= primary_icon_button(ui, VectorIcon::Play, i18n.tr("Gửi lệnh", "Send")).clicked();

        clear = secondary_icon_button(ui, VectorIcon::Trash, i18n.tr("Xoá màn hình", "Clear"))
            .clicked();
    });
    if send {
        resp.request_focus();
    }
    (send, clear)
}

fn render_tab_files(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    srv: &ManagedServer,
    unified_scroll: bool,
) {
    ui.horizontal_wrapped(|ui| {
        if secondary_icon_button(ui, VectorIcon::Folder, app.i18n.tr("Gốc (/)", "Root (/)"))
            .clicked()
        {
            app.servers.current_rel_dir.clear();
            app.servers.selected_entries.clear();
            app.refresh_selected_server_tabs();
        }
        if !app.servers.current_rel_dir.is_empty()
            && secondary_icon_button(ui, VectorIcon::ArrowLeft, app.i18n.tr("Lên cấp trên", "Up"))
                .clicked()
        {
            let parent = app
                .servers
                .current_rel_dir
                .rsplit_once('/')
                .map(|(p, _)| p.to_string())
                .unwrap_or_default();
            app.servers.current_rel_dir = parent;
            app.servers.selected_entries.clear();
            app.refresh_selected_server_tabs();
        }

        let display_rel = if app.servers.current_rel_dir.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", app.servers.current_rel_dir)
        };
        badge(ui, &display_rel, ACCENT_BLUE);

        if secondary_icon_button(ui, VectorIcon::Refresh, app.i18n.tr("Làm mới", "Refresh"))
            .clicked()
        {
            app.refresh_selected_server_tabs();
        }
        if primary_icon_button(
            ui,
            VectorIcon::Upload,
            app.i18n.tr("Tải tệp lên", "Upload File"),
        )
        .clicked()
        {
            app.import_external_file_into_server();
        }
        if secondary_icon_button(ui, VectorIcon::Folder, app.i18n.t("home.open_folder")).clicked() {
            let target = if app.servers.current_rel_dir.is_empty() {
                srv.server_dir.clone()
            } else {
                thk_core::SafePath::resolve_child(&srv.server_dir, &app.servers.current_rel_dir)
                    .unwrap_or_else(|_| srv.server_dir.clone())
            };
            let _ = std::fs::create_dir_all(&target);
            app.open_local_folder_or_toast(&target);
        }

        ui.separator();
        let arch_w = 120.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
        ui.add(
            egui::TextEdit::singleline(&mut app.servers.archive_name_input)
                .hint_text("backup.zip")
                .desired_width(arch_w),
        );
        if secondary_icon_button(
            ui,
            VectorIcon::Cube,
            app.i18n.tr("Nén mục chọn", "Compress .zip"),
        )
        .clicked()
        {
            app.compress_selected_server_files();
        }
        if !app.servers.selected_entries.is_empty() {
            let del_lbl = format!(
                "{} ({})",
                app.i18n.tr("Xóa", "Delete"),
                app.servers.selected_entries.len()
            );
            if danger_icon_button(ui, VectorIcon::Trash, del_lbl).clicked() {
                app.delete_selected_server_files();
            }
        }
    });

    ui.add_space(8.0);

    if let Some(editing_rel) = app.servers.editing_file_rel.clone() {
        gaming_card_3d(
            ui,
            "srv_inline_editor_card",
            true,
            ACCENT_EMERALD,
            Margin::same(12),
            |ui| {
                ui.set_width(ui.available_width());
                let editor_w = ui.available_width();
                ui.horizontal_wrapped(|ui| {
                    bounded_wrap_label(
                        ui,
                        RichText::new(format!(
                            "{}: {editing_rel}",
                            app.i18n.tr("Đang chỉnh sửa", "Editing")
                        ))
                        .strong()
                        .color(ACCENT_EMERALD),
                        editor_w,
                    );
                    if primary_icon_button(
                        ui,
                        VectorIcon::Save,
                        app.i18n.tr("Lưu tệp", "Save File"),
                    )
                    .clicked()
                    {
                        app.save_server_file_editor();
                    }
                    if secondary_button(ui, app.i18n.tr("Đóng trình sửa", "Close")).clicked() {
                        app.servers.editing_file_rel = None;
                    }
                });
                ui.add_space(6.0);
                let editor_scroll_w = ui.available_width();
                ScrollArea::vertical()
                    .id_salt("server_file_editor_scroll")
                    .max_height(240.0)
                    .show(ui, |ui| {
                        ui.set_max_width(editor_scroll_w);
                        ui.add(
                            egui::TextEdit::multiline(&mut app.servers.editing_file_text)
                                .code_editor()
                                .desired_rows(12)
                                .desired_width(f32::INFINITY),
                        );
                    });
            },
        );
        ui.add_space(8.0);
    }

    let entries = app.servers.dir_entries.clone();
    maybe_vertical_scroll(
        ui,
        "server_files_list_scroll",
        !unified_scroll,
        true,
        |ui| {
            if entries.is_empty() {
                ui.label(
                    RichText::new(
                        app.i18n
                            .tr("Thư mục này hiện đang trống.", "This directory is empty."),
                    )
                    .weak(),
                );
                return;
            }

            for entry in entries {
                let checked_init = app.servers.selected_entries.contains(&entry.rel_path);
                gaming_card_3d(
                    ui,
                    ("srv_file_entry", &entry.rel_path),
                    checked_init,
                    if entry.is_dir {
                        ACCENT_CYAN
                    } else {
                        ACCENT_BLUE
                    },
                    Margin::symmetric(10, 6),
                    |ui| {
                        let row_w = ui.available_width();
                        ui.set_width(row_w);
                        if row_w < 460.0 {
                            ui.horizontal_wrapped(|ui| {
                                let mut checked = checked_init;
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked {
                                        app.servers.selected_entries.insert(entry.rel_path.clone());
                                    } else {
                                        app.servers.selected_entries.remove(&entry.rel_path);
                                    }
                                }
                                if entry.is_dir {
                                    badge(ui, "DIR", ACCENT_CYAN);
                                    let resp = bounded_wrap_label(
                                        ui,
                                        RichText::new(&entry.name)
                                            .strong()
                                            .underline()
                                            .color(ACCENT_CYAN),
                                        (row_w - 72.0).max(48.0),
                                    );
                                    if resp.interact(Sense::click()).clicked() {
                                        app.servers.current_rel_dir = entry.rel_path.clone();
                                        app.servers.selected_entries.clear();
                                        app.refresh_selected_server_tabs();
                                    }
                                } else {
                                    bounded_wrap_label(
                                        ui,
                                        RichText::new(&entry.name),
                                        (row_w - 36.0).max(48.0),
                                    );
                                    ui.label(
                                        RichText::new(format_bytes(entry.size_bytes))
                                            .size(11.5)
                                            .weak(),
                                    );
                                    let low = entry.name.to_ascii_lowercase();
                                    if is_editable_text_filename(&low)
                                        && secondary_icon_button(
                                            ui,
                                            VectorIcon::Wrench,
                                            app.i18n.tr("Sửa", "Edit"),
                                        )
                                        .clicked()
                                    {
                                        app.open_server_file_editor(&entry.rel_path);
                                    }
                                    if low.ends_with(".zip")
                                        && secondary_icon_button(
                                            ui,
                                            VectorIcon::Folder,
                                            app.i18n.tr("Giải nén", "Extract"),
                                        )
                                        .clicked()
                                    {
                                        app.extract_server_archive(&entry.rel_path);
                                    }
                                }
                            });
                        } else {
                            ui.horizontal(|ui| {
                                let mut checked = checked_init;
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked {
                                        app.servers.selected_entries.insert(entry.rel_path.clone());
                                    } else {
                                        app.servers.selected_entries.remove(&entry.rel_path);
                                    }
                                }

                                let name_cap = (row_w - 230.0).max(80.0);
                                if entry.is_dir {
                                    badge(ui, "DIR", ACCENT_CYAN);
                                    if ui.link(RichText::new(&entry.name).strong()).clicked() {
                                        app.servers.current_rel_dir = entry.rel_path.clone();
                                        app.servers.selected_entries.clear();
                                        app.refresh_selected_server_tabs();
                                    }
                                } else {
                                    bounded_wrap_label(ui, RichText::new(&entry.name), name_cap);
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !entry.is_dir {
                                            let low = entry.name.to_ascii_lowercase();
                                            if low.ends_with(".zip")
                                                && secondary_icon_button(
                                                    ui,
                                                    VectorIcon::Folder,
                                                    app.i18n.tr("Giải nén", "Extract"),
                                                )
                                                .clicked()
                                            {
                                                app.extract_server_archive(&entry.rel_path);
                                            }
                                            if is_editable_text_filename(&low)
                                                && secondary_icon_button(
                                                    ui,
                                                    VectorIcon::Wrench,
                                                    app.i18n.tr("Sửa", "Edit"),
                                                )
                                                .clicked()
                                            {
                                                app.open_server_file_editor(&entry.rel_path);
                                            }
                                            ui.label(
                                                RichText::new(format_bytes(entry.size_bytes))
                                                    .size(11.5)
                                                    .weak(),
                                            );
                                        }
                                    },
                                );
                            });
                        }
                    },
                );
                ui.add_space(4.0);
            }
        },
    );
}

fn is_editable_text_filename(lower_name: &str) -> bool {
    [
        ".properties",
        ".yml",
        ".yaml",
        ".json",
        ".toml",
        ".txt",
        ".conf",
        ".cfg",
        ".ini",
        ".log",
        ".sh",
        ".bat",
        ".cmd",
        ".mcmeta",
    ]
    .iter()
    .any(|ext| lower_name.ends_with(ext))
}

fn render_tab_plugins_mods(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    srv: &ManagedServer,
    unified_scroll: bool,
) {
    if !srv.engine.supports_plugins() && !srv.engine.supports_mods() {
        gaming_card_3d(
            ui,
            "srv_vanilla_no_addons",
            false,
            ACCENT_AMBER,
            Margin::same(16),
            |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(app.i18n.tr(
                        "Lõi Vanilla nguyên bản không nạp Plugin hoặc Mod (.jar). Hãy dùng Paper/Purpur/Folia cho Plugin hoặc Fabric/Forge/NeoForge cho Mod.",
                        "Vanilla servers do not load plugins or mods (.jar). Use Paper/Purpur/Folia for plugins or Fabric/Forge/NeoForge for mods.",
                    ))
                    .color(ACCENT_AMBER),
                );
            },
        );
        return;
    }

    gaming_card_3d(
        ui,
        "srv_installed_addons_card",
        false,
        ACCENT_CYAN,
        Margin::same(12),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(app.i18n.tr(
                        "Plugin / Mod đã cài trên máy chủ",
                        "Installed Server Plugins & Mods",
                    ))
                    .size(14.2)
                    .strong(),
                );
                badge(
                    ui,
                    &app.servers.installed_addons.len().to_string(),
                    ACCENT_CYAN,
                );
                let addon_flt_w = 180.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                ui.add(
                    egui::TextEdit::singleline(&mut app.servers.addon_filter)
                        .hint_text(app.i18n.tr("Lọc gói đã cài...", "Filter installed..."))
                        .desired_width(addon_flt_w),
                );
                if secondary_icon_button(ui, VectorIcon::Refresh, app.i18n.tr("Làm mới", "Refresh"))
                    .clicked()
                {
                    app.refresh_selected_server_tabs();
                }
                if srv.engine.supports_plugins()
                    && secondary_icon_button(
                        ui,
                        VectorIcon::Upload,
                        app.i18n.tr("+ .jar vào plugins/", "+ .jar to plugins/"),
                    )
                    .clicked()
                {
                    app.import_jar_into_server_addons("plugins");
                }
                if srv.engine.supports_mods()
                    && secondary_icon_button(
                        ui,
                        VectorIcon::Upload,
                        app.i18n.tr("+ .jar vào mods/", "+ .jar to mods/"),
                    )
                    .clicked()
                {
                    app.import_jar_into_server_addons("mods");
                }
            });

            ui.add_space(6.0);
            let filter_raw = app.servers.addon_filter.trim();
            let has_filter = !filter_raw.is_empty();
            let filter_low = if has_filter {
                filter_raw.to_lowercase()
            } else {
                String::new()
            };
            let filtered_indices: Vec<usize> = if has_filter {
                app.servers
                    .installed_addons
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, addon)| {
                        if addon.title.to_lowercase().contains(&filter_low)
                            || addon.file_name.to_lowercase().contains(&filter_low)
                        {
                            Some(idx)
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let total_items = if has_filter {
                filtered_indices.len()
            } else {
                app.servers.installed_addons.len()
            };

            let mut toggle_req: Option<(String, String, bool)> = None;
            let mut delete_req: Option<(String, String)> = None;

            let inst_scroll_w = ui.available_width();
            let mut render_installed_list = |ui: &mut egui::Ui| {
                ui.set_max_width(inst_scroll_w);
                if app.servers.installed_addons.is_empty() {
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Chưa có tệp .jar nào trong thư mục plugins/ hoặc mods/.",
                            "No .jar files installed in plugins/ or mods/ yet.",
                        ))
                        .weak(),
                    );
                    return;
                }
                if total_items == 0 {
                    ui.label(
                        RichText::new(
                            app.i18n
                                .tr("Không tìm thấy tệp phù hợp.", "No matching files found."),
                        )
                        .weak(),
                    );
                    return;
                }

                let row_w = ui.available_width();
                let is_narrow = row_w < 500.0;
                let row_h =
                    (if is_narrow { 88.0 } else { 58.0 }) * app.settings.font_scale.max(1.0);
                let clip = ui.clip_rect();
                let list_top = ui.cursor().top();

                let (start_idx, end_idx) = if clip.is_finite() && clip.height() > 0.0 {
                    let rel_top = (clip.min.y - list_top).max(0.0);
                    let rel_bottom = (clip.max.y - list_top).max(0.0);
                    let first_visible = (rel_top / row_h).floor() as usize;
                    let last_visible = ((rel_bottom / row_h).ceil() as usize).min(total_items);
                    let start = first_visible.saturating_sub(2).min(total_items);
                    let end = (last_visible + 2).min(total_items);
                    (start, end.max(start))
                } else {
                    (0, total_items)
                };

                if start_idx > 0 {
                    ui.add_space(start_idx as f32 * row_h);
                }

                for idx in start_idx..end_idx {
                    let item_idx = filtered_indices.get(idx).copied().unwrap_or(idx);
                    let addon = &app.servers.installed_addons[item_idx];
                    let folder_col = if addon.folder == "plugins" {
                        ACCENT_VIOLET
                    } else {
                        ACCENT_CYAN
                    };
                    if is_narrow {
                        ui.horizontal_wrapped(|ui| {
                            let mut enabled = addon.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                toggle_req =
                                    Some((addon.folder.clone(), addon.file_name.clone(), enabled));
                            }
                            badge(ui, &format!("{}/", addon.folder), folder_col);
                            ui.label(RichText::new(&addon.title).strong());
                            if !addon.version_number.is_empty() {
                                badge(ui, &addon.version_number, ACCENT_BLUE);
                            }
                        });
                        bounded_wrap_label(
                            ui,
                            RichText::new(format!(
                                "{} ({})",
                                addon.file_name,
                                format_bytes(addon.size_bytes)
                            ))
                            .size(11.2)
                            .weak(),
                            row_w,
                        );
                        ui.horizontal(|ui| {
                            let status_lbl = if addon.enabled {
                                RichText::new(app.i18n.tr("Đã bật", "Enabled"))
                                    .size(11.2)
                                    .color(ACCENT_EMERALD)
                            } else {
                                RichText::new(app.i18n.tr("Đã tắt", "Disabled"))
                                    .size(11.2)
                                    .color(Color32::GRAY)
                            };
                            ui.label(status_lbl);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if danger_icon_button(
                                        ui,
                                        VectorIcon::Trash,
                                        app.i18n.tr("Xóa", "Delete"),
                                    )
                                    .clicked()
                                    {
                                        delete_req =
                                            Some((addon.folder.clone(), addon.file_name.clone()));
                                    }
                                },
                            );
                        });
                        ui.add_space(6.0);
                    } else {
                        let action_w = 88.0;
                        let content_max_w = (row_w - action_w - 12.0).max(100.0);
                        ui.horizontal(|ui| {
                            let mut enabled = addon.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                toggle_req =
                                    Some((addon.folder.clone(), addon.file_name.clone(), enabled));
                            }
                            badge(ui, &format!("{}/", addon.folder), folder_col);
                            ui.vertical(|ui| {
                                ui.set_max_width(content_max_w);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(&addon.title).strong());
                                    if !addon.version_number.is_empty() {
                                        badge(ui, &addon.version_number, ACCENT_BLUE);
                                    }
                                });
                                bounded_wrap_label(
                                    ui,
                                    RichText::new(format!(
                                        "{} ({})",
                                        addon.file_name,
                                        format_bytes(addon.size_bytes)
                                    ))
                                    .size(11.2)
                                    .weak(),
                                    content_max_w,
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if danger_icon_button(
                                        ui,
                                        VectorIcon::Trash,
                                        app.i18n.tr("Xóa", "Delete"),
                                    )
                                    .clicked()
                                    {
                                        delete_req =
                                            Some((addon.folder.clone(), addon.file_name.clone()));
                                    }
                                },
                            );
                        });
                        ui.add_space(4.0);
                    }
                }

                if end_idx < total_items {
                    ui.add_space((total_items - end_idx) as f32 * row_h);
                }
            };
            if unified_scroll {
                render_installed_list(ui);
            } else {
                ScrollArea::vertical()
                    .id_salt("srv_installed_addons_scroll")
                    .max_height(168.0)
                    .show(ui, |ui| render_installed_list(ui));
            }
            if let Some((folder, file_name, enabled)) = toggle_req {
                app.toggle_server_installed_addon(&folder, &file_name, enabled);
            }
            if let Some((folder, file_name)) = delete_req {
                app.delete_server_installed_addon(&folder, &file_name);
            }
        },
    );

    ui.add_space(8.0);

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(app.i18n.tr("Nguồn tải:", "Provider:")).strong());
        if ui
            .selectable_label(
                app.servers.addon_provider == ProviderSource::Modrinth,
                "Modrinth",
            )
            .clicked()
        {
            app.servers.addon_provider = ProviderSource::Modrinth;
            app.search_server_addons();
        }
        if srv.engine.supports_mods()
            && ui
                .selectable_label(
                    app.servers.addon_provider == ProviderSource::CurseForge,
                    "CurseForge",
                )
                .clicked()
        {
            app.servers.addon_provider = ProviderSource::CurseForge;
            app.servers.addon_kind = ContentKind::Mod;
            app.search_server_addons();
        }
        if srv.engine.supports_plugins()
            && ui
                .selectable_label(
                    app.servers.addon_provider == ProviderSource::Spiget,
                    "Spiget (SpigotMC)",
                )
                .clicked()
        {
            app.servers.addon_provider = ProviderSource::Spiget;
            app.servers.addon_kind = ContentKind::Plugin;
            app.search_server_addons();
        }

        if app.servers.addon_provider == ProviderSource::Modrinth {
            ui.separator();
            if srv.engine.supports_plugins()
                && ui
                    .selectable_label(
                        app.servers.addon_kind == ContentKind::Plugin,
                        "Plugin (plugins/)",
                    )
                    .clicked()
            {
                app.servers.addon_kind = ContentKind::Plugin;
                app.search_server_addons();
            }
            if srv.engine.supports_mods()
                && ui
                    .selectable_label(
                        app.servers.addon_kind == ContentKind::Mod,
                        "Server Mod (mods/)",
                    )
                    .clicked()
            {
                app.servers.addon_kind = ContentKind::Mod;
                app.search_server_addons();
            }
        }
    });

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        let addon_q_w = 340.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.servers.addon_query)
                .hint_text(app.i18n.tr(
                    "Tìm kiếm EssentialsX, LuckPerms, WorldEdit, Geyser, Lithium...",
                    "Search EssentialsX, LuckPerms, WorldEdit, Geyser, Lithium...",
                ))
                .desired_width(addon_q_w),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            app.search_server_addons();
        }
        if primary_icon_button(ui, VectorIcon::Search, app.i18n.tr("Tìm kiếm", "Search")).clicked()
        {
            app.search_server_addons();
        }
        if app.servers.addon_selected.is_some()
            && secondary_icon_button(
                ui,
                VectorIcon::ArrowLeft,
                app.i18n.tr("Quay lại kết quả", "Back to Results"),
            )
            .clicked()
        {
            app.servers.addon_selected = None;
        }
    });

    ui.add_space(8.0);

    if let Some((summary, versions)) = app.servers.addon_selected.clone() {
        let sel_tex = summary
            .icon_url
            .as_deref()
            .and_then(|u| app.request_modpack_texture(u, 64));

        gaming_card_3d(
            ui,
            "srv_addon_version_picker_card",
            true,
            ACCENT_VIOLET,
            Margin::same(12),
            |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    paint_addon_thumbnail(ui, sel_tex.as_ref(), 38.0, ACCENT_VIOLET);
                    let right_w = ui.available_width().max(48.0);
                    ui.allocate_ui_with_layout(
                        vec2(right_w, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.set_max_width(right_w);
                            bounded_wrap_label(
                                ui,
                                RichText::new(if app.settings.language == Language::Vi {
                                    format!(
                                        "Chọn phiên bản '{}' cho máy chủ MC {}:",
                                        summary.title, srv.game_version
                                    )
                                } else {
                                    format!(
                                        "Select '{}' build for MC {}:",
                                        summary.title, srv.game_version
                                    )
                                })
                                .strong(),
                                right_w,
                            );
                            bounded_wrap_label(
                                ui,
                                RichText::new(&summary.summary).size(11.5).weak(),
                                right_w,
                            );
                        },
                    );
                });
                ui.add_space(6.0);
                if versions.is_empty() {
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Không tìm thấy bản phát hành tương thích MC của máy chủ này.",
                            "No compatible server builds found for this MC version.",
                        ))
                        .color(ACCENT_RED),
                    );
                    return;
                }
                if app.servers.addon_chosen_ver_idx >= versions.len() {
                    app.servers.addon_chosen_ver_idx = 0;
                }
                let cur_ver = &versions[app.servers.addon_chosen_ver_idx];
                let combo_txt = crate::views::format_project_version_option(cur_ver, false);
                ui.horizontal_wrapped(|ui| {
                    let addon_ver_w = 340.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                    egui::ComboBox::from_id_salt("srv_addon_ver_combo")
                        .width(addon_ver_w)
                        .truncate()
                        .selected_text(combo_txt)
                        .show_ui(ui, |ui| {
                            for (idx, v) in versions.iter().enumerate() {
                                let lbl = crate::views::format_project_version_option(v, false);
                                ui.selectable_value(
                                    &mut app.servers.addon_chosen_ver_idx,
                                    idx,
                                    lbl,
                                );
                            }
                        });
                    if primary_icon_button(ui, VectorIcon::Download, app.i18n.t("mods.install"))
                        .clicked()
                    {
                        let chosen = versions[app.servers.addon_chosen_ver_idx].clone();
                        app.install_server_addon_version(summary, Some(chosen));
                    }
                });
            },
        );
        return;
    }

    let installed_names: HashSet<String> = app
        .servers
        .installed_addons
        .iter()
        .flat_map(|a| {
            [
                a.title.to_ascii_lowercase(),
                a.base_file_name.to_ascii_lowercase(),
            ]
        })
        .collect();

    let hits = app.servers.addon_hits.clone();
    maybe_vertical_scroll(
        ui,
        "server_addons_hits_scroll",
        !unified_scroll,
        true,
        |ui| {
            if hits.is_empty() {
                ui.label(
                    RichText::new(app.i18n.tr(
                        "Nhập từ khóa và nhấn 'Tìm kiếm' để tải Plugin/Mod trực tiếp vào máy chủ.",
                        "Enter a keyword and click 'Search' to install plugins or mods directly.",
                    ))
                    .weak(),
                );
                return;
            }

            for hit in &hits {
                let hit_id = hit.project_id.clone();
                let hit_tex = hit
                    .icon_url
                    .as_deref()
                    .and_then(|u| app.request_modpack_texture(u, 64));
                let title_low = hit.title.to_ascii_lowercase();
                let slug_low = hit.slug.to_ascii_lowercase();
                let is_installed = installed_names
                    .iter()
                    .any(|n| n == &title_low || (!slug_low.is_empty() && n.contains(&slug_low)));

                gaming_card_3d(
                    ui,
                    ("srv_addon_hit", hit_id),
                    is_installed,
                    ACCENT_EMERALD,
                    Margin::symmetric(12, 9),
                    |ui| {
                        let card_w = ui.available_width();
                        ui.set_width(card_w);
                        if card_w < 520.0 {
                            ui.horizontal(|ui| {
                                paint_addon_thumbnail(ui, hit_tex.as_ref(), 36.0, ACCENT_EMERALD);
                                let info_w = ui.available_width().max(48.0);
                                ui.allocate_ui_with_layout(
                                    vec2(info_w, 0.0),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.set_max_width(info_w);
                                        ui.horizontal_wrapped(|ui| {
                                            bounded_wrap_label(
                                                ui,
                                                RichText::new(&hit.title).size(14.5).strong(),
                                                info_w,
                                            );
                                            badge(ui, hit.provider.display_name(), ACCENT_BLUE);
                                            ui.label(
                                                RichText::new(format!("by {}", hit.author))
                                                    .size(11.5)
                                                    .weak(),
                                            );
                                            if is_installed {
                                                badge(
                                                    ui,
                                                    app.i18n.t("mods.installed"),
                                                    ACCENT_EMERALD,
                                                );
                                            }
                                        });
                                        bounded_wrap_label(
                                            ui,
                                            RichText::new(&hit.summary).size(12.0),
                                            info_w,
                                        );
                                    },
                                );
                            });
                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                if hit.provider != ProviderSource::Spiget
                                    && secondary_icon_button(
                                        ui,
                                        VectorIcon::Sliders,
                                        app.i18n.tr("Chọn bản", "Versions"),
                                    )
                                    .clicked()
                                {
                                    app.load_server_addon_versions(hit.clone());
                                }
                                if primary_icon_button(
                                    ui,
                                    VectorIcon::Download,
                                    app.i18n.t("mods.install"),
                                )
                                .clicked()
                                {
                                    app.install_server_addon_item(hit.clone());
                                }
                            });
                        } else {
                            ui.horizontal(|ui| {
                                paint_addon_thumbnail(ui, hit_tex.as_ref(), 36.0, ACCENT_EMERALD);
                                let info_w = (card_w - 252.0).max(120.0);
                                ui.allocate_ui_with_layout(
                                    vec2(info_w, 0.0),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.set_max_width(info_w);
                                        ui.horizontal_wrapped(|ui| {
                                            bounded_wrap_label(
                                                ui,
                                                RichText::new(&hit.title).size(14.5).strong(),
                                                info_w,
                                            );
                                            badge(ui, hit.provider.display_name(), ACCENT_BLUE);
                                            ui.label(
                                                RichText::new(format!("by {}", hit.author))
                                                    .size(11.5)
                                                    .weak(),
                                            );
                                            if is_installed {
                                                badge(
                                                    ui,
                                                    app.i18n.t("mods.installed"),
                                                    ACCENT_EMERALD,
                                                );
                                            }
                                        });
                                        bounded_wrap_label(
                                            ui,
                                            RichText::new(&hit.summary).size(12.0),
                                            info_w,
                                        );
                                    },
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if primary_icon_button(
                                            ui,
                                            VectorIcon::Download,
                                            app.i18n.t("mods.install"),
                                        )
                                        .clicked()
                                        {
                                            app.install_server_addon_item(hit.clone());
                                        }
                                        if hit.provider != ProviderSource::Spiget
                                            && secondary_icon_button(
                                                ui,
                                                VectorIcon::Sliders,
                                                app.i18n.tr("Chọn bản", "Versions"),
                                            )
                                            .clicked()
                                        {
                                            app.load_server_addon_versions(hit.clone());
                                        }
                                    },
                                );
                            });
                        }
                    },
                );
                ui.add_space(5.0);
            }

            if hits.len() < app.servers.addon_total_hits {
                ui.add_space(4.0);
                let more_lbl = if app.settings.language == Language::Vi {
                    format!("Tải thêm ({}/{})", hits.len(), app.servers.addon_total_hits)
                } else {
                    format!(
                        "Load More ({}/{})",
                        hits.len(),
                        app.servers.addon_total_hits
                    )
                };
                if secondary_icon_button(ui, VectorIcon::Download, more_lbl).clicked() {
                    let next_page = app.servers.addon_page + 1;
                    app.search_server_addons_page(next_page, true);
                }
            }
        },
    );
}

fn prop_checkbox(
    ui: &mut egui::Ui,
    props: &mut BTreeMap<String, String>,
    key: &str,
    default_val: bool,
    label: &str,
) {
    let current = props
        .get(key)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(default_val);
    let mut val = current;
    if crate::views::wrapped_checkbox(ui, &mut val, label).changed() {
        props.insert(key.to_string(), if val { "true" } else { "false" }.into());
    }
}

fn prop_combo(
    ui: &mut egui::Ui,
    props: &mut BTreeMap<String, String>,
    key: &str,
    default_val: &str,
    options: &[&str],
    label_id: egui::Id,
) {
    let current = props
        .get(key)
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_val.to_string());
    let combo_w = 135.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
    egui::ComboBox::from_id_salt(("srv_prop_combo", key))
        .width(combo_w)
        .truncate()
        .selected_text(&current)
        .show_ui(ui, |ui| {
            for &opt in options {
                if ui.selectable_label(current == opt, opt).clicked() {
                    props.insert(key.to_string(), opt.to_string());
                }
            }
        })
        .response
        .labelled_by(label_id);
}

fn prop_u32_drag(
    ui: &mut egui::Ui,
    props: &mut BTreeMap<String, String>,
    key: &str,
    default_val: u32,
    range: std::ops::RangeInclusive<u32>,
    label_id: egui::Id,
) {
    let mut val = props
        .get(key)
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(default_val);
    if ui
        .add(egui::DragValue::new(&mut val).range(range))
        .labelled_by(label_id)
        .changed()
    {
        props.insert(key.to_string(), val.to_string());
    }
}

fn render_tab_properties(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    srv: &ManagedServer,
    unified_scroll: bool,
) {
    if app.servers.edit_draft.loaded_for_id != srv.id {
        app.sync_server_edit_draft();
    }

    maybe_vertical_scroll(
        ui,
        "server_properties_outer_scroll",
        !unified_scroll,
        true,
        |ui| {
            gaming_card_3d(
                ui,
                "srv_hw_jvm_card",
                false,
                ACCENT_CYAN,
                Margin::same(14),
                |ui| {
                    let card_w = ui.available_width();
                    ui.set_width(card_w);
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Cấu hình Phần cứng, Cổng & Tham số JVM Máy chủ",
                            "Server Hardware, Port & JVM Arguments",
                        ))
                        .size(14.5)
                        .strong(),
                    );
                    ui.add_space(6.0);

                    let rt_state = app
                        .servers
                        .server_states
                        .get(&srv.id)
                        .copied()
                        .unwrap_or_else(|| app.server_supervisor.runtime_state(&srv.id));

                    if card_w < 560.0 {
                        let srv_name_lbl = ui.label(
                            RichText::new(app.i18n.tr("Tên máy chủ:", "Server Name:")).strong(),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.edit_draft.name)
                                .desired_width(card_w),
                        )
                        .labelled_by(srv_name_lbl.id);
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            let port_lbl = ui.label(RichText::new("Port:").strong());
                            ui.add(
                                egui::DragValue::new(&mut app.servers.edit_draft.port)
                                    .range(1024..=65535),
                            )
                            .labelled_by(port_lbl.id);
                        });

                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(app.i18n.tr("CPU hệ thống:", "System CPU:")).strong(),
                        );
                        server_cpu_details_wrapped(
                            ui,
                            CpuHardwareInfo::host(),
                            rt_state,
                            app.servers.server_stats.get(&srv.id),
                            &app.i18n,
                        );

                        ui.add_space(6.0);
                        let slider_cap = (card_w - 84.0).clamp(42.0, 220.0);
                        let min_ram_lbl = ui.label(
                            RichText::new(app.i18n.tr("RAM tối thiểu:", "Min RAM:")).strong(),
                        );
                        ui.scope(|ui| {
                            if ui.spacing().slider_width > slider_cap {
                                ui.spacing_mut().slider_width = slider_cap;
                            }
                            ui.add(
                                egui::Slider::new(
                                    &mut app.servers.edit_draft.ram_min_mb,
                                    512..=16384,
                                )
                                .step_by(256.0)
                                .suffix(" MB"),
                            )
                            .labelled_by(min_ram_lbl.id);
                        });
                        let max_ram_lbl = ui
                            .label(RichText::new(app.i18n.tr("RAM tối đa:", "Max RAM:")).strong());
                        ui.scope(|ui| {
                            if ui.spacing().slider_width > slider_cap {
                                ui.spacing_mut().slider_width = slider_cap;
                            }
                            ui.add(
                                egui::Slider::new(
                                    &mut app.servers.edit_draft.ram_max_mb,
                                    1024..=32768,
                                )
                                .step_by(512.0)
                                .suffix(" MB"),
                            )
                            .labelled_by(max_ram_lbl.id);
                        });

                        ui.add_space(6.0);
                        let java_lbl =
                            ui.label(RichText::new(app.i18n.t("profile.java_path")).strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.edit_draft.java_path_str)
                                .hint_text(app.i18n.tr(
                                    "Mặc định tự động chọn Java phù hợp",
                                    "Auto-select compatible Java runtime",
                                ))
                                .desired_width(card_w),
                        )
                        .labelled_by(java_lbl.id);
                        ui.horizontal_wrapped(|ui| {
                            if secondary_icon_button(
                                ui,
                                VectorIcon::Folder,
                                app.i18n.tr("Chọn...", "Browse..."),
                            )
                            .clicked()
                            {
                                if let Some(f) = crate::dialogs::pick_file_sync() {
                                    app.servers.edit_draft.java_path_str =
                                        f.to_string_lossy().to_string();
                                }
                            }
                            if !app.servers.edit_draft.java_path_str.is_empty()
                                && secondary_icon_button(
                                    ui,
                                    VectorIcon::Refresh,
                                    app.i18n.tr("Mặc định", "Auto"),
                                )
                                .clicked()
                            {
                                app.servers.edit_draft.java_path_str.clear();
                            }
                        });

                        ui.add_space(6.0);
                        let jar_lbl = ui.label(
                            RichText::new(app.i18n.tr(
                                "Tệp khởi chạy (JAR / @args):",
                                "Launch Target (JAR / @args):",
                            ))
                            .strong(),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.edit_draft.jar_file_str)
                                .hint_text(app.i18n.tr("Tự động phát hiện", "Auto-detect"))
                                .desired_width(card_w),
                        )
                        .labelled_by(jar_lbl.id);
                        let targets = app.servers.runnable_targets.clone();
                        if !targets.is_empty() {
                            ui.add_space(3.0);
                            let selected_preview =
                                if app.servers.edit_draft.jar_file_str.trim().is_empty() {
                                    app.i18n.tr("Tự động", "Auto").to_string()
                                } else {
                                    app.servers.edit_draft.jar_file_str.clone()
                                };
                            egui::ComboBox::from_id_salt("srv_runnable_target_combo")
                                .width((card_w - 4.0).max(72.0))
                                .truncate()
                                .selected_text(selected_preview)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut app.servers.edit_draft.jar_file_str,
                                        String::new(),
                                        app.i18n.tr("Tự động phát hiện", "Auto-detect"),
                                    );
                                    for t in targets {
                                        ui.selectable_value(
                                            &mut app.servers.edit_draft.jar_file_str,
                                            t.clone(),
                                            t,
                                        );
                                    }
                                })
                                .response
                                .labelled_by(jar_lbl.id);
                        }

                        ui.add_space(6.0);
                        let jvm_lbl =
                            ui.label(RichText::new(app.i18n.t("profile.jvm_args")).strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.edit_draft.jvm_flags)
                                .hint_text("-XX:+UseG1GC -XX:+ParallelRefProcEnabled")
                                .desired_width(card_w),
                        )
                        .labelled_by(jvm_lbl.id);
                    } else {
                        let col2_w = (card_w - 220.0).max(140.0);
                        egui::Grid::new("server_hw_edit_grid")
                            .num_columns(2)
                            .spacing([14.0, 9.0])
                            .show(ui, |ui| {
                                let srv_name_lbl =
                                    ui.label(app.i18n.tr("Tên máy chủ:", "Server Name:"));
                                ui.allocate_ui_with_layout(
                                    vec2(col2_w, 0.0),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.set_max_width(col2_w);
                                        ui.horizontal_wrapped(|ui| {
                                            let nw = 240.0_f32.min((col2_w - 110.0).max(96.0));
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut app.servers.edit_draft.name,
                                                )
                                                .desired_width(nw),
                                            )
                                            .labelled_by(srv_name_lbl.id);
                                            let port_lbl = ui.label("Port:");
                                            ui.add(
                                                egui::DragValue::new(
                                                    &mut app.servers.edit_draft.port,
                                                )
                                                .range(1024..=65535),
                                            )
                                            .labelled_by(port_lbl.id);
                                        });
                                    },
                                );
                                ui.end_row();

                                ui.label(app.i18n.tr("CPU hệ thống:", "System CPU:"));
                                ui.allocate_ui_with_layout(
                                    vec2(col2_w, 0.0),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.set_max_width(col2_w);
                                        let host_cpu = CpuHardwareInfo::host();
                                        let live_stats = app.servers.server_stats.get(&srv.id);
                                        server_cpu_details_wrapped(
                                            ui, host_cpu, rt_state, live_stats, &app.i18n,
                                        );
                                    },
                                );
                                ui.end_row();

                                let ram_lbl = ui.label(
                                    app.i18n.tr("RAM tối thiểu / tối đa:", "Min / Max RAM:"),
                                );
                                ui.allocate_ui_with_layout(
                                    vec2(col2_w, 0.0),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.set_max_width(col2_w);
                                        let slider_cap =
                                            ((col2_w - 190.0) * 0.5).clamp(64.0, 140.0);
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.spacing().slider_width > slider_cap {
                                                ui.spacing_mut().slider_width = slider_cap;
                                            }
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut app.servers.edit_draft.ram_min_mb,
                                                    512..=16384,
                                                )
                                                .step_by(256.0)
                                                .suffix(" MB"),
                                            )
                                            .labelled_by(ram_lbl.id);
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut app.servers.edit_draft.ram_max_mb,
                                                    1024..=32768,
                                                )
                                                .step_by(512.0)
                                                .suffix(" MB"),
                                            )
                                            .labelled_by(ram_lbl.id);
                                        });
                                    },
                                );
                                ui.end_row();

                                let java_lbl = ui.label(app.i18n.t("profile.java_path"));
                                ui.allocate_ui_with_layout(
                                    vec2(col2_w, 0.0),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.set_max_width(col2_w);
                                        ui.horizontal_wrapped(|ui| {
                                            let jw = 260.0_f32.min((col2_w - 115.0).max(96.0));
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut app.servers.edit_draft.java_path_str,
                                                )
                                                .hint_text(app.i18n.tr(
                                                    "Mặc định tự động chọn Java phù hợp",
                                                    "Auto-select compatible Java runtime",
                                                ))
                                                .desired_width(jw),
                                            )
                                            .labelled_by(java_lbl.id);
                                            if secondary_icon_button(
                                                ui,
                                                VectorIcon::Folder,
                                                app.i18n.tr("Chọn...", "Browse..."),
                                            )
                                            .clicked()
                                            {
                                                if let Some(f) = crate::dialogs::pick_file_sync() {
                                                    app.servers.edit_draft.java_path_str =
                                                        f.to_string_lossy().to_string();
                                                }
                                            }
                                            if !app.servers.edit_draft.java_path_str.is_empty()
                                                && secondary_icon_button(
                                                    ui,
                                                    VectorIcon::Refresh,
                                                    app.i18n.tr("Mặc định", "Auto"),
                                                )
                                                .clicked()
                                            {
                                                app.servers.edit_draft.java_path_str.clear();
                                            }
                                        });
                                    },
                                );
                                ui.end_row();

                                let jar_lbl = ui.label(app.i18n.tr(
                                    "Tệp khởi chạy (JAR / @args):",
                                    "Launch Target (JAR / @args):",
                                ));
                                ui.allocate_ui_with_layout(
                                    vec2(col2_w, 0.0),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.set_max_width(col2_w);
                                        ui.horizontal_wrapped(|ui| {
                                            let targets = app.servers.runnable_targets.clone();
                                            let jar_edit_w = if targets.is_empty() {
                                                300.0_f32.min(col2_w)
                                            } else {
                                                220.0_f32.min((col2_w - 165.0).max(96.0))
                                            };
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut app.servers.edit_draft.jar_file_str,
                                                )
                                                .hint_text(
                                                    app.i18n.tr("Tự động phát hiện", "Auto-detect"),
                                                )
                                                .desired_width(jar_edit_w),
                                            )
                                            .labelled_by(jar_lbl.id);
                                            if !targets.is_empty() {
                                                let selected_preview = if app
                                                    .servers
                                                    .edit_draft
                                                    .jar_file_str
                                                    .trim()
                                                    .is_empty()
                                                {
                                                    app.i18n.tr("Tự động", "Auto").to_string()
                                                } else {
                                                    app.servers.edit_draft.jar_file_str.clone()
                                                };
                                                egui::ComboBox::from_id_salt(
                                                    "srv_runnable_target_combo",
                                                )
                                                .width(150.0_f32.min((col2_w - 8.0).max(80.0)))
                                                .truncate()
                                                .selected_text(selected_preview)
                                                .show_ui(ui, |ui| {
                                                    ui.selectable_value(
                                                        &mut app.servers.edit_draft.jar_file_str,
                                                        String::new(),
                                                        app.i18n
                                                            .tr("Tự động phát hiện", "Auto-detect"),
                                                    );
                                                    for t in targets {
                                                        ui.selectable_value(
                                                            &mut app
                                                                .servers
                                                                .edit_draft
                                                                .jar_file_str,
                                                            t.clone(),
                                                            t,
                                                        );
                                                    }
                                                })
                                                .response
                                                .labelled_by(jar_lbl.id);
                                            }
                                        });
                                    },
                                );
                                ui.end_row();

                                let jvm_lbl = ui.label(app.i18n.t("profile.jvm_args"));
                                let jvm_w = 360.0_f32.min((col2_w - 4.0).max(96.0));
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut app.servers.edit_draft.jvm_flags,
                                    )
                                    .hint_text("-XX:+UseG1GC -XX:+ParallelRefProcEnabled")
                                    .desired_width(jvm_w),
                                )
                                .labelled_by(jvm_lbl.id);
                                ui.end_row();
                            });
                    }

                    ui.add_space(10.0);
                    if primary_icon_button(
                        ui,
                        VectorIcon::Save,
                        app.i18n.tr(
                            "Lưu cấu hình Phần cứng & JVM",
                            "Save Hardware & JVM Settings",
                        ),
                    )
                    .clicked()
                    {
                        app.save_server_config_edits();
                    }
                },
            );

            ui.add_space(10.0);
            gaming_card_3d(
                ui,
                "srv_quick_visual_props_card",
                false,
                ACCENT_VIOLET,
                Margin::same(14),
                |ui| {
                    let card_w = ui.available_width();
                    ui.set_width(card_w);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(app.i18n.tr(
                                "Điều khiển Trực quan Nhanh (server.properties)",
                                "Visual Quick-Controls (server.properties)",
                            ))
                            .size(14.5)
                            .strong(),
                        );
                        if primary_icon_button(
                            ui,
                            VectorIcon::Save,
                            app.i18n
                                .tr("Lưu server.properties", "Save server.properties"),
                        )
                        .clicked()
                        {
                            app.save_server_properties_edits();
                        }
                    });
                    ui.add_space(6.0);

                    let lbl_online = app
                        .i18n
                        .tr("Xác thực Mojang (online-mode)", "Mojang Auth (online-mode)")
                        .to_string();
                    let lbl_wl = app.i18n.tr("Bật Whitelist", "Enable Whitelist").to_string();
                    let lbl_ewl = app
                        .i18n
                        .tr("Cưỡng chế Whitelist", "Enforce Whitelist")
                        .to_string();
                    let lbl_pvp = "PvP".to_string();
                    let lbl_fly = app.i18n.tr("Cho phép bay", "Allow Flight").to_string();
                    let lbl_cmd = app
                        .i18n
                        .tr("Khối lệnh (Command Block)", "Command Blocks")
                        .to_string();
                    let lbl_hc = "Hardcore".to_string();

                    ui.horizontal_wrapped(|ui| {
                        let props = &mut app.servers.properties_map;
                        prop_checkbox(ui, props, "online-mode", true, &lbl_online);
                        prop_checkbox(ui, props, "white-list", false, &lbl_wl);
                        prop_checkbox(ui, props, "enforce-whitelist", false, &lbl_ewl);
                        prop_checkbox(ui, props, "pvp", true, &lbl_pvp);
                        prop_checkbox(ui, props, "allow-flight", false, &lbl_fly);
                        prop_checkbox(ui, props, "enable-command-block", false, &lbl_cmd);
                        prop_checkbox(ui, props, "hardcore", false, &lbl_hc);
                    });

                    ui.add_space(6.0);
                    let lbl_gm = app.i18n.tr("Chế độ chơi:", "Gamemode:").to_string();
                    let lbl_diff = app.i18n.tr("Độ khó:", "Difficulty:").to_string();
                    let lbl_maxp = app
                        .i18n
                        .tr("Giới hạn người chơi:", "Max Players:")
                        .to_string();
                    let lbl_view = app
                        .i18n
                        .tr("Tầm nhìn / Mô phỏng:", "View / Sim Distance:")
                        .to_string();
                    let lbl_spawn = app
                        .i18n
                        .tr("Bảo vệ Spawn:", "Spawn Protection:")
                        .to_string();
                    let lbl_world = app
                        .i18n
                        .tr("Thế giới & Seed:", "World Name & Seed:")
                        .to_string();

                    if card_w < 520.0 {
                        ui.horizontal_wrapped(|ui| {
                            let gm_lbl = ui.label(RichText::new(&lbl_gm).strong());
                            prop_combo(
                                ui,
                                &mut app.servers.properties_map,
                                "gamemode",
                                "survival",
                                &["survival", "creative", "adventure", "spectator"],
                                gm_lbl.id,
                            );
                            let diff_lbl = ui.label(RichText::new(&lbl_diff).strong());
                            prop_combo(
                                ui,
                                &mut app.servers.properties_map,
                                "difficulty",
                                "normal",
                                &["peaceful", "easy", "normal", "hard"],
                                diff_lbl.id,
                            );
                        });
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            let maxp_lbl = ui.label(RichText::new(&lbl_maxp).strong());
                            prop_u32_drag(
                                ui,
                                &mut app.servers.properties_map,
                                "max-players",
                                20,
                                1..=1000,
                                maxp_lbl.id,
                            );
                            let spawn_lbl = ui.label(RichText::new(&lbl_spawn).strong());
                            prop_u32_drag(
                                ui,
                                &mut app.servers.properties_map,
                                "spawn-protection",
                                16,
                                0..=256,
                                spawn_lbl.id,
                            );
                        });
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            let view_lbl = ui.label(RichText::new(&lbl_view).strong());
                            prop_u32_drag(
                                ui,
                                &mut app.servers.properties_map,
                                "view-distance",
                                10,
                                2..=32,
                                view_lbl.id,
                            );
                            ui.label("/");
                            prop_u32_drag(
                                ui,
                                &mut app.servers.properties_map,
                                "simulation-distance",
                                10,
                                2..=32,
                                view_lbl.id,
                            );
                        });
                        ui.add_space(6.0);
                        let motd_lbl = ui.label(RichText::new("MOTD:").strong());
                        let motd_entry = app
                            .servers
                            .properties_map
                            .entry("motd".to_string())
                            .or_insert_with(|| format!("{} - TriHienKun Laucher", srv.name));
                        ui.add(egui::TextEdit::singleline(motd_entry).desired_width(card_w))
                            .labelled_by(motd_lbl.id);

                        ui.add_space(6.0);
                        let world_lbl = ui.label(RichText::new(&lbl_world).strong());
                        let lvl_entry = app
                            .servers
                            .properties_map
                            .entry("level-name".to_string())
                            .or_insert_with(|| "world".to_string());
                        ui.add(egui::TextEdit::singleline(lvl_entry).desired_width(card_w))
                            .labelled_by(world_lbl.id);
                        ui.add_space(4.0);
                        let seed_lbl = ui.label(RichText::new("Seed:").strong());
                        let seed_entry = app
                            .servers
                            .properties_map
                            .entry("level-seed".to_string())
                            .or_default();
                        ui.add(
                            egui::TextEdit::singleline(seed_entry)
                                .hint_text("optional")
                                .desired_width(card_w),
                        )
                        .labelled_by(seed_lbl.id);
                    } else {
                        let wide_props = card_w >= 680.0;
                        let prop_edit_cap = if wide_props {
                            ((card_w - 380.0) * 0.55).clamp(96.0, 240.0)
                        } else {
                            (card_w - 220.0).clamp(96.0, 240.0)
                        };
                        egui::Grid::new("srv_quick_props_grid")
                            .num_columns(if wide_props { 4 } else { 2 })
                            .spacing([14.0, 8.0])
                            .show(ui, |ui| {
                                let gm_lbl = ui.label(&lbl_gm);
                                prop_combo(
                                    ui,
                                    &mut app.servers.properties_map,
                                    "gamemode",
                                    "survival",
                                    &["survival", "creative", "adventure", "spectator"],
                                    gm_lbl.id,
                                );
                                if !wide_props {
                                    ui.end_row();
                                }
                                let diff_lbl = ui.label(&lbl_diff);
                                prop_combo(
                                    ui,
                                    &mut app.servers.properties_map,
                                    "difficulty",
                                    "normal",
                                    &["peaceful", "easy", "normal", "hard"],
                                    diff_lbl.id,
                                );
                                ui.end_row();

                                let maxp_lbl = ui.label(&lbl_maxp);
                                prop_u32_drag(
                                    ui,
                                    &mut app.servers.properties_map,
                                    "max-players",
                                    20,
                                    1..=1000,
                                    maxp_lbl.id,
                                );
                                if !wide_props {
                                    ui.end_row();
                                }
                                let view_lbl = ui.label(&lbl_view);
                                ui.horizontal_wrapped(|ui| {
                                    prop_u32_drag(
                                        ui,
                                        &mut app.servers.properties_map,
                                        "view-distance",
                                        10,
                                        2..=32,
                                        view_lbl.id,
                                    );
                                    ui.label("/");
                                    prop_u32_drag(
                                        ui,
                                        &mut app.servers.properties_map,
                                        "simulation-distance",
                                        10,
                                        2..=32,
                                        view_lbl.id,
                                    );
                                });
                                ui.end_row();

                                let spawn_lbl = ui.label(&lbl_spawn);
                                prop_u32_drag(
                                    ui,
                                    &mut app.servers.properties_map,
                                    "spawn-protection",
                                    16,
                                    0..=256,
                                    spawn_lbl.id,
                                );
                                if !wide_props {
                                    ui.end_row();
                                }
                                let motd_lbl = ui.label("MOTD:");
                                let motd_entry = app
                                    .servers
                                    .properties_map
                                    .entry("motd".to_string())
                                    .or_insert_with(|| {
                                        format!("{} - TriHienKun Laucher", srv.name)
                                    });
                                ui.add(
                                    egui::TextEdit::singleline(motd_entry)
                                        .desired_width(prop_edit_cap),
                                )
                                .labelled_by(motd_lbl.id);
                                ui.end_row();

                                let world_lbl = ui.label(&lbl_world);
                                let lvl_w = 135.0_f32.min(prop_edit_cap);
                                let lvl_entry = app
                                    .servers
                                    .properties_map
                                    .entry("level-name".to_string())
                                    .or_insert_with(|| "world".to_string());
                                ui.add(egui::TextEdit::singleline(lvl_entry).desired_width(lvl_w))
                                    .labelled_by(world_lbl.id);
                                if !wide_props {
                                    ui.end_row();
                                }
                                let seed_lbl = ui.label("Seed:");
                                let seed_entry = app
                                    .servers
                                    .properties_map
                                    .entry("level-seed".to_string())
                                    .or_default();
                                ui.add(
                                    egui::TextEdit::singleline(seed_entry)
                                        .hint_text("optional")
                                        .desired_width(prop_edit_cap),
                                )
                                .labelled_by(seed_lbl.id);
                                ui.end_row();
                            });
                    }
                },
            );

            ui.add_space(10.0);
            gaming_card_3d(
                ui,
                "srv_props_kv_card",
                false,
                ACCENT_EMERALD,
                Margin::same(14),
                |ui| {
                    let card_w = ui.available_width();
                    ui.set_width(card_w);
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Bảng thuộc tính chi tiết (server.properties)",
                            "Raw Key-Value Table (server.properties)",
                        ))
                        .size(14.5)
                        .strong(),
                    );
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        let max_kv_w = (ui.max_rect().width() - 16.0).max(72.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.properties_filter)
                                .hint_text(app.i18n.tr("Lọc tên khoá...", "Filter keys..."))
                                .desired_width(160.0_f32.min(max_kv_w)),
                        );
                        ui.separator();
                        ui.label(RichText::new(app.i18n.tr("Thêm khoá:", "Add key:")).strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.new_prop_key)
                                .hint_text("online-mode")
                                .desired_width(145.0_f32.min(max_kv_w)),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.new_prop_val)
                                .hint_text("false")
                                .desired_width(145.0_f32.min(max_kv_w)),
                        );
                        if primary_icon_button(
                            ui,
                            VectorIcon::Save,
                            app.i18n
                                .tr("Lưu server.properties", "Save server.properties"),
                        )
                        .clicked()
                        {
                            app.save_server_properties_edits();
                        }
                    });

                    ui.separator();

                    let filter_low = app.servers.properties_filter.trim().to_ascii_lowercase();
                    let keys: Vec<String> = app
                        .servers
                        .properties_map
                        .keys()
                        .filter(|k| {
                            filter_low.is_empty() || k.to_ascii_lowercase().contains(&filter_low)
                        })
                        .cloned()
                        .collect();

                    if card_w < 500.0 {
                        for (idx, key) in keys.into_iter().enumerate() {
                            if idx > 0 {
                                ui.add_space(4.0);
                            }
                            let k_lbl = bounded_wrap_label(
                                ui,
                                RichText::new(&key).monospace().size(12.2).strong(),
                                card_w,
                            );
                            if let Some(val) = app.servers.properties_map.get_mut(&key) {
                                ui.add(egui::TextEdit::singleline(val).desired_width(card_w))
                                    .labelled_by(k_lbl.id);
                            }
                        }
                    } else {
                        let val_w = (card_w - 230.0).clamp(80.0, 320.0);
                        egui::Grid::new("server_properties_grid")
                            .num_columns(2)
                            .spacing([16.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for key in keys {
                                    let k_lbl = bounded_wrap_label(
                                        ui,
                                        RichText::new(&key).monospace().size(12.5),
                                        200.0,
                                    );
                                    if let Some(val) = app.servers.properties_map.get_mut(&key) {
                                        ui.add(
                                            egui::TextEdit::singleline(val).desired_width(val_w),
                                        )
                                        .labelled_by(k_lbl.id);
                                    }
                                    ui.end_row();
                                }
                            });
                    }
                },
            );
        },
    );
}

fn prop_bool(map: &BTreeMap<String, String>, key: &str, default_val: bool) -> bool {
    map.get(key)
        .map(|v| {
            let trimmed = v.trim();
            if default_val {
                !trimmed.eq_ignore_ascii_case("false")
            } else {
                trimmed.eq_ignore_ascii_case("true")
            }
        })
        .unwrap_or(default_val)
}

fn set_prop_bool(map: &mut BTreeMap<String, String>, key: &str, val: bool) {
    map.insert(
        key.to_string(),
        if val { "true" } else { "false" }.to_string(),
    );
}

fn prop_u32(map: &BTreeMap<String, String>, key: &str, default_val: u32) -> u32 {
    map.get(key)
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(default_val)
}

fn render_player_rules_card(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let mut ask_save = false;

    gaming_card_3d(
        ui,
        "srv_player_rules_card",
        false,
        ACCENT_CYAN,
        Margin::same(14),
        |ui| {
            ui.set_width(ui.available_width());
            ui.heading(
                RichText::new(app.i18n.tr(
                    "Cấu hình Quy tắc Người chơi",
                    "Player Rules & Configuration",
                ))
                .size(16.0)
                .strong(),
            );
            ui.add_space(8.0);

            let current_gm = app
                .servers
                .properties_map
                .get("gamemode")
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "survival".to_string());

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(app.i18n.tr("Chế độ chơi mặc định:", "Default Gamemode:"))
                        .strong(),
                );
                for (gm_val, gm_vi, gm_en) in [
                    ("survival", "Sinh tồn", "Survival"),
                    ("creative", "Sáng tạo", "Creative"),
                    ("adventure", "Phiêu lưu", "Adventure"),
                    ("spectator", "Khán giả", "Spectator"),
                ] {
                    if ui
                        .selectable_label(current_gm == gm_val, app.i18n.tr(gm_vi, gm_en))
                        .clicked()
                    {
                        app.servers
                            .properties_map
                            .insert("gamemode".to_string(), gm_val.to_string());
                    }
                }
            });

            let mut op_perm =
                prop_u32(&app.servers.properties_map, "op-permission-level", 4).clamp(1, 4);
            let mut max_players =
                prop_u32(&app.servers.properties_map, "max-players", 20).clamp(1, 1000);
            let mut idle_timeout =
                prop_u32(&app.servers.properties_map, "player-idle-timeout", 0).min(1440);
            let mut spawn_prot =
                prop_u32(&app.servers.properties_map, "spawn-protection", 16).min(256);

            ui.horizontal_wrapped(|ui| {
                let max_lbl = ui.label(
                    RichText::new(app.i18n.tr("Tối đa người chơi:", "Max Players:")).strong(),
                );
                if ui
                    .add(egui::DragValue::new(&mut max_players).range(1..=1000))
                    .labelled_by(max_lbl.id)
                    .changed()
                {
                    app.servers
                        .properties_map
                        .insert("max-players".to_string(), max_players.to_string());
                }

                ui.separator();
                let idle_lbl = ui
                    .label(RichText::new(app.i18n.tr("Treo máy tự kick:", "Idle Kick:")).strong());
                if ui
                    .add(
                        egui::DragValue::new(&mut idle_timeout)
                            .range(0..=1440)
                            .suffix(app.i18n.tr(" phút", " min")),
                    )
                    .labelled_by(idle_lbl.id)
                    .changed()
                {
                    app.servers
                        .properties_map
                        .insert("player-idle-timeout".to_string(), idle_timeout.to_string());
                }

                ui.separator();
                let sp_lbl = ui.label(
                    RichText::new(app.i18n.tr("Bảo vệ khu Spawn:", "Spawn Protection:")).strong(),
                );
                if ui
                    .add(
                        egui::DragValue::new(&mut spawn_prot)
                            .range(0..=256)
                            .suffix(" block"),
                    )
                    .labelled_by(sp_lbl.id)
                    .changed()
                {
                    app.servers
                        .properties_map
                        .insert("spawn-protection".to_string(), spawn_prot.to_string());
                }

                ui.separator();
                ui.label(
                    RichText::new(app.i18n.tr("Mức quyền OP mặc định:", "Default OP Level:"))
                        .strong(),
                );
                for lvl in 1..=4u32 {
                    if ui
                        .selectable_value(
                            &mut op_perm,
                            lvl,
                            format!("Cấp {lvl}").replace("Cấp", app.i18n.tr("Cấp", "Lv")),
                        )
                        .changed()
                    {
                        app.servers
                            .properties_map
                            .insert("op-permission-level".to_string(), op_perm.to_string());
                    }
                }
            });

            let mut pvp = prop_bool(&app.servers.properties_map, "pvp", true);
            let mut allow_flight = prop_bool(&app.servers.properties_map, "allow-flight", false);
            let mut force_gm = prop_bool(&app.servers.properties_map, "force-gamemode", false);
            let mut whitelist_on = prop_bool(&app.servers.properties_map, "white-list", false);
            let mut enforce_wl = prop_bool(&app.servers.properties_map, "enforce-whitelist", false);
            let mut hide_players =
                prop_bool(&app.servers.properties_map, "hide-online-players", false);

            ui.horizontal_wrapped(|ui| {
                if crate::views::wrapped_checkbox(
                    ui,
                    &mut pvp,
                    app.i18n.tr("Cho phép PvP", "Allow PvP"),
                )
                .changed()
                {
                    set_prop_bool(&mut app.servers.properties_map, "pvp", pvp);
                }
                if crate::views::wrapped_checkbox(
                    ui,
                    &mut allow_flight,
                    app.i18n.tr("Cho phép bay", "Allow Flight"),
                )
                .changed()
                {
                    set_prop_bool(
                        &mut app.servers.properties_map,
                        "allow-flight",
                        allow_flight,
                    );
                }
                if crate::views::wrapped_checkbox(
                    ui,
                    &mut force_gm,
                    app.i18n
                        .tr("Ép chế độ chơi khi vào", "Force Gamemode on Join"),
                )
                .changed()
                {
                    set_prop_bool(&mut app.servers.properties_map, "force-gamemode", force_gm);
                }
                if crate::views::wrapped_checkbox(
                    ui,
                    &mut whitelist_on,
                    app.i18n.tr("Bật Whitelist", "Enable Whitelist"),
                )
                .changed()
                {
                    set_prop_bool(&mut app.servers.properties_map, "white-list", whitelist_on);
                }
                if crate::views::wrapped_checkbox(
                    ui,
                    &mut enforce_wl,
                    app.i18n
                        .tr("Đuổi ngay người ngoài Whitelist", "Enforce Whitelist"),
                )
                .changed()
                {
                    set_prop_bool(
                        &mut app.servers.properties_map,
                        "enforce-whitelist",
                        enforce_wl,
                    );
                }
                if crate::views::wrapped_checkbox(
                    ui,
                    &mut hide_players,
                    app.i18n
                        .tr("Ẩn danh sách người chơi", "Hide Online Players"),
                )
                .changed()
                {
                    set_prop_bool(
                        &mut app.servers.properties_map,
                        "hide-online-players",
                        hide_players,
                    );
                }
            });

            ui.add_space(6.0);
            if primary_icon_button(
                ui,
                VectorIcon::Save,
                app.i18n
                    .tr("Lưu cấu hình người chơi", "Save Player Configuration"),
            )
            .clicked()
            {
                ask_save = true;
            }
        },
    );

    if ask_save {
        app.click_sfx();
        app.save_player_rules_properties();
    }
}

#[derive(Clone)]
struct DisplayPlayerRow {
    name: String,
    uuid: String,
    is_online: bool,
    op_level: Option<u8>,
    is_whitelisted: bool,
    is_banned: bool,
    has_playerdata: bool,
    playerdata_bytes: u64,
}

enum PlayerQuickAction {
    LiveCmd(String, String),
    GrantOp(String, u8),
    RevokeOp(String),
    AddWhitelist(String),
    RemoveWhitelist(String),
    BanPlayer(String),
    PardonPlayer(String),
    AskResetData(String),
    CancelResetData,
    ConfirmResetData(String, String),
}

fn render_known_players_card(app: &mut LauncherAppState, ui: &mut egui::Ui, srv: &ManagedServer) {
    let rt_state = app
        .servers
        .server_states
        .get(&srv.id)
        .copied()
        .unwrap_or_else(|| app.server_supervisor.runtime_state(&srv.id));
    let server_online = rt_state == ServerRuntimeState::Online;
    let online_names = app
        .servers
        .server_stats
        .get(&srv.id)
        .map(|s| s.online_player_names.clone())
        .unwrap_or_default();

    let default_op_lvl =
        prop_u32(&app.servers.properties_map, "op-permission-level", 4).clamp(1, 4) as u8;

    let mut rows: Vec<DisplayPlayerRow> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    for on_name in &online_names {
        let low = on_name.to_ascii_lowercase();
        if !seen_names.insert(low) {
            continue;
        }
        let known = app
            .servers
            .known_players
            .iter()
            .find(|k| k.name.eq_ignore_ascii_case(on_name));
        let op_level = app
            .servers
            .ops_list
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(on_name))
            .map(|o| o.level);
        let is_whitelisted = app
            .servers
            .whitelist
            .iter()
            .any(|w| w.name.eq_ignore_ascii_case(on_name));
        let is_banned = app
            .servers
            .banlist
            .iter()
            .any(|b| b.name.eq_ignore_ascii_case(on_name));
        rows.push(DisplayPlayerRow {
            name: on_name.clone(),
            uuid: known.map(|k| k.uuid.clone()).unwrap_or_default(),
            is_online: true,
            op_level,
            is_whitelisted,
            is_banned,
            has_playerdata: known.is_some_and(|k| k.has_playerdata),
            playerdata_bytes: known.map_or(0, |k| k.playerdata_bytes),
        });
    }

    for k in &app.servers.known_players {
        let low = k.name.to_ascii_lowercase();
        if !seen_names.insert(low) {
            continue;
        }
        let op_level = app
            .servers
            .ops_list
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(&k.name) || o.uuid.eq_ignore_ascii_case(&k.uuid))
            .map(|o| o.level);
        let is_whitelisted =
            app.servers.whitelist.iter().any(|w| {
                w.name.eq_ignore_ascii_case(&k.name) || w.uuid.eq_ignore_ascii_case(&k.uuid)
            });
        let is_banned =
            app.servers.banlist.iter().any(|b| {
                b.name.eq_ignore_ascii_case(&k.name) || b.uuid.eq_ignore_ascii_case(&k.uuid)
            });
        rows.push(DisplayPlayerRow {
            name: k.name.clone(),
            uuid: k.uuid.clone(),
            is_online: false,
            op_level,
            is_whitelisted,
            is_banned,
            has_playerdata: k.has_playerdata,
            playerdata_bytes: k.playerdata_bytes,
        });
    }

    let filter_low = app.servers.player_filter.trim().to_ascii_lowercase();
    if !filter_low.is_empty() {
        rows.retain(|r| {
            r.name.to_ascii_lowercase().contains(&filter_low)
                || r.uuid.to_ascii_lowercase().contains(&filter_low)
        });
    }

    let mut pending_action: Option<PlayerQuickAction> = None;
    let mut ask_refresh = false;

    gaming_card_3d(
        ui,
        "srv_known_players_card",
        server_online && !online_names.is_empty(),
        ACCENT_EMERALD,
        Margin::same(14),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.heading(
                    RichText::new(app.i18n.tr(
                        "Người chơi Trực tuyến & Đã tham gia",
                        "Online & Known Players",
                    ))
                    .size(16.0)
                    .strong(),
                );
                badge(
                    ui,
                    &format!(
                        "{} {}",
                        online_names.len(),
                        app.i18n.tr("trực tuyến", "online")
                    ),
                    if online_names.is_empty() {
                        Color32::GRAY
                    } else {
                        ACCENT_EMERALD
                    },
                );
                badge(
                    ui,
                    &format!("{} {}", rows.len(), app.i18n.tr("hồ sơ", "profiles")),
                    ACCENT_BLUE,
                );
                let pf_w = 180.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                ui.add(
                    egui::TextEdit::singleline(&mut app.servers.player_filter)
                        .hint_text(
                            app.i18n
                                .tr("Tìm tên hoặc UUID...", "Filter name or UUID..."),
                        )
                        .desired_width(pf_w),
                );
                if secondary_icon_button(ui, VectorIcon::Refresh, app.i18n.tr("Làm mới", "Refresh"))
                    .clicked()
                {
                    ask_refresh = true;
                }
            });

            ui.add_space(6.0);
            if rows.is_empty() {
                ui.label(
                    RichText::new(app.i18n.tr(
                        "Chưa ghi nhận người chơi nào tham gia máy chủ này.",
                        "No players have joined or been configured on this server yet.",
                    ))
                    .weak(),
                );
                return;
            }

            for row in rows {
                let card_accent = if row.is_banned {
                    ACCENT_RED
                } else if row.is_online {
                    ACCENT_EMERALD
                } else if row.op_level.is_some() {
                    ACCENT_AMBER
                } else {
                    ACCENT_BLUE
                };
                gaming_card_3d(
                    ui,
                    ("srv_player_row", &row.name, &row.uuid),
                    row.is_online,
                    card_accent,
                    Margin::symmetric(10, 8),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&row.name).size(13.8).strong());
                            badge(
                                ui,
                                if row.is_online {
                                    app.i18n.tr("Trực tuyến", "Online")
                                } else {
                                    "Offline"
                                },
                                if row.is_online {
                                    ACCENT_EMERALD
                                } else {
                                    Color32::GRAY
                                },
                            );
                            if let Some(lvl) = row.op_level {
                                badge(
                                    ui,
                                    &format!("OP {} {lvl}", app.i18n.tr("Cấp", "Lv")),
                                    ACCENT_AMBER,
                                );
                            }
                            if row.is_whitelisted {
                                badge(ui, "Whitelist", ACCENT_EMERALD);
                            }
                            if row.is_banned {
                                badge(ui, app.i18n.tr("Đang bị cấm", "Banned"), ACCENT_RED);
                            }
                            if row.has_playerdata {
                                badge(
                                    ui,
                                    &format!(
                                        "{}: {}",
                                        app.i18n.tr("Dữ liệu", "Save"),
                                        format_bytes(row.playerdata_bytes)
                                    ),
                                    ACCENT_CYAN,
                                );
                            }
                            if !row.uuid.is_empty() {
                                bounded_wrap_label(
                                    ui,
                                    RichText::new(&row.uuid).monospace().size(11.0).weak(),
                                    ui.available_width(),
                                );
                            }
                        });

                        let is_named_player = !row.name.starts_with("UUID ");
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if server_online && row.is_online && is_named_player {
                                for (gm_cmd, gm_vi, gm_en) in [
                                    ("survival", "Sinh tồn", "Survival"),
                                    ("creative", "Sáng tạo", "Creative"),
                                    ("spectator", "Khán giả", "Spectator"),
                                ] {
                                    if secondary_button(ui, app.i18n.tr(gm_vi, gm_en)).clicked() {
                                        let label = app.i18n.tr(gm_vi, gm_en);
                                        let toast = if app.settings.language == Language::Vi {
                                            format!("Đã chuyển '{}' sang chế độ {label}.", row.name)
                                        } else {
                                            format!("Set '{}' gamemode to {label}.", row.name)
                                        };
                                        pending_action = Some(PlayerQuickAction::LiveCmd(
                                            format!("gamemode {gm_cmd} {}", row.name),
                                            toast,
                                        ));
                                    }
                                }
                                if secondary_button(ui, app.i18n.tr("Xóa túi đồ", "Clear Inv"))
                                    .clicked()
                                {
                                    let toast = if app.settings.language == Language::Vi {
                                        format!("Đã xóa túi đồ của '{}'.", row.name)
                                    } else {
                                        format!("Cleared inventory for '{}'.", row.name)
                                    };
                                    pending_action = Some(PlayerQuickAction::LiveCmd(
                                        format!("clear {}", row.name),
                                        toast,
                                    ));
                                }
                                if danger_icon_button(ui, VectorIcon::Stop, "Kick").clicked() {
                                    let toast = if app.settings.language == Language::Vi {
                                        format!("Đã kick '{}' khỏi máy chủ.", row.name)
                                    } else {
                                        format!("Kicked '{}' from server.", row.name)
                                    };
                                    pending_action = Some(PlayerQuickAction::LiveCmd(
                                        format!("kick {}", row.name),
                                        toast,
                                    ));
                                }
                            }

                            if is_named_player {
                                if row.op_level.is_some() {
                                    if secondary_button(ui, app.i18n.tr("Gỡ OP", "De-OP")).clicked()
                                    {
                                        pending_action =
                                            Some(PlayerQuickAction::RevokeOp(row.name.clone()));
                                    }
                                } else if secondary_button(ui, "+OP").clicked() {
                                    pending_action = Some(PlayerQuickAction::GrantOp(
                                        row.name.clone(),
                                        default_op_lvl,
                                    ));
                                }

                                if row.is_whitelisted {
                                    if secondary_button(ui, app.i18n.tr("Gỡ WL", "Unwhitelist"))
                                        .clicked()
                                    {
                                        pending_action = Some(PlayerQuickAction::RemoveWhitelist(
                                            row.name.clone(),
                                        ));
                                    }
                                } else if secondary_button(ui, "+Whitelist").clicked() {
                                    pending_action =
                                        Some(PlayerQuickAction::AddWhitelist(row.name.clone()));
                                }

                                if row.is_banned {
                                    if secondary_button(ui, app.i18n.tr("Ân xá", "Pardon"))
                                        .clicked()
                                    {
                                        pending_action =
                                            Some(PlayerQuickAction::PardonPlayer(row.name.clone()));
                                    }
                                } else if danger_icon_button(
                                    ui,
                                    VectorIcon::Shield,
                                    app.i18n.tr("Cấm", "Ban"),
                                )
                                .clicked()
                                {
                                    pending_action =
                                        Some(PlayerQuickAction::BanPlayer(row.name.clone()));
                                }
                            }

                            if !row.is_online && row.has_playerdata && !row.uuid.is_empty() {
                                let confirming = app.servers.confirm_reset_player_uuid.as_deref()
                                    == Some(row.uuid.as_str());
                                if confirming {
                                    if danger_icon_button(
                                        ui,
                                        VectorIcon::Trash,
                                        app.i18n.tr("Xác nhận xóa dữ liệu", "Confirm Reset"),
                                    )
                                    .clicked()
                                    {
                                        pending_action = Some(PlayerQuickAction::ConfirmResetData(
                                            row.uuid.clone(),
                                            row.name.clone(),
                                        ));
                                    }
                                    if secondary_button(ui, app.i18n.t("common.cancel")).clicked() {
                                        pending_action = Some(PlayerQuickAction::CancelResetData);
                                    }
                                } else if secondary_icon_button(
                                    ui,
                                    VectorIcon::Trash,
                                    app.i18n.tr("Đặt lại dữ liệu", "Reset Save"),
                                )
                                .clicked()
                                {
                                    pending_action =
                                        Some(PlayerQuickAction::AskResetData(row.uuid.clone()));
                                }
                            }
                        });
                    },
                );
                ui.add_space(4.0);
            }
        },
    );

    if ask_refresh {
        app.click_sfx();
        app.refresh_selected_server_tabs();
    }
    if let Some(act) = pending_action {
        app.click_sfx();
        match act {
            PlayerQuickAction::LiveCmd(cmd, toast) => app.run_live_player_command(&cmd, toast),
            PlayerQuickAction::GrantOp(name, lvl) => app.upsert_server_op_player(&name, lvl, false),
            PlayerQuickAction::RevokeOp(name) => app.remove_server_op(&name),
            PlayerQuickAction::AddWhitelist(name) => app.add_server_whitelist_by_name(&name),
            PlayerQuickAction::RemoveWhitelist(name) => app.remove_server_whitelist(&name),
            PlayerQuickAction::BanPlayer(name) => {
                app.add_server_ban_by_name(&name, "Banned by server operator")
            }
            PlayerQuickAction::PardonPlayer(name) => app.remove_server_ban(&name),
            PlayerQuickAction::AskResetData(uuid) => {
                app.servers.confirm_reset_player_uuid = Some(uuid);
            }
            PlayerQuickAction::CancelResetData => {
                app.servers.confirm_reset_player_uuid = None;
            }
            PlayerQuickAction::ConfirmResetData(uuid, name) => {
                app.reset_offline_player_data(&uuid, &name);
            }
        }
    }
}

fn render_ops_card(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let heading = app
        .i18n
        .tr("Quản trị viên OP (Operators)", "Server Operators (OP)")
        .to_string();
    let player_hint = app
        .i18n
        .tr("Tên người chơi...", "Player username...")
        .to_string();
    let add_lbl = app.i18n.tr("Cấp OP", "Grant OP").to_string();
    let remove_lbl = app.i18n.tr("Gỡ OP", "Revoke").to_string();
    let empty_lbl = app
        .i18n
        .tr(
            "Chưa có quản trị viên OP nào.",
            "No server operators configured.",
        )
        .to_string();

    let mut ask_add = false;
    let mut ask_remove: Option<String> = None;

    gaming_card_3d(
        ui,
        "srv_ops_list_card",
        false,
        ACCENT_AMBER,
        Margin::same(12),
        |ui| {
            ui.set_width(ui.available_width());
            ui.heading(RichText::new(&heading).size(15.2).strong());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let op_w = 145.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                ui.add(
                    egui::TextEdit::singleline(&mut app.servers.new_op_name)
                        .hint_text(&player_hint)
                        .desired_width(op_w),
                );
                for lvl in 1..=4u8 {
                    let lbl = format!("{} {lvl}", app.i18n.tr("Cấp", "Lv"));
                    ui.selectable_value(&mut app.servers.new_op_level, lvl, lbl);
                }
                crate::views::wrapped_checkbox(
                    ui,
                    &mut app.servers.new_op_bypass_limit,
                    app.i18n.tr("Vượt giới hạn slot", "Bypass Slot Limit"),
                );
                if primary_icon_button(ui, VectorIcon::Plus, &add_lbl).clicked() {
                    ask_add = true;
                }
            });
            ui.separator();

            let ops_snapshot = app.servers.ops_list.clone();
            if ops_snapshot.is_empty() {
                ui.label(RichText::new(&empty_lbl).weak());
            }
            for item in ops_snapshot {
                gaming_card_3d(
                    ui,
                    ("op_entry", &item.name),
                    false,
                    ACCENT_AMBER,
                    Margin::symmetric(10, 6),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&item.name).strong());
                            badge(
                                ui,
                                &format!("{} {}", app.i18n.tr("Cấp", "Lv"), item.level),
                                ACCENT_AMBER,
                            );
                            if item.bypasses_player_limit {
                                badge(ui, app.i18n.tr("Vượt slot", "Bypass limit"), ACCENT_CYAN);
                            }
                            if danger_icon_button(ui, VectorIcon::Trash, &remove_lbl).clicked() {
                                ask_remove = Some(item.name.clone());
                            }
                        });
                        bounded_wrap_label(
                            ui,
                            RichText::new(&item.uuid).monospace().size(11.0).weak(),
                            ui.available_width(),
                        );
                    },
                );
                ui.add_space(4.0);
            }
        },
    );

    if ask_add {
        app.click_sfx();
        app.add_server_op();
    }
    if let Some(name) = ask_remove {
        app.click_sfx();
        app.remove_server_op(&name);
    }
}

fn render_whitelist_card(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let wl_heading = app
        .i18n
        .tr("Danh sách Trắng (Whitelist)", "Whitelist")
        .to_string();
    let player_hint = app
        .i18n
        .tr("Tên người chơi...", "Player username...")
        .to_string();
    let add_lbl = app.i18n.tr("Thêm", "Add").to_string();
    let remove_lbl = app.i18n.tr("Gỡ", "Remove").to_string();
    let empty_wl = app
        .i18n
        .tr(
            "Chưa có người chơi nào trong Whitelist.",
            "No players in whitelist.",
        )
        .to_string();

    let mut ask_add = false;
    let mut ask_remove: Option<String> = None;

    gaming_card_3d(
        ui,
        "srv_whitelist_card",
        false,
        ACCENT_EMERALD,
        Margin::same(12),
        |ui| {
            ui.set_width(ui.available_width());
            ui.heading(RichText::new(&wl_heading).size(15.2).strong());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let wl_w = 170.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                ui.add(
                    egui::TextEdit::singleline(&mut app.servers.new_wl_name)
                        .hint_text(&player_hint)
                        .desired_width(wl_w),
                );
                if primary_icon_button(ui, VectorIcon::Plus, &add_lbl).clicked() {
                    ask_add = true;
                }
            });
            ui.separator();

            let wl_snapshot = app.servers.whitelist.clone();
            if wl_snapshot.is_empty() {
                ui.label(RichText::new(&empty_wl).weak());
            }
            for item in wl_snapshot {
                gaming_card_3d(
                    ui,
                    ("wl_entry", &item.name),
                    false,
                    ACCENT_EMERALD,
                    Margin::symmetric(10, 6),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&item.name).strong());
                            if danger_icon_button(ui, VectorIcon::Trash, &remove_lbl).clicked() {
                                ask_remove = Some(item.name.clone());
                            }
                        });
                        bounded_wrap_label(
                            ui,
                            RichText::new(&item.uuid).monospace().size(11.0).weak(),
                            ui.available_width(),
                        );
                    },
                );
                ui.add_space(4.0);
            }
        },
    );

    if ask_add {
        app.click_sfx();
        app.add_server_whitelist();
    }
    if let Some(name) = ask_remove {
        app.click_sfx();
        app.remove_server_whitelist(&name);
    }
}

fn render_banned_players_card(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let ban_heading = app
        .i18n
        .tr("Danh sách Cấm Tài khoản", "Banned Players")
        .to_string();
    let player_hint = app
        .i18n
        .tr("Tên người chơi...", "Player username...")
        .to_string();
    let reason_hint = app.i18n.tr("Lý do cấm...", "Ban reason...").to_string();
    let ban_lbl = app.i18n.tr("Cấm", "Ban").to_string();
    let pardon_lbl = app.i18n.tr("Ân xá", "Pardon").to_string();
    let empty_ban = app
        .i18n
        .tr("Không có người chơi nào bị cấm.", "No banned players.")
        .to_string();
    let reason_prefix = app.i18n.tr("Lý do", "Reason").to_string();

    let mut ask_ban = false;
    let mut ask_pardon: Option<String> = None;

    gaming_card_3d(
        ui,
        "srv_banned_players_card",
        false,
        ACCENT_RED,
        Margin::same(12),
        |ui| {
            ui.set_width(ui.available_width());
            ui.heading(RichText::new(&ban_heading).size(15.2).strong());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let max_ban_w = (ui.max_rect().width() - 16.0).max(72.0);
                ui.add(
                    egui::TextEdit::singleline(&mut app.servers.new_ban_name)
                        .hint_text(&player_hint)
                        .desired_width(130.0_f32.min(max_ban_w)),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.servers.new_ban_reason)
                        .hint_text(&reason_hint)
                        .desired_width(145.0_f32.min(max_ban_w)),
                );
                if danger_icon_button(ui, VectorIcon::Shield, &ban_lbl).clicked() {
                    ask_ban = true;
                }
            });
            ui.separator();

            let ban_snapshot = app.servers.banlist.clone();
            if ban_snapshot.is_empty() {
                ui.label(RichText::new(&empty_ban).weak());
            }
            for item in ban_snapshot {
                gaming_card_3d(
                    ui,
                    ("ban_entry", &item.name),
                    false,
                    ACCENT_RED,
                    Margin::symmetric(10, 6),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&item.name).strong());
                            if secondary_icon_button(ui, VectorIcon::Refresh, &pardon_lbl).clicked()
                            {
                                ask_pardon = Some(item.name.clone());
                            }
                        });
                        ui.label(
                            RichText::new(format!("{reason_prefix}: {}", item.reason))
                                .size(11.5)
                                .weak(),
                        );
                    },
                );
                ui.add_space(4.0);
            }
        },
    );

    if ask_ban {
        app.click_sfx();
        app.add_server_ban();
    }
    if let Some(name) = ask_pardon {
        app.click_sfx();
        app.remove_server_ban(&name);
    }
}

fn render_banned_ips_card(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let heading = app
        .i18n
        .tr("Danh sách Cấm Địa chỉ IP", "Banned IP Addresses")
        .to_string();
    let ip_hint = app
        .i18n
        .tr("Địa chỉ IPv4 / IPv6...", "IPv4 / IPv6 address...")
        .to_string();
    let reason_hint = app.i18n.tr("Lý do cấm...", "Ban reason...").to_string();
    let ban_lbl = app.i18n.tr("Cấm IP", "Ban IP").to_string();
    let pardon_lbl = app.i18n.tr("Gỡ cấm IP", "Pardon IP").to_string();
    let empty_lbl = app
        .i18n
        .tr("Không có địa chỉ IP nào bị cấm.", "No banned IP addresses.")
        .to_string();
    let reason_prefix = app.i18n.tr("Lý do", "Reason").to_string();

    let mut ask_ban = false;
    let mut ask_pardon: Option<String> = None;

    gaming_card_3d(
        ui,
        "srv_banned_ips_card",
        false,
        ACCENT_VIOLET,
        Margin::same(12),
        |ui| {
            ui.set_width(ui.available_width());
            ui.heading(RichText::new(&heading).size(15.2).strong());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let max_ip_w = (ui.max_rect().width() - 16.0).max(72.0);
                ui.add(
                    egui::TextEdit::singleline(&mut app.servers.new_ban_ip)
                        .hint_text(&ip_hint)
                        .desired_width(140.0_f32.min(max_ip_w)),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.servers.new_ban_ip_reason)
                        .hint_text(&reason_hint)
                        .desired_width(145.0_f32.min(max_ip_w)),
                );
                if danger_icon_button(ui, VectorIcon::Shield, &ban_lbl).clicked() {
                    ask_ban = true;
                }
            });
            ui.separator();

            let ip_snapshot = app.servers.banned_ips.clone();
            if ip_snapshot.is_empty() {
                ui.label(RichText::new(&empty_lbl).weak());
            }
            for item in ip_snapshot {
                gaming_card_3d(
                    ui,
                    ("ban_ip_entry", &item.ip),
                    false,
                    ACCENT_VIOLET,
                    Margin::symmetric(10, 6),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            bounded_wrap_label(
                                ui,
                                RichText::new(&item.ip).monospace().strong(),
                                ui.available_width(),
                            );
                            if secondary_icon_button(ui, VectorIcon::Refresh, &pardon_lbl).clicked()
                            {
                                ask_pardon = Some(item.ip.clone());
                            }
                        });
                        ui.label(
                            RichText::new(format!("{reason_prefix}: {}", item.reason))
                                .size(11.5)
                                .weak(),
                        );
                    },
                );
                ui.add_space(4.0);
            }
        },
    );

    if ask_ban {
        app.click_sfx();
        app.add_server_ban_ip();
    }
    if let Some(ip) = ask_pardon {
        app.click_sfx();
        app.remove_server_ban_ip(&ip);
    }
}

fn render_tab_access_control(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    srv: &ManagedServer,
    unified_scroll: bool,
) {
    maybe_vertical_scroll(ui, "srv_players_tab_scroll", !unified_scroll, false, |ui| {
        render_player_rules_card(app, ui);
        ui.add_space(10.0);
        render_known_players_card(app, ui, srv);
        ui.add_space(10.0);

        if ui.available_width() >= 760.0 {
            ui.columns(2, |cols| {
                render_ops_card(app, &mut cols[0]);
                cols[0].add_space(10.0);
                render_banned_players_card(app, &mut cols[0]);

                render_whitelist_card(app, &mut cols[1]);
                cols[1].add_space(10.0);
                render_banned_ips_card(app, &mut cols[1]);
            });
        } else {
            render_ops_card(app, ui);
            ui.add_space(10.0);
            render_whitelist_card(app, ui);
            ui.add_space(10.0);
            render_banned_players_card(app, ui);
            ui.add_space(10.0);
            render_banned_ips_card(app, ui);
        }
    });
}

fn render_tab_network(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    srv: &ManagedServer,
    unified_scroll: bool,
) {
    if app.servers.ftp_loaded_for_id != srv.id {
        app.sync_selected_server_ftp_config();
    }
    let live_stats = app
        .servers
        .server_stats
        .get(&srv.id)
        .cloned()
        .unwrap_or_default();

    maybe_vertical_scroll(ui, "srv_network_tab_scroll", !unified_scroll, false, |ui| {
        let effective_port = resolve_effective_server_port(srv);
        let local_addr = format!("127.0.0.1:{effective_port}");
        let lan_addr = app
            .servers
            .network_snapshot
            .primary_ipv4
            .map(|ip| ip.to_string())
            .or_else(|| app.lan_modal.primary_lan_ip.clone())
            .map(|ip| format!("{ip}:{effective_port}"));

        let direct_status = app
            .servers
            .server_direct_hosts
            .get(&srv.id)
            .cloned()
            .unwrap_or_else(|| app.server_supervisor.server_direct_host_status(&srv.id));
        let direct_running = matches!(direct_status, DirectHostStatus::Running(_));
        let direct_starting = matches!(direct_status, DirectHostStatus::Starting { .. });

        let mut ask_refresh_net = false;
        let mut ask_toggle_direct = false;
        let mut copy_target: Option<String> = None;

        gaming_card_3d(
            ui,
            "srv_local_lan_card",
            direct_running,
            if direct_running {
                ACCENT_EMERALD
            } else {
                ACCENT_CYAN
            },
            Margin::same(14),
            |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.heading(
                        RichText::new(app.i18n.tr(
                            "Mở Port Theo Mạng Đang Kết Nối (LAN & Online)",
                            "Host Port on Connected Network (LAN & Online)",
                        ))
                        .size(16.0)
                        .strong(),
                    );
                    if app.servers.probing_network {
                        ui.spinner();
                    } else if secondary_icon_button(
                        ui,
                        VectorIcon::Refresh,
                        app.i18n.tr("Làm mới mạng", "Refresh Network"),
                    )
                    .clicked()
                    {
                        ask_refresh_net = true;
                    }
                });
                ui.add_space(4.0);

                ui.horizontal_wrapped(|ui| {
                    ui.label(app.i18n.tr("Địa chỉ trên máy này:", "Loopback Address:"));
                    badge(ui, &local_addr, ACCENT_EMERALD);
                    if secondary_icon_button(ui, VectorIcon::Copy, app.i18n.tr("Sao chép", "Copy"))
                        .clicked()
                    {
                        copy_target = Some(local_addr.clone());
                    }
                    if let Some(ref lan) = lan_addr {
                        ui.label(app.i18n.tr("Mạng nội bộ:", "Local LAN:"));
                        badge(ui, lan, ACCENT_BLUE);
                        if secondary_icon_button(
                            ui,
                            VectorIcon::Copy,
                            app.i18n.tr("Sao chép LAN", "Copy LAN"),
                        )
                        .clicked()
                        {
                            copy_target = Some(lan.clone());
                        }
                    }
                    if let Some(v6) = app.servers.network_snapshot.global_ipv6 {
                        badge(ui, &format!("IPv6 [{v6}]"), ACCENT_CYAN);
                    }
                    if let Some(local_ms) = live_stats.local_ping_ms {
                        badge(
                            ui,
                            &format!(
                                "{}: {local_ms} ms",
                                app.i18n.tr("Ping nội bộ thực", "Live Local Ping")
                            ),
                            ping_badge_color(local_ms, false),
                        );
                    }
                    if let Some((tps_lbl, tps_col)) = tps_badge_info(&live_stats) {
                        badge(ui, &tps_lbl, tps_col);
                    }
                });

                ui.add_space(6.0);
                ui.add_enabled_ui(!direct_running && !direct_starting, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        network_interface_selector_ui(
                            ui,
                            "srv_direct_bind_nic",
                            &app.i18n,
                            &app.servers.network_snapshot,
                            &mut app.servers.direct_bind_ip,
                        );
                        let port_lbl =
                            ui.label(RichText::new(app.i18n.tr("Cổng mở:", "Host Port:")).strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.direct_host_port_input)
                                .hint_text("25565")
                                .desired_width(78.0),
                        )
                        .labelled_by(port_lbl.id);
                        crate::views::wrapped_checkbox(
                            ui,
                            &mut app.servers.direct_announce_lan,
                            app.i18n.tr(
                                "Hiện tự động trong Chơi mạng LAN",
                                "Auto-show in Multiplayer LAN",
                            ),
                        );
                        crate::views::wrapped_checkbox(
                            ui,
                            &mut app.servers.direct_try_upnp,
                            app.i18n
                                .tr("Mở cổng tự động trên Modem", "Auto-open router port"),
                        );
                    });
                });

                ui.add_space(6.0);
                match &direct_status {
                    DirectHostStatus::Idle | DirectHostStatus::Stopped => {
                        if primary_icon_button(
                            ui,
                            VectorIcon::Globe,
                            app.i18n
                                .tr("Mở Port Mạng Hiện Tại", "Open Port on Current Network"),
                        )
                        .clicked()
                        {
                            ask_toggle_direct = true;
                        }
                    }
                    DirectHostStatus::Starting { host_port, .. } => {
                        ui.horizontal_wrapped(|ui| {
                            ui.spinner();
                            ui.label(if app.settings.language == Language::Vi {
                                format!("Đang mở cổng {host_port} trên mạng đang kết nối...")
                            } else {
                                format!("Opening port {host_port} on connected network...")
                            });
                            if danger_icon_button(ui, VectorIcon::Stop, app.i18n.t("common.cancel"))
                                .clicked()
                            {
                                ask_toggle_direct = true;
                            }
                        });
                    }
                    DirectHostStatus::Running(info) => {
                        let host_ping_col = live_stats
                            .public_ping_ms
                            .map(|ms| ping_badge_color(ms, true));
                        if let Some(ep) = render_direct_host_endpoints_ui(
                            ui,
                            &app.i18n,
                            info,
                            live_stats.public_ping_ms,
                            host_ping_col,
                        ) {
                            copy_target = Some(ep);
                        }
                        ui.add_space(4.0);
                        if danger_icon_button(
                            ui,
                            VectorIcon::Stop,
                            app.i18n.tr("Đóng Port Mạng", "Close Hosted Port"),
                        )
                        .clicked()
                        {
                            ask_toggle_direct = true;
                        }
                    }
                    DirectHostStatus::Error(err) => {
                        ui.label(
                            RichText::new(format!(
                                "{}: {err}",
                                app.i18n.tr("Không mở được cổng", "Port Host Error")
                            ))
                            .color(ACCENT_RED),
                        );
                        if primary_icon_button(
                            ui,
                            VectorIcon::Refresh,
                            app.i18n
                                .tr("Mở Port Mạng Hiện Tại", "Open Port on Current Network"),
                        )
                        .clicked()
                        {
                            ask_toggle_direct = true;
                        }
                    }
                }
            },
        );

        if ask_refresh_net {
            app.click_sfx();
            app.refresh_connected_network_snapshot();
        }
        if ask_toggle_direct {
            app.click_sfx();
            app.toggle_selected_server_direct_host();
        }
        if let Some(txt) = copy_target {
            app.copy_text_with_toast(ui.ctx(), txt);
        }

        ui.add_space(10.0);

        let tunnel_status = app
            .servers
            .server_tunnels
            .get(&srv.id)
            .cloned()
            .unwrap_or_else(|| app.server_supervisor.server_tunnel_status(&srv.id));

        let is_running = matches!(tunnel_status, TunnelStatus::Running { .. });
        let is_connecting = matches!(tunnel_status, TunnelStatus::Connecting { .. });
        let remote_lbl = match &tunnel_status {
            st @ TunnelStatus::Running { .. } => st
                .public_endpoint()
                .unwrap_or_else(|| "bore.pub".to_string()),
            TunnelStatus::Connecting { .. } => "bore.pub (...)".to_string(),
            _ => "bore.pub (idle)".to_string(),
        };

        gaming_card_3d(
            ui,
            "srv_bore_tunnel_card",
            is_running,
            ACCENT_EMERALD,
            Margin::same(14),
            |ui| {
                ui.set_width(ui.available_width());
                ui.heading(
                    RichText::new(app.i18n.tr(
                        "Đường hầm Công khai Dự phòng (bore.pub)",
                        "Fallback Public Tunnel (bore.pub)",
                    ))
                    .size(16.0)
                    .strong(),
                );
                ui.add_space(8.0);
                paint_3d_tunnel_conduit(ui, is_running, is_connecting, &local_addr, &remote_lbl);
                ui.add_space(8.0);

                match tunnel_status {
                    TunnelStatus::Idle | TunnelStatus::Stopped => {
                        if primary_icon_button(
                            ui,
                            VectorIcon::Globe,
                            app.i18n.tr(
                                "Mở cổng công khai cho Máy chủ này",
                                "Open Public Tunnel for This Server",
                            ),
                        )
                        .clicked()
                        {
                            app.toggle_selected_server_tunnel();
                        }
                    }
                    TunnelStatus::Connecting { local_port } => {
                        ui.horizontal_wrapped(|ui| {
                                ui.spinner();
                                ui.label(if app.settings.language == Language::Vi {
                                    format!(
                                        "Đang thiết lập kết nối từ cổng nội bộ {local_port} tới bore.pub..."
                                    )
                                } else {
                                    format!("Connecting local port {local_port} to bore.pub...")
                                });
                                if danger_icon_button(
                                    ui,
                                    VectorIcon::Stop,
                                    app.i18n.t("common.cancel"),
                                )
                                .clicked()
                                {
                                    app.toggle_selected_server_tunnel();
                                }
                            });
                    }
                    ref st @ TunnelStatus::Running { local_port, .. } => {
                        let public_endpoint = st
                            .public_endpoint()
                            .unwrap_or_else(|| "bore.pub".to_string());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(
                                    app.i18n.tr("Địa chỉ công khai:", "Public Endpoint:"),
                                )
                                .strong(),
                            );
                            badge(ui, &public_endpoint, ACCENT_EMERALD);
                            if let Some(pub_ms) = live_stats.public_ping_ms {
                                badge(
                                    ui,
                                    &format!(
                                        "{}: {pub_ms} ms",
                                        app.i18n.tr("Ping công khai", "Public Ping")
                                    ),
                                    ping_badge_color(pub_ms, true),
                                );
                            }
                            if primary_icon_button(
                                ui,
                                VectorIcon::Copy,
                                app.i18n.tr("Sao chép địa chỉ", "Copy Address"),
                            )
                            .clicked()
                            {
                                app.copy_text_with_toast(ui.ctx(), public_endpoint.clone());
                            }
                            if danger_icon_button(
                                ui,
                                VectorIcon::Stop,
                                app.i18n.tr("Đóng cổng", "Close Tunnel"),
                            )
                            .clicked()
                            {
                                app.toggle_selected_server_tunnel();
                            }
                        });
                        ui.label(
                                RichText::new(if app.settings.language == Language::Vi {
                                    format!(
                                        "Đang chuyển tiếp từ {public_endpoint} → 127.0.0.1:{local_port}"
                                    )
                                } else {
                                    format!(
                                        "Forwarding from {public_endpoint} → 127.0.0.1:{local_port}"
                                    )
                                })
                                .size(11.5)
                                .weak(),
                            );
                    }
                    TunnelStatus::Error(err) => {
                        ui.label(
                            RichText::new(format!(
                                "{}: {err}",
                                app.i18n.tr("Lỗi kết nối", "Tunnel Error")
                            ))
                            .color(ACCENT_RED),
                        );
                        if primary_icon_button(
                            ui,
                            VectorIcon::Refresh,
                            app.i18n.tr("Thử kết nối lại", "Reconnect"),
                        )
                        .clicked()
                        {
                            app.toggle_selected_server_tunnel();
                        }
                    }
                }
            },
        );

        ui.add_space(10.0);

        let ftp_busy = app.servers.ftp_testing || app.servers.ftp_deploying;
        let ftp_task_prog = app
            .active_tasks
            .get(&format!("ftp-deploy:{}", srv.id))
            .map(|t| t.progress.clone());

        let mut ask_save_ftp = false;
        let mut ask_test_ftp = false;
        let mut ask_deploy_ftp = false;
        let mut ask_cancel_ftp = false;

        gaming_card_3d(
            ui,
            "srv_ftp_hosting_deploy_card",
            app.servers.ftp_deploying,
            ACCENT_VIOLET,
            Margin::same(14),
            |ui| {
                ui.set_width(ui.available_width());
                ui.heading(
                    RichText::new(app.i18n.tr(
                        "Triển khai lên Hosting (FTP / FTPS)",
                        "Deploy to Hosting (FTP / FTPS)",
                    ))
                    .size(16.0)
                    .strong(),
                );
                ui.add_space(8.0);

                ui.add_enabled_ui(!ftp_busy, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let cap_w = (ui.max_rect().width() - 16.0).max(72.0);
                        let host_lbl = ui.label(RichText::new("Host:").strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.ftp_config.host)
                                .hint_text("ftp.example-hosting.net")
                                .desired_width(240.0_f32.min(cap_w)),
                        )
                        .labelled_by(host_lbl.id);
                        let port_lbl = ui.label(RichText::new("Port:").strong());
                        ui.add(
                            egui::DragValue::new(&mut app.servers.ftp_config.port).range(1..=65535),
                        )
                        .labelled_by(port_lbl.id);
                    });

                    ui.horizontal_wrapped(|ui| {
                        let cap_w = (ui.max_rect().width() - 16.0).max(72.0);
                        let user_lbl = ui
                            .label(RichText::new(app.i18n.tr("Tài khoản:", "Username:")).strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.ftp_config.username)
                                .hint_text("srv_12345")
                                .desired_width(180.0_f32.min(cap_w)),
                        )
                        .labelled_by(user_lbl.id);
                        let pass_lbl =
                            ui.label(RichText::new(app.i18n.tr("Mật khẩu:", "Password:")).strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.ftp_password)
                                .password(true)
                                .desired_width(180.0_f32.min(cap_w)),
                        )
                        .labelled_by(pass_lbl.id);
                    });

                    ui.horizontal_wrapped(|ui| {
                        let cap_w = (ui.max_rect().width() - 16.0).max(72.0);
                        let dir_lbl = ui.label(
                            RichText::new(app.i18n.tr("Thư mục trên Hosting:", "Remote Folder:"))
                                .strong(),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.ftp_config.remote_dir)
                                .hint_text("/")
                                .desired_width(220.0_f32.min(cap_w)),
                        )
                        .labelled_by(dir_lbl.id);
                        crate::views::wrapped_checkbox(
                            ui,
                            &mut app.servers.ftp_config.use_ftps,
                            app.i18n.tr("Bảo mật FTPS (TLS)", "Explicit FTPS (TLS)"),
                        );
                        crate::views::wrapped_checkbox(
                            ui,
                            &mut app.servers.ftp_config.include_world,
                            app.i18n.tr("Bao gồm thư mục World", "Include World Folder"),
                        );
                    });
                });

                if let Some(prog) = ftp_task_prog {
                    ui.add_space(8.0);
                    let frac = (prog.percent / 100.0).clamp(0.0, 1.0);
                    paint_glowing_progress_bar(ui, frac, ui.available_width(), 16.0);
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        badge(
                            ui,
                            &format!("{}/{}", prog.completed_files, prog.total_files),
                            ACCENT_CYAN,
                        );
                        ui.label(RichText::new(&prog.message).size(11.6));
                        if prog.bytes_per_sec > 0 {
                            ui.label(
                                RichText::new(format!("({}/s)", format_bytes(prog.bytes_per_sec)))
                                    .size(11.2)
                                    .weak(),
                            );
                        }
                    });
                } else if app.servers.ftp_testing {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spinner();
                        ui.label(app.i18n.tr(
                            "Đang kiểm tra đăng nhập FTP/FTPS...",
                            "Verifying FTP/FTPS connection...",
                        ));
                    });
                }

                if let Some((ok, ref msg)) = app.servers.ftp_last_status {
                    ui.add_space(6.0);
                    ui.label(RichText::new(msg).size(12.0).color(if ok {
                        ACCENT_EMERALD
                    } else {
                        ACCENT_RED
                    }));
                }

                ui.add_space(8.0);
                if app.servers.ftp_deploying {
                    ui.horizontal_wrapped(|ui| {
                        if danger_icon_button(
                            ui,
                            VectorIcon::Stop,
                            app.i18n.tr("Hủy tải lên", "Cancel Upload"),
                        )
                        .clicked()
                        {
                            ask_cancel_ftp = true;
                        }
                    });
                } else {
                    ui.add_enabled_ui(!app.servers.ftp_testing, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if primary_icon_button(
                                ui,
                                VectorIcon::Upload,
                                app.i18n.tr("Đẩy dữ liệu lên Hosting", "Deploy to Hosting"),
                            )
                            .clicked()
                            {
                                ask_deploy_ftp = true;
                            }
                            if secondary_icon_button(
                                ui,
                                VectorIcon::Refresh,
                                app.i18n.tr("Kiểm tra kết nối", "Test Connection"),
                            )
                            .clicked()
                            {
                                ask_test_ftp = true;
                            }
                            if secondary_icon_button(
                                ui,
                                VectorIcon::Save,
                                app.i18n.tr("Lưu cấu hình", "Save Config"),
                            )
                            .clicked()
                            {
                                ask_save_ftp = true;
                            }
                        });
                    });
                }
            },
        );

        if ask_save_ftp {
            app.click_sfx();
            app.save_selected_server_ftp_config(true);
        } else if ask_test_ftp {
            app.click_sfx();
            app.test_selected_server_ftp();
        } else if ask_deploy_ftp {
            app.click_sfx();
            app.deploy_selected_server_ftp();
        } else if ask_cancel_ftp {
            app.click_sfx();
            app.cancel_selected_server_ftp_deploy();
        }
    });
}
