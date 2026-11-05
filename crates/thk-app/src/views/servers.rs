use crate::fx3d::{
    gaming_card_3d, paint_3d_tunnel_conduit, paint_glowing_progress_bar, paint_icon_3d,
    sliding_pill_tabs, tactile_button_3d, ButtonStyle3d, Icon3dKind, VectorIcon,
};
use crate::state::{LauncherAppState, ServerTab};
use crate::theme::{ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_RED};
use crate::views::{
    badge, danger_icon_button, format_bytes, format_duration_hms, primary_icon_button,
    secondary_button, secondary_icon_button,
};
use egui::{vec2, Color32, Frame, Margin, RichText, ScrollArea, Sense};
use thk_core::models::{ContentKind, ManagedServer, ProviderSource};
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
                            RichText::new(
                                "Chưa có máy chủ cục bộ nào. Nhấn 'Tạo Máy chủ mới' để bắt đầu.",
                            )
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
                        let (st_label, st_color) = runtime_state_badge_info(rt_state);
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
                RichText::new("Trình Quản lý Máy chủ Minecraft Chuyên dụng")
                    .size(21.0)
                    .strong(),
            );
            ui.label(
                RichText::new(
                    "Hỗ trợ 10 lõi: Paper, Purpur, Folia, Vanilla, Fabric, Forge, NeoForge, Arclight, Mohist, SpongeVanilla.",
                )
                .weak(),
            );
            ui.add_space(14.0);
            if primary_icon_button(ui, VectorIcon::Plus, "Tạo Máy chủ mới").clicked() {
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

fn runtime_state_badge_info(state: ServerRuntimeState) -> (&'static str, Color32) {
    match state {
        ServerRuntimeState::Offline => ("Offline", Color32::GRAY),
        ServerRuntimeState::Starting => ("Đang khởi động", ACCENT_AMBER),
        ServerRuntimeState::Online => ("Trực tuyến", ACCENT_EMERALD),
        ServerRuntimeState::Stopping => ("Đang dừng", ACCENT_RED),
    }
}

fn render_server_header(app: &mut LauncherAppState, ui: &mut egui::Ui, srv: &ManagedServer) {
    let rt_state = app
        .servers
        .server_states
        .get(&srv.id)
        .copied()
        .unwrap_or_else(|| app.server_supervisor.runtime_state(&srv.id));
    let (st_label, st_color) = runtime_state_badge_info(rt_state);
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
                            "Chưa tải lõi JAR"
                        } else {
                            srv.jar_file.as_str()
                        };
                        ui.label(RichText::new(format!("Lõi: {jar_info}")).size(12.0).weak());
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
                                    "{}/{} người chơi • {}",
                                    stats.online_players,
                                    stats.max_players.max(20),
                                    format_duration_hms(stats.uptime_secs)
                                ),
                                ACCENT_EMERALD,
                            );
                        }
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if rt_state == ServerRuntimeState::Offline
                        && danger_icon_button(ui, VectorIcon::Trash, "Xóa").clicked()
                    {
                        app.delete_selected_server();
                        return;
                    }

                    if secondary_icon_button(ui, VectorIcon::Folder, "Thư mục").clicked() {
                        let _ = std::fs::create_dir_all(&srv.server_dir);
                        let _ = open::that(&srv.server_dir);
                    }

                    if rt_state == ServerRuntimeState::Offline
                        && secondary_icon_button(ui, VectorIcon::Download, "Tải / Cài lại Lõi")
                            .clicked()
                    {
                        app.install_selected_server_jar();
                    }

                    match rt_state {
                        ServerRuntimeState::Offline => {
                            if primary_icon_button(ui, VectorIcon::Play, "Khởi động").clicked() {
                                app.start_selected_server();
                            }
                        }
                        ServerRuntimeState::Online => {
                            if danger_icon_button(ui, VectorIcon::Stop, "Dừng").clicked() {
                                app.stop_selected_server();
                            }
                            if secondary_icon_button(ui, VectorIcon::Refresh, "Khởi động lại")
                                .clicked()
                            {
                                app.restart_selected_server();
                            }
                        }
                        ServerRuntimeState::Starting | ServerRuntimeState::Stopping => {
                            if danger_icon_button(ui, VectorIcon::Stop, "Buộc dừng").clicked() {
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
            "Whitelist & Banlist".to_string(),
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
                            RichText::new("Máy chủ chưa có nhật ký hoạt động.")
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
        let tab_pressed_while_focused = ui.memory(|m| m.has_focus(input_id))
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));

        if tab_pressed_while_focused {
            let trimmed = app.servers.console_input.trim_start_matches('/');
            if let Some(matched) = MC_SERVER_AUTOCOMPLETE_CMDS
                .iter()
                .find(|cand| cand.starts_with(trimmed) && **cand != trimmed)
            {
                app.servers.console_input = (*matched).to_string();
            }
        }

        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.servers.console_input)
                .id(input_id)
                .hint_text("Nhập lệnh máy chủ (nhấn Tab để gợi ý lệnh, Enter để gửi)...")
                .desired_width((ui.available_width() - 210.0).max(160.0)),
        );

        let enter_send = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter_send {
            app.send_selected_server_command();
            resp.request_focus();
        }

        if primary_icon_button(ui, VectorIcon::Play, "Gửi lệnh").clicked() {
            app.send_selected_server_command();
        }

        if secondary_icon_button(ui, VectorIcon::Trash, "Xoá màn hình").clicked() {
            app.servers.server_logs.remove(&srv.id);
        }
    });
}

