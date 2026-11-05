use crate::fx3d::{
    gaming_card_3d, paint_3d_tunnel_conduit, paint_glowing_progress_bar, paint_icon_3d,
    sliding_pill_tabs, tactile_button_3d, ButtonStyle3d, Icon3dKind, VectorIcon,
};
use crate::state::{LauncherAppState, ServerTab};
use crate::theme::{
    ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_RED, ACCENT_VIOLET,
};
use crate::views::{
    badge, danger_icon_button, format_bytes, format_duration_hms, primary_icon_button,
    secondary_button, secondary_icon_button,
};
use egui::{
    pos2, vec2, Color32, CornerRadius, Frame, Margin, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind,
};
use std::collections::{BTreeMap, HashSet};
use thk_core::models::{ContentKind, Language, ManagedServer, ProviderSource};
use thk_core::I18n;
use thk_network::TunnelStatus;
use thk_server::ServerRuntimeState;

const MC_SERVER_AUTOCOMPLETE_CMDS: &[&str] = &[
    "help",
    "list",
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
    "gamemode spectator ",
    "tp ",
    "give ",
    "time set day",
    "time set night",
    "weather clear",
    "ban ",
    "pardon ",
    "kick ",
    "difficulty normal",
    "gamerule keepInventory true",
];

fn paint_addon_thumbnail(
    ui: &mut egui::Ui,
    tex_opt: Option<&egui::TextureHandle>,
    size: f32,
    accent: Color32,
) {
    if let Some(tex) = tex_opt {
        let radius = CornerRadius::same(8);
        let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
        ui.painter()
            .rect_filled(rect, radius, Color32::from_rgb(14, 19, 32));

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
    egui::Panel::left("thk_servers_sidebar")
        .resizable(true)
        .default_size(262.0)
        .size_range(215.0..=350.0)
        .frame(Frame::NONE.inner_margin(Margin {
            left: 0,
            right: 12,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| {
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

            ScrollArea::vertical()
                .id_salt("servers_sidebar_scroll")
                .show(ui, |ui| {
                    if servers_list.is_empty() {
                        ui.label(
                            RichText::new(app.i18n.tr(
                                "Chưa có máy chủ cục bộ nào. Nhấn 'Tạo Máy chủ mới' để bắt đầu.",
                                "No local servers yet. Click 'Create Server' to get started.",
                            ))
                            .weak(),
                        );
                    }

                    for srv in servers_list {
                        let is_sel = sel_id.as_deref() == Some(&srv.id);
                        let rt_state = app
                            .servers
                            .server_states
                            .get(&srv.id)
                            .copied()
                            .unwrap_or_else(|| app.server_supervisor.runtime_state(&srv.id));
                        let (st_label, st_color) = runtime_state_badge_info(rt_state, &app.i18n);
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
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&srv.name).strong().size(13.6));
                                    badge(ui, st_label, st_color);
                                });
                                ui.add_space(2.0);
                                ui.horizontal_wrapped(|ui| {
                                    badge(ui, srv.engine.display_name(), ACCENT_BLUE);
                                    badge(ui, &srv.game_version, ACCENT_AMBER);
                                    ui.label(
                                        RichText::new(format!(":{}", srv.port)).size(11.0).weak(),
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

    let Some(srv) = app.selected_server().cloned() else {
        ui.vertical_centered(|ui| {
            ui.add_space(56.0);
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
            ui.label(
                RichText::new(app.i18n.tr(
                    "Hỗ trợ 10 lõi: Paper, Purpur, Folia, Vanilla, Fabric, Forge, NeoForge, Arclight, Mohist, SpongeVanilla.",
                    "Supports 10 engines: Paper, Purpur, Folia, Vanilla, Fabric, Forge, NeoForge, Arclight, Mohist, SpongeVanilla.",
                ))
                .weak(),
            );
            ui.add_space(14.0);
            if primary_icon_button(ui, VectorIcon::Plus, app.i18n.t("server.create")).clicked() {
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

    match app.servers.active_tab {
        ServerTab::Console => render_tab_console(app, ui, &srv),
        ServerTab::Files => render_tab_files(app, ui, &srv),
        ServerTab::PluginsMods => render_tab_plugins_mods(app, ui, &srv),
        ServerTab::Properties => render_tab_properties(app, ui, &srv),
        ServerTab::AccessControl => render_tab_access_control(app, ui),
        ServerTab::Network => render_tab_network(app, ui, &srv),
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

fn render_server_header(app: &mut LauncherAppState, ui: &mut egui::Ui, srv: &ManagedServer) {
    let rt_state = app
        .servers
        .server_states
        .get(&srv.id)
        .copied()
        .unwrap_or_else(|| app.server_supervisor.runtime_state(&srv.id));
    let (st_label, st_color) = runtime_state_badge_info(rt_state, &app.i18n);
    let stats = app
        .servers
        .server_stats
        .get(&srv.id)
        .cloned()
        .unwrap_or_default();

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
            ui.horizontal(|ui| {
                paint_icon_3d(
                    ui,
                    40.0,
                    Icon3dKind::ServerBladeTower,
                    rt_state == ServerRuntimeState::Online,
                );
                ui.add_space(4.0);

                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.heading(RichText::new(&srv.name).size(20.0).strong());
                        badge(ui, st_label, st_color);
                        badge(ui, srv.engine.display_name(), ACCENT_BLUE);
                        badge(ui, &format!("MC {}", srv.game_version), ACCENT_EMERALD);
                        badge(ui, &format!("Port {}", srv.port), ACCENT_AMBER);
                    });

                    ui.horizontal_wrapped(|ui| {
                        let jar_info = if srv.jar_file.is_empty() {
                            app.i18n.tr("Chưa tải lõi JAR", "JAR not installed")
                        } else {
                            srv.jar_file.as_str()
                        };
                        ui.label(
                            RichText::new(format!(
                                "{}: {jar_info}",
                                app.i18n.tr("Lõi", "Core")
                            ))
                            .size(12.0)
                            .weak(),
                        );
                        if rt_state != ServerRuntimeState::Offline {
                            ui.separator();
                            let cpu_frac = (stats.cpu_percent / 100.0).clamp(0.0, 1.0);
                            let ram_frac =
                                (stats.ram_mb as f32 / srv.ram_max_mb.max(1) as f32).clamp(0.0, 1.0);
                            ui.label(RichText::new("CPU").size(11.2).strong());
                            paint_glowing_progress_bar(ui, cpu_frac, 68.0, 14.0);
                            ui.label(
                                RichText::new(format!("RAM {}/{}MB", stats.ram_mb, srv.ram_max_mb))
                                    .size(11.2)
                                    .strong(),
                            );
                            paint_glowing_progress_bar(ui, ram_frac, 76.0, 14.0);
                            badge(
                                ui,
                                &format!(
                                    "{}/{} {} • {}",
                                    stats.online_players,
                                    stats.max_players.max(20),
                                    app.i18n.tr("người chơi", "players"),
                                    format_duration_hms(stats.uptime_secs)
                                ),
                                ACCENT_EMERALD,
                            );
                        }
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if rt_state == ServerRuntimeState::Offline
                        && danger_icon_button(
                            ui,
                            VectorIcon::Trash,
                            app.i18n.tr("Xóa", "Delete"),
                        )
                        .clicked()
                    {
                        app.delete_selected_server();
                        return;
                    }

                    if secondary_icon_button(
                        ui,
                        VectorIcon::Folder,
                        app.i18n.tr("Thư mục", "Folder"),
                    )
                    .clicked()
                    {
                        let _ = std::fs::create_dir_all(&srv.server_dir);
                        let _ = open::that(&srv.server_dir);
                    }

                    if rt_state == ServerRuntimeState::Offline
                        && secondary_icon_button(
                            ui,
                            VectorIcon::Download,
                            app.i18n.tr("Tải / Cài lại Lõi", "Install / Repair JAR"),
                        )
                        .clicked()
                    {
                        app.install_selected_server_jar();
                    }

                    match rt_state {
                        ServerRuntimeState::Offline => {
                            if primary_icon_button(
                                ui,
                                VectorIcon::Play,
                                app.i18n.t("server.start"),
                            )
                            .clicked()
                            {
                                app.start_selected_server();
                            }
                        }
                        ServerRuntimeState::Online => {
                            if danger_icon_button(ui, VectorIcon::Stop, app.i18n.t("server.stop"))
                                .clicked()
                            {
                                app.stop_selected_server();
                            }
                            if secondary_icon_button(
                                ui,
                                VectorIcon::Refresh,
                                app.i18n.t("server.restart"),
                            )
                            .clicked()
                            {
                                app.restart_selected_server();
                            }
                        }
                        ServerRuntimeState::Starting | ServerRuntimeState::Stopping => {
                            if danger_icon_button(
                                ui,
                                VectorIcon::Stop,
                                app.i18n.tr("Buộc dừng", "Force Stop"),
                            )
                            .clicked()
                            {
                                app.stop_selected_server();
                            }
                        }
                    }
                });
            });
        },
    );
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
                .tr("Whitelist & Banlist", "Access Control")
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
    }
}

fn render_tab_console(app: &mut LauncherAppState, ui: &mut egui::Ui, srv: &ManagedServer) {
    let lines = app
        .servers
        .server_logs
        .get(&srv.id)
        .cloned()
        .unwrap_or_default();

    let console_h = (ui.available_height() - 52.0).max(160.0);
    Frame::group(ui.style())
        .fill(Color32::from_rgb(7, 10, 18))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
            ScrollArea::vertical()
                .id_salt("server_console_scroll")
                .max_height(console_h)
                .stick_to_bottom(true)
                .show_rows(ui, row_h, lines.len().max(1), |ui, range| {
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
                                ACCENT_RED
                            } else if line.contains("WARN") {
                                ACCENT_AMBER
                            } else if line.starts_with("> ") {
                                ACCENT_EMERALD
                            } else {
                                Color32::from_rgb(214, 224, 235)
                            };
                            ui.label(RichText::new(line).monospace().size(12.0).color(col));
                        }
                    }
                });
        });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let input_id = egui::Id::unique("server_console_cmd_input");
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

        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.servers.console_input)
                .id(input_id)
                .hint_text(app.i18n.tr(
                    "Nhập lệnh máy chủ (↑/↓ lịch sử, Tab gợi ý lệnh, Enter gửi)...",
                    "Enter server command (↑/↓ history, Tab autocomplete, Enter send)...",
                ))
                .desired_width((ui.available_width() - 215.0).max(160.0)),
        );

        let enter_send = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter_send {
            app.send_selected_server_command();
            resp.request_focus();
        }

        if primary_icon_button(
            ui,
            VectorIcon::Play,
            app.i18n.tr("Gửi lệnh", "Send"),
        )
        .clicked()
        {
            app.send_selected_server_command();
        }

        if secondary_icon_button(
            ui,
            VectorIcon::Trash,
            app.i18n.tr("Xoá màn hình", "Clear"),
        )
        .clicked()
        {
            app.servers.server_logs.remove(&srv.id);
        }
    });
}

fn render_tab_files(app: &mut LauncherAppState, ui: &mut egui::Ui, srv: &ManagedServer) {
    ui.horizontal_wrapped(|ui| {
        if secondary_icon_button(
            ui,
            VectorIcon::Folder,
            app.i18n.tr("Gốc (/)", "Root (/)"),
        )
        .clicked()
        {
            app.servers.current_rel_dir.clear();
            app.servers.selected_entries.clear();
            app.refresh_selected_server_tabs();
        }
        if !app.servers.current_rel_dir.is_empty()
            && secondary_icon_button(
                ui,
                VectorIcon::ArrowLeft,
                app.i18n.tr("Lên cấp trên", "Up"),
            )
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
            let _ = open::that(&target);
        }

        ui.separator();
        ui.add(
            egui::TextEdit::singleline(&mut app.servers.archive_name_input)
                .hint_text("backup.zip")
                .desired_width(120.0),
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
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}: {editing_rel}",
                            app.i18n.tr("Đang chỉnh sửa", "Editing")
                        ))
                        .strong()
                        .color(ACCENT_EMERALD),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if secondary_button(ui, app.i18n.tr("Đóng trình sửa", "Close")).clicked() {
                            app.servers.editing_file_rel = None;
                        }
                        if primary_icon_button(
                            ui,
                            VectorIcon::Save,
                            app.i18n.tr("Lưu tệp", "Save File"),
                        )
                        .clicked()
                        {
                            app.save_server_file_editor();
                        }
                    });
                });
                ui.add_space(6.0);
                ScrollArea::vertical()
                    .id_salt("server_file_editor_scroll")
                    .max_height(240.0)
                    .show(ui, |ui| {
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
    ScrollArea::vertical()
        .id_salt("server_files_list_scroll")
        .show(ui, |ui| {
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
                    if entry.is_dir { ACCENT_CYAN } else { ACCENT_BLUE },
                    Margin::symmetric(10, 6),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
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
                                if ui.link(RichText::new(&entry.name).strong()).clicked() {
                                    app.servers.current_rel_dir = entry.rel_path.clone();
                                    app.servers.selected_entries.clear();
                                    app.refresh_selected_server_tabs();
                                }
                            } else {
                                ui.label(RichText::new(&entry.name));
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
                    },
                );
                ui.add_space(4.0);
            }
        });
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