fn render_tab_files(app: &mut LauncherAppState, ui: &mut egui::Ui, _srv: &ManagedServer) {
    ui.horizontal_wrapped(|ui| {
        if secondary_icon_button(ui, VectorIcon::Folder, "Gốc (/)").clicked() {
            app.servers.current_rel_dir.clear();
            app.servers.selected_entries.clear();
            app.refresh_selected_server_tabs();
        }
        if !app.servers.current_rel_dir.is_empty()
            && secondary_icon_button(ui, VectorIcon::ArrowLeft, "Lên cấp trên").clicked()
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

        if secondary_icon_button(ui, VectorIcon::Refresh, "Làm mới").clicked() {
            app.refresh_selected_server_tabs();
        }
        if primary_icon_button(ui, VectorIcon::Upload, "Tải tệp lên").clicked() {
            app.import_external_file_into_server();
        }

        ui.separator();
        ui.add(
            egui::TextEdit::singleline(&mut app.servers.archive_name_input)
                .hint_text("backup.zip")
                .desired_width(120.0),
        );
        if secondary_icon_button(ui, VectorIcon::Cube, "Nén mục chọn").clicked() {
            app.compress_selected_server_files();
        }
        if !app.servers.selected_entries.is_empty()
            && danger_icon_button(
                ui,
                VectorIcon::Trash,
                format!("Xóa ({})", app.servers.selected_entries.len()),
            )
            .clicked()
        {
            app.delete_selected_server_files();
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
                        RichText::new(format!("Đang chỉnh sửa: {editing_rel}"))
                            .strong()
                            .color(ACCENT_EMERALD),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if secondary_button(ui, "Đóng trình sửa").clicked() {
                            app.servers.editing_file_rel = None;
                        }
                        if primary_icon_button(ui, VectorIcon::Save, "Lưu tệp").clicked() {
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
                ui.label(RichText::new("Thư mục này hiện đang trống.").weak());
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
                                if ui
                                    .link(RichText::new(&entry.name).strong())
                                    .clicked()
                                {
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
                                                "Giải nén",
                                            )
                                            .clicked()
                                        {
                                            app.extract_server_archive(&entry.rel_path);
                                        }
                                        if is_editable_text_filename(&low)
                                            && secondary_icon_button(ui, VectorIcon::Wrench, "Sửa")
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
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Nguồn tải:").strong());
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
            if srv.engine.supports_plugins() {
                ui.selectable_value(
                    &mut app.servers.addon_kind,
                    ContentKind::Plugin,
                    "Plugin (plugins/)",
                );
            }
            if srv.engine.supports_mods() {
                ui.selectable_value(
                    &mut app.servers.addon_kind,
                    ContentKind::Mod,
                    "Server Mod (mods/)",
                );
            }
        }
    });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.servers.addon_query)
                .hint_text("Tìm kiếm EssentialsX, LuckPerms, WorldEdit, Geyser, Lithium...")
                .desired_width(340.0),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            app.search_server_addons();
        }
        if primary_icon_button(ui, VectorIcon::Search, "Tìm kiếm").clicked() {
            app.search_server_addons();
        }
    });

    ui.add_space(8.0);

    let hits = app.servers.addon_hits.clone();
    ScrollArea::vertical()
        .id_salt("server_addons_hits_scroll")
        .show(ui, |ui| {
            if hits.is_empty() {
                ui.label(
                    RichText::new(
                        "Nhập từ khóa và nhấn 'Tìm kiếm' để tải Plugin/Mod trực tiếp vào máy chủ.",
                    )
                    .weak(),
                );
                return;
            }

            for hit in hits {
                let hit_id = hit.project_id.clone();
                gaming_card_3d(
                    ui,
                    ("srv_addon_hit", hit_id),
                    false,
                    ACCENT_EMERALD,
                    Margin::symmetric(12, 9),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(&hit.title).size(14.5).strong());
                                    badge(ui, hit.provider.display_name(), ACCENT_BLUE);
                                    ui.label(
                                        RichText::new(format!("bởi {}", hit.author))
                                            .size(11.5)
                                            .weak(),
                                    );
                                });
                                ui.label(RichText::new(&hit.summary).size(12.0));
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if primary_icon_button(ui, VectorIcon::Download, "Cài đặt")
                                        .clicked()
                                    {
                                        app.install_server_addon_item(hit);
                                    }
                                },
                            );
                        });
                    },
                );
                ui.add_space(5.0);
            }
        });
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
                "srv_props_kv_card",
                false,
                ACCENT_EMERALD,
                Margin::same(14),
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("server.properties").size(14.5).strong());
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(app.i18n.tr("Thêm khoá:", "Add key:")).strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.new_prop_key)
                                .hint_text("online-mode")
                                .desired_width(160.0),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut app.servers.new_prop_val)
                                .hint_text("false")
                                .desired_width(160.0),
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

                    let keys: Vec<String> = app.servers.properties_map.keys().cloned().collect();
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
    ui.columns(2, |cols| {
        cols[0].heading(RichText::new("Danh sách Trắng (Whitelist)").size(15.5).strong());
        cols[0].add_space(4.0);
        cols[0].horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.servers.new_wl_name)
                    .hint_text("Tên người chơi...")
                    .desired_width(170.0),
            );
            if primary_icon_button(ui, VectorIcon::Plus, "Thêm").clicked() {
                app.add_server_whitelist();
            }
        });
        cols[0].separator();

        let wl_snapshot = app.servers.whitelist.clone();
        ScrollArea::vertical()
            .id_salt("server_whitelist_scroll")
            .show(&mut cols[0], |ui| {
                if wl_snapshot.is_empty() {
                    ui.label(RichText::new("Chưa có người chơi nào trong Whitelist.").weak());
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
                                        if danger_icon_button(ui, VectorIcon::Trash, "Gỡ").clicked()
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

        cols[1].heading(RichText::new("Danh sách Cấm (Banned Players)").size(15.5).strong());
        cols[1].add_space(4.0);
        cols[1].horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.servers.new_ban_name)
                    .hint_text("Tên người chơi...")
                    .desired_width(135.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.servers.new_ban_reason)
                    .hint_text("Lý do cấm...")
                    .desired_width(150.0),
            );
            if danger_icon_button(ui, VectorIcon::Shield, "Cấm").clicked() {
                app.add_server_ban();
            }
        });
        cols[1].separator();

        let ban_snapshot = app.servers.banlist.clone();
        ScrollArea::vertical()
            .id_salt("server_banlist_scroll")
            .show(&mut cols[1], |ui| {
                if ban_snapshot.is_empty() {
                    ui.label(RichText::new("Không có người chơi nào bị cấm.").weak());
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
                                        RichText::new(format!("Lý do: {}", item.reason))
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
                                            "Ân xá (Pardon)",
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
            ui.heading(RichText::new("Kết nối Mạng Nội bộ (Local & LAN)").size(16.0).strong());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Địa chỉ trên máy này:");
                badge(ui, &local_addr, ACCENT_EMERALD);
                if secondary_icon_button(ui, VectorIcon::Copy, "Sao chép").clicked() {
                    ui.ctx().copy_text(local_addr.clone());
                }
            });
            if let Some(lan) = lan_addr {
                ui.horizontal(|ui| {
                    ui.label("Địa chỉ mạng LAN nội bộ:");
                    badge(ui, &lan, ACCENT_BLUE);
                    if secondary_icon_button(ui, VectorIcon::Copy, "Sao chép LAN").clicked() {
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
                RichText::new("Đường hầm TCP Công khai (Tích hợp Bore Rust Client)")
                    .size(16.0)
                    .strong(),
            );
            ui.label(
                RichText::new(
                    "Mở địa chỉ công khai qua bore.pub để bạn bè ở khác mạng Wi-Fi có thể tham gia ngay mà không cần mở port modem.",
                )
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
                        "Mở Bore Tunnel cho Máy chủ này",
                    )
                    .clicked()
                    {
                        app.toggle_selected_server_tunnel();
                    }
                }
                TunnelStatus::Connecting { local_port } => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!(
                            "Đang thiết lập kết nối từ cổng nội bộ {local_port} tới bore.pub..."
                        ));
                        if danger_icon_button(ui, VectorIcon::Stop, "Huỷ").clicked() {
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
                        ui.label(RichText::new("Địa chỉ công khai:").strong());
                        badge(ui, &public_endpoint, ACCENT_EMERALD);
                        if primary_icon_button(ui, VectorIcon::Copy, "Sao chép địa chỉ").clicked()
                        {
                            ui.ctx().copy_text(public_endpoint.clone());
                        }
                        if danger_icon_button(ui, VectorIcon::Stop, "Đóng Tunnel").clicked() {
                            app.toggle_selected_server_tunnel();
                        }
                    });
                    ui.label(
                        RichText::new(format!(
                            "Đang chuyển tiếp lưu lượng TCP từ {public_endpoint} → 127.0.0.1:{local_port}"
                        ))
                        .size(11.5)
                        .weak(),
                    );
                }
                TunnelStatus::Error(err) => {
                    ui.label(RichText::new(format!("Lỗi Tunnel: {err}")).color(ACCENT_RED));
                    if primary_icon_button(ui, VectorIcon::Refresh, "Thử kết nối lại").clicked() {
                        app.toggle_selected_server_tunnel();
                    }
                }
            }
        },
    );
}