fn render_tab_plugins_mods(app: &mut LauncherAppState, ui: &mut egui::Ui, srv: &ManagedServer) {
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
                ui.add(
                    egui::TextEdit::singleline(&mut app.servers.addon_filter)
                        .hint_text(app.i18n.tr("Lọc gói đã cài...", "Filter installed..."))
                        .desired_width(180.0),
                );
                if secondary_icon_button(
                    ui,
                    VectorIcon::Refresh,
                    app.i18n.tr("Làm mới", "Refresh"),
                )
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
            let filter_low = app.servers.addon_filter.trim().to_ascii_lowercase();
            let installed = app.servers.installed_addons.clone();

            ScrollArea::vertical()
                .id_salt("srv_installed_addons_scroll")
                .max_height(168.0)
                .show(ui, |ui| {
                    if installed.is_empty() {
                        ui.label(
                            RichText::new(app.i18n.tr(
                                "Chưa có tệp .jar nào trong thư mục plugins/ hoặc mods/.",
                                "No .jar files installed in plugins/ or mods/ yet.",
                            ))
                            .weak(),
                        );
                        return;
                    }
                    for addon in installed {
                        if !filter_low.is_empty()
                            && !addon.title.to_ascii_lowercase().contains(&filter_low)
                            && !addon.file_name.to_ascii_lowercase().contains(&filter_low)
                        {
                            continue;
                        }
                        let folder_col = if addon.folder == "plugins" {
                            ACCENT_VIOLET
                        } else {
                            ACCENT_CYAN
                        };
                        ui.horizontal(|ui| {
                            let mut enabled = addon.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                app.toggle_server_installed_addon(
                                    &addon.folder,
                                    &addon.file_name,
                                    enabled,
                                );
                            }
                            badge(ui, &format!("{}/", addon.folder), folder_col);
                            ui.label(RichText::new(&addon.title).strong());
                            if !addon.version_number.is_empty() {
                                badge(ui, &addon.version_number, ACCENT_BLUE);
                            }
                            ui.label(
                                RichText::new(format!(
                                    "{} ({})",
                                    addon.file_name,
                                    format_bytes(addon.size_bytes)
                                ))
                                .size(11.2)
                                .weak(),
                            );
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
                                        app.delete_server_installed_addon(
                                            &addon.folder,
                                            &addon.file_name,
                                        );
                                    }
                                },
                            );
                        });
                    }
                });
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
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.servers.addon_query)
                .hint_text(app.i18n.tr(
                    "Tìm kiếm EssentialsX, LuckPerms, WorldEdit, Geyser, Lithium...",
                    "Search EssentialsX, LuckPerms, WorldEdit, Geyser, Lithium...",
                ))
                .desired_width(340.0),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            app.search_server_addons();
        }
        if primary_icon_button(
            ui,
            VectorIcon::Search,
            app.i18n.tr("Tìm kiếm", "Search"),
        )
        .clicked()
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
                    ui.vertical(|ui| {
                        ui.label(
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
                        );
                        ui.label(RichText::new(&summary.summary).size(11.5).weak());
                    });
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
                let combo_txt = format!(
                    "[{}] {} — {}",
                    cur_ver.release_channel.to_uppercase(),
                    cur_ver.version_number,
                    cur_ver.name
                );
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt("srv_addon_ver_combo")
                        .width(340.0)
                        .selected_text(combo_txt)
                        .show_ui(ui, |ui| {
                            for (idx, v) in versions.iter().enumerate() {
                                let lbl = format!(
                                    "[{}] {} — {}",
                                    v.release_channel.to_uppercase(),
                                    v.version_number,
                                    v.name
                                );
                                ui.selectable_value(
                                    &mut app.servers.addon_chosen_ver_idx,
                                    idx,
                                    lbl,
                                );
                            }
                        });
                    if primary_icon_button(
                        ui,
                        VectorIcon::Download,
                        app.i18n.t("mods.install"),
                    )
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
    ScrollArea::vertical()
        .id_salt("server_addons_hits_scroll")
        .show(ui, |ui| {
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
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            paint_addon_thumbnail(ui, hit_tex.as_ref(), 36.0, ACCENT_EMERALD);
                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(&hit.title).size(14.5).strong());
                                    badge(ui, hit.provider.display_name(), ACCENT_BLUE);
                                    ui.label(
                                        RichText::new(format!("by {}", hit.author))
                                            .size(11.5)
                                            .weak(),
                                    );
                                    if is_installed {
                                        badge(ui, app.i18n.t("mods.installed"), ACCENT_EMERALD);
                                    }
                                });
                                ui.label(RichText::new(&hit.summary).size(12.0));
                            });

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
                    },
                );
                ui.add_space(5.0);
            }

            if hits.len() < app.servers.addon_total_hits {
                ui.add_space(4.0);
                let more_lbl = if app.settings.language == Language::Vi {
                    format!(
                        "Tải thêm ({}/{})",
                        hits.len(),
                        app.servers.addon_total_hits
                    )
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
        });
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
    if ui.checkbox(&mut val, label).changed() {
        props.insert(key.to_string(), if val { "true" } else { "false" }.into());
    }
}

fn prop_combo(
    ui: &mut egui::Ui,
    props: &mut BTreeMap<String, String>,
    key: &str,
    default_val: &str,
    options: &[&str],
) {
    let current = props
        .get(key)
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_val.to_string());
    egui::ComboBox::from_id_salt(("srv_prop_combo", key))
        .width(135.0)
        .selected_text(&current)
        .show_ui(ui, |ui| {
            for &opt in options {
                if ui.selectable_label(current == opt, opt).clicked() {
                    props.insert(key.to_string(), opt.to_string());
                }
            }
        });
}

fn prop_u32_drag(
    ui: &mut egui::Ui,
    props: &mut BTreeMap<String, String>,
    key: &str,
    default_val: u32,
    range: std::ops::RangeInclusive<u32>,
) {
    let mut val = props
        .get(key)
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(default_val);
    if ui.add(egui::DragValue::new(&mut val).range(range)).changed() {
        props.insert(key.to_string(), val.to_string());
    }
}

fn render_tab_properties(app: &mut LauncherAppState, ui: &mut egui::Ui, srv: &ManagedServer) {
    if app.servers.edit_draft.loaded_for_id != srv.id {
        app.sync_server_edit_draft();
    }

    ScrollArea::vertical()
        .id_salt("server_properties_outer_scroll")
        .show(ui, |ui| {
            gaming_card_3d(
                ui,
                "srv_hw_jvm_card",
                false,
                ACCENT_CYAN,
                Margin::same(14),
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Cấu hình Phần cứng, Cổng & Tham số JVM Máy chủ",
                            "Server Hardware, Port & JVM Arguments",
                        ))
                        .size(14.5)
                        .strong(),
                    );
                    ui.add_space(6.0);

                    egui::Grid::new("server_hw_edit_grid")
                        .num_columns(2)
                        .spacing([14.0, 9.0])
                        .show(ui, |ui| {
                            ui.label(app.i18n.tr("Tên máy chủ:", "Server Name:"));
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.servers.edit_draft.name)
                                        .desired_width(240.0),
                                );
                                ui.label("Port:");
                                ui.add(
                                    egui::DragValue::new(&mut app.servers.edit_draft.port)
                                        .range(1024..=65535),
                                );
                            });
                            ui.end_row();

                            ui.label(app.i18n.tr("RAM tối thiểu / tối đa:", "Min / Max RAM:"));
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Slider::new(
                                        &mut app.servers.edit_draft.ram_min_mb,
                                        512..=16384,
                                    )
                                    .step_by(256.0)
                                    .suffix(" MB"),
                                );
                                ui.add(
                                    egui::Slider::new(
                                        &mut app.servers.edit_draft.ram_max_mb,
                                        1024..=32768,
                                    )
                                    .step_by(512.0)
                                    .suffix(" MB"),
                                );
                            });
                            ui.end_row();

                            ui.label(app.i18n.t("profile.java_path"));
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut app.servers.edit_draft.java_path_str,
                                    )
                                    .hint_text(app.i18n.tr(
                                        "Mặc định tự động chọn Java phù hợp",
                                        "Auto-select compatible Java runtime",
                                    ))
                                    .desired_width(260.0),
                                );
                                if secondary_icon_button(
                                    ui,
                                    VectorIcon::Folder,
                                    app.i18n.tr("Chọn...", "Browse..."),
                                )
                                .clicked()
                                {
                                    if let Some(f) = rfd::FileDialog::new().pick_file() {
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
                            ui.end_row();

                            ui.label(app.i18n.t("profile.jvm_args"));
                            ui.add(
                                egui::TextEdit::singleline(&mut app.servers.edit_draft.jvm_flags)
                                    .hint_text("-XX:+UseG1GC -XX:+ParallelRefProcEnabled")
                                    .desired_width(360.0),
                            );
                            ui.end_row();
                        });

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
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(app.i18n.tr(
                                "Điều khiển Trực quan Nhanh (server.properties)",
                                "Visual Quick-Controls (server.properties)",
                            ))
                            .size(14.5)
                            .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                    });
                    ui.add_space(6.0);

                    let lbl_online = app
                        .i18n
                        .tr("Xác thực Mojang (online-mode)", "Mojang Auth (online-mode)")
                        .to_string();
                    let lbl_wl = app
                        .i18n
                        .tr("Bật Whitelist", "Enable Whitelist")
                        .to_string();
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

                    egui::Grid::new("srv_quick_props_grid")
                        .num_columns(4)
                        .spacing([14.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(&lbl_gm);
                            prop_combo(
                                ui,
                                &mut app.servers.properties_map,
                                "gamemode",
                                "survival",
                                &["survival", "creative", "adventure", "spectator"],
                            );
                            ui.label(&lbl_diff);
                            prop_combo(
                                ui,
                                &mut app.servers.properties_map,
                                "difficulty",
                                "normal",
                                &["peaceful", "easy", "normal", "hard"],
                            );
                            ui.end_row();

                            ui.label(&lbl_maxp);
                            prop_u32_drag(
                                ui,
                                &mut app.servers.properties_map,
                                "max-players",
                                20,
                                1..=1000,
                            );
                            ui.label(&lbl_view);
                            ui.horizontal(|ui| {
                                prop_u32_drag(
                                    ui,
                                    &mut app.servers.properties_map,
                                    "view-distance",
                                    10,
                                    2..=32,
                                );
                                ui.label("/");
                                prop_u32_drag(
                                    ui,
                                    &mut app.servers.properties_map,
                                    "simulation-distance",
                                    10,
                                    2..=32,
                                );
                            });
                            ui.end_row();

                            ui.label(&lbl_spawn);
                            prop_u32_drag(
                                ui,
                                &mut app.servers.properties_map,
                                "spawn-protection",
                                16,
                                0..=256,
                            );
                            ui.label("MOTD:");
                            let motd_entry = app
                                .servers
                                .properties_map
                                .entry("motd".to_string())
                                .or_insert_with(|| format!("{} - TriHienKun Laucher", srv.name));
                            ui.add(egui::TextEdit::singleline(motd_entry).desired_width(240.0));
                            ui.end_row();

                            ui.label(&lbl_world);
                            let lvl_entry = app
                                .servers
                                .properties_map
                                .entry("level-name".to_string())
                                .or_insert_with(|| "world".to_string());
                            ui.add(egui::TextEdit::singleline(lvl_entry).desired_width(135.0));
                            ui.label("Seed:");
                            let seed_entry = app
                                .servers
                                .properties_map
                                .entry("level-seed".to_string())
                                .or_default();
                            ui.add(
                                egui::TextEdit::singleline(seed_entry)
                                    .hint_text("optional")
                                    .desired_width(240.0),
                            );
                            ui.end_row();
                        });
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
                    ui.set_width(ui.available_width());
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
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.properties_filter)
                                .hint_text(app.i18n.tr("Lọc tên khoá...", "Filter keys..."))
                                .desired_width(160.0),
                        );
                        ui.separator();
                        ui.label(RichText::new(app.i18n.tr("Thêm khoá:", "Add key:")).strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.new_prop_key)
                                .hint_text("online-mode")
                                .desired_width(145.0),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.new_prop_val)
                                .hint_text("false")
                                .desired_width(145.0),
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

                    egui::Grid::new("server_properties_grid")
                        .num_columns(2)
                        .spacing([16.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for key in keys {
                                ui.label(RichText::new(&key).monospace().size(12.5));
                                if let Some(val) = app.servers.properties_map.get_mut(&key) {
                                    ui.add(egui::TextEdit::singleline(val).desired_width(320.0));
                                }
                                ui.end_row();
                            }
                        });
                },
            );
        });
}

fn render_tab_access_control(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let wl_heading = app
        .i18n
        .tr("Danh sách Trắng (Whitelist)", "Whitelist")
        .to_string();
    let ban_heading = app
        .i18n
        .tr("Danh sách Cấm (Banned Players)", "Banned Players")
        .to_string();
    let player_hint = app
        .i18n
        .tr("Tên người chơi...", "Player username...")
        .to_string();
    let reason_hint = app
        .i18n
        .tr("Lý do cấm...", "Ban reason...")
        .to_string();
    let add_lbl = app.i18n.tr("Thêm", "Add").to_string();
    let remove_lbl = app.i18n.tr("Gỡ", "Remove").to_string();
    let ban_lbl = app.i18n.tr("Cấm", "Ban").to_string();
    let pardon_lbl = app.i18n.tr("Ân xá (Pardon)", "Pardon").to_string();
    let empty_wl = app
        .i18n
        .tr(
            "Chưa có người chơi nào trong Whitelist.",
            "No players in whitelist.",
        )
        .to_string();
    let empty_ban = app
        .i18n
        .tr("Không có người chơi nào bị cấm.", "No banned players.")
        .to_string();
    let reason_prefix = app.i18n.tr("Lý do", "Reason").to_string();

    ui.columns(2, |cols| {
        cols[0].heading(RichText::new(&wl_heading).size(15.5).strong());
        cols[0].add_space(4.0);
        cols[0].horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.servers.new_wl_name)
                    .hint_text(&player_hint)
                    .desired_width(170.0),
            );
            if primary_icon_button(ui, VectorIcon::Plus, &add_lbl).clicked() {
                app.add_server_whitelist();
            }
        });
        cols[0].separator();

        let wl_snapshot = app.servers.whitelist.clone();
        ScrollArea::vertical()
            .id_salt("server_whitelist_scroll")
            .show(&mut cols[0], |ui| {
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
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&item.name).strong());
                                    ui.label(
                                        RichText::new(&item.uuid).monospace().size(11.0).weak(),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if danger_icon_button(ui, VectorIcon::Trash, &remove_lbl)
                                            .clicked()
                                        {
                                            app.remove_server_whitelist(&item.name);
                                        }
                                    },
                                );
                            });
                        },
                    );
                    ui.add_space(4.0);
                }
            });

        cols[1].heading(RichText::new(&ban_heading).size(15.5).strong());
        cols[1].add_space(4.0);
        cols[1].horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.servers.new_ban_name)
                    .hint_text(&player_hint)
                    .desired_width(135.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.servers.new_ban_reason)
                    .hint_text(&reason_hint)
                    .desired_width(150.0),
            );
            if danger_icon_button(ui, VectorIcon::Shield, &ban_lbl).clicked() {
                app.add_server_ban();
            }
        });
        cols[1].separator();

        let ban_snapshot = app.servers.banlist.clone();
        ScrollArea::vertical()
            .id_salt("server_banlist_scroll")
            .show(&mut cols[1], |ui| {
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
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&item.name).strong());
                                    ui.label(
                                        RichText::new(format!("{reason_prefix}: {}", item.reason))
                                            .size(11.5)
                                            .weak(),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if secondary_icon_button(
                                            ui,
                                            VectorIcon::Refresh,
                                            &pardon_lbl,
                                        )
                                        .clicked()
                                        {
                                            app.remove_server_ban(&item.name);
                                        }
                                    },
                                );
                            });
                        },
                    );
                    ui.add_space(4.0);
                }
            });
    });
}

fn render_tab_network(app: &mut LauncherAppState, ui: &mut egui::Ui, srv: &ManagedServer) {
    let local_addr = format!("127.0.0.1:{}", srv.port);
    let lan_addr = app
        .lan_modal
        .primary_lan_ip
        .as_ref()
        .map(|ip| format!("{ip}:{}", srv.port));

    gaming_card_3d(
        ui,
        "srv_local_lan_card",
        false,
        ACCENT_CYAN,
        Margin::same(14),
        |ui| {
            ui.set_width(ui.available_width());
            ui.heading(
                RichText::new(app.i18n.tr(
                    "Kết nối Mạng Nội bộ (Local & LAN)",
                    "Local & LAN Network Endpoints",
                ))
                .size(16.0)
                .strong(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(app.i18n.tr("Địa chỉ trên máy này:", "Loopback Address:"));
                badge(ui, &local_addr, ACCENT_EMERALD);
                if secondary_icon_button(ui, VectorIcon::Copy, app.i18n.tr("Sao chép", "Copy"))
                    .clicked()
                {
                    ui.ctx().copy_text(local_addr.clone());
                }
            });
            if let Some(lan) = lan_addr {
                ui.horizontal(|ui| {
                    ui.label(app.i18n.tr("Địa chỉ mạng LAN nội bộ:", "LAN Address:"));
                    badge(ui, &lan, ACCENT_BLUE);
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Copy,
                        app.i18n.tr("Sao chép LAN", "Copy LAN"),
                    )
                    .clicked()
                    {
                        ui.ctx().copy_text(lan);
                    }
                });
            }
        },
    );

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
        TunnelStatus::Running {
            remote_addr,
            remote_port,
            ..
        } => format!("{remote_addr}:{remote_port}"),
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
                    "Đường hầm TCP Công khai (Tích hợp Bore Rust Client)",
                    "Public TCP Tunnel (Embedded Bore Rust Client)",
                ))
                .size(16.0)
                .strong(),
            );
            ui.label(
                RichText::new(app.i18n.tr(
                    "Mở địa chỉ công khai qua bore.pub để bạn bè ở khác mạng Wi-Fi có thể tham gia ngay mà không cần mở port modem.",
                    "Expose a public bore.pub TCP address so friends outside your Wi-Fi can join without router port forwarding.",
                ))
                .size(12.0)
                .weak(),
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
                            "Mở Bore Tunnel cho Máy chủ này",
                            "Open Bore Tunnel for This Server",
                        ),
                    )
                    .clicked()
                    {
                        app.toggle_selected_server_tunnel();
                    }
                }
                TunnelStatus::Connecting { local_port } => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if app.settings.language == Language::Vi {
                            format!(
                                "Đang thiết lập kết nối từ cổng nội bộ {local_port} tới bore.pub..."
                            )
                        } else {
                            format!("Connecting local port {local_port} to bore.pub...")
                        });
                        if danger_icon_button(ui, VectorIcon::Stop, app.i18n.t("common.cancel"))
                            .clicked()
                        {
                            app.toggle_selected_server_tunnel();
                        }
                    });
                }
                TunnelStatus::Running {
                    local_port,
                    remote_addr,
                    remote_port,
                } => {
                    let public_endpoint = format!("{remote_addr}:{remote_port}");
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(app.i18n.tr("Địa chỉ công khai:", "Public Endpoint:"))
                                .strong(),
                        );
                        badge(ui, &public_endpoint, ACCENT_EMERALD);
                        if primary_icon_button(
                            ui,
                            VectorIcon::Copy,
                            app.i18n.tr("Sao chép địa chỉ", "Copy Address"),
                        )
                        .clicked()
                        {
                            ui.ctx().copy_text(public_endpoint.clone());
                        }
                        if danger_icon_button(
                            ui,
                            VectorIcon::Stop,
                            app.i18n.tr("Đóng Tunnel", "Close Tunnel"),
                        )
                        .clicked()
                        {
                            app.toggle_selected_server_tunnel();
                        }
                    });
                    ui.label(
                        RichText::new(if app.settings.language == Language::Vi {
                            format!(
                                "Đang chuyển tiếp lưu lượng TCP từ {public_endpoint} → 127.0.0.1:{local_port}"
                            )
                        } else {
                            format!(
                                "Forwarding TCP traffic from {public_endpoint} → 127.0.0.1:{local_port}"
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
                            app.i18n.tr("Lỗi Tunnel", "Tunnel Error")
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
}
