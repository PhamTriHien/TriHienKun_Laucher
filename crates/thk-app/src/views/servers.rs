use crate::state::{LauncherAppState, ServerTab};
use crate::theme::{ACCENT_AMBER, ACCENT_BLUE, ACCENT_EMERALD, ACCENT_RED};
use crate::views::{
    badge, danger_button, format_bytes, format_duration_hms, primary_button,
};
use egui::{Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke};
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
        .default_size(255.0)
        .size_range(210.0..=350.0)
        .frame(Frame::NONE.inner_margin(Margin {
            left: 0,
            right: 12,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.heading(RichText::new("Máy chủ cục bộ").size(17.0).strong());
            ui.add_space(6.0);

            if primary_button(ui, "➕  Tạo Máy chủ mới").clicked() {
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
                            RichText::new("Chưa có máy chủ cục bộ nào. Nhấn 'Tạo Máy chủ mới' để bắt đầu.")
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

                        let fill = if is_sel {
                            Color32::from_rgba_unmultiplied(34, 197, 94, 34)
                        } else {
                            ui.visuals().faint_bg_color
                        };
                        let stroke = if is_sel {
                            Stroke::new(1.5, ACCENT_EMERALD)
                        } else {
                            ui.visuals().widgets.noninteractive.bg_stroke
                        };

                        let resp = Frame::NONE
                            .fill(fill)
                            .stroke(stroke)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&srv.name).strong().size(13.5));
                                    badge(ui, st_label, st_color);
                                });
                                ui.horizontal(|ui| {
                                    badge(ui, srv.engine.display_name(), ACCENT_BLUE);
                                    badge(ui, &srv.game_version, ACCENT_AMBER);
                                    ui.label(
                                        RichText::new(format!(":{}", srv.port)).size(11.0).weak(),
                                    );
                                });
                            })
                            .response
                            .interact(egui::Sense::click());

                        if resp.clicked() {
                            app.click_sfx();
                            app.select_server(srv.id);
                        }
                        ui.add_space(4.0);
                    }
                });
        });

    let Some(srv) = app.selected_server().cloned() else {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading("Trình Quản lý Máy chủ Minecraft Chuyên dụng");
            ui.label(
                "Hỗ trợ 10 lõi: Paper, Purpur, Folia, Vanilla, Fabric, Forge, NeoForge, Arclight, Mohist, SpongeVanilla.",
            );
            ui.add_space(12.0);
            if primary_button(ui, "➕  Tạo Máy chủ mới").clicked() {
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
    ui.separator();
    ui.add_space(6.0);

    match app.servers.active_tab {
        ServerTab::Console => render_tab_console(app, ui, &srv),
        ServerTab::Files => render_tab_files(app, ui, &srv),
        ServerTab::PluginsMods => render_tab_plugins_mods(app, ui, &srv),
        ServerTab::Properties => render_tab_properties(app, ui),
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

    Frame::group(ui.style())
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
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
                            ui.label(
                                RichText::new(format!(
                                    "CPU: {:.1}%  •  RAM: {} / {} MB  •  Người chơi: {}/{}  •  Uptime: {}",
                                    stats.cpu_percent,
                                    stats.ram_mb,
                                    srv.ram_max_mb,
                                    stats.online_players,
                                    stats.max_players.max(20),
                                    format_duration_hms(stats.uptime_secs),
                                ))
                                .size(12.0)
                                .color(ACCENT_EMERALD),
                            );
                        }
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if rt_state == ServerRuntimeState::Offline && danger_button(ui, "🗑 Xóa").clicked() {
                        app.delete_selected_server();
                        return;
                    }

                    if ui.button("📂 Thư mục").clicked() {
                        let _ = std::fs::create_dir_all(&srv.server_dir);
                        let _ = open::that(&srv.server_dir);
                    }

                    if rt_state == ServerRuntimeState::Offline
                        && ui.button("⬇ Tải / Cài lại Lõi").clicked()
                    {
                        app.install_selected_server_jar();
                    }

                    match rt_state {
                        ServerRuntimeState::Offline => {
                            if primary_button(ui, "▶ Khởi động").clicked() {
                                app.start_selected_server();
                            }
                        }
                        ServerRuntimeState::Online => {
                            if danger_button(ui, "⏹ Dừng").clicked() {
                                app.stop_selected_server();
                            }
                            if ui.button("🔄 Khởi động lại").clicked() {
                                app.restart_selected_server();
                            }
                        }
                        ServerRuntimeState::Starting | ServerRuntimeState::Stopping => {
                            if danger_button(ui, "⏹ Buộc dừng").clicked() {
                                app.stop_selected_server();
                            }
                        }
                    }
                });
            });
        });
}

fn render_server_tabs_bar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let tabs = [
        (ServerTab::Console, "🖥 Bảng điều khiển (Console)"),
        (ServerTab::Files, "📁 Quản lý Tệp"),
        (ServerTab::PluginsMods, "🧩 Plugins / Mods"),
        (ServerTab::Properties, "⚙ server.properties"),
        (ServerTab::AccessControl, "🛡 Whitelist & Banlist"),
        (ServerTab::Network, "🌐 Mạng & Bore Tunnel"),
    ];

    ui.horizontal_wrapped(|ui| {
        for (tab, title) in tabs {
            if ui
                .selectable_label(app.servers.active_tab == tab, RichText::new(title).strong())
                .clicked()
            {
                app.click_sfx();
                app.servers.active_tab = tab;
                app.refresh_selected_server_tabs();
            }
        }
    });
}

fn render_tab_console(app: &mut LauncherAppState, ui: &mut egui::Ui, srv: &ManagedServer) {
    let lines = app
        .servers
        .server_logs
        .get(&srv.id)
        .cloned()
        .unwrap_or_default();

    let console_h = (ui.available_height() - 48.0).max(160.0);
    Frame::group(ui.style())
        .fill(Color32::from_rgb(9, 12, 17))
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
                .desired_width((ui.available_width() - 180.0).max(160.0)),
        );

        let enter_send = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter_send {
            app.send_selected_server_command();
            resp.request_focus();
        }

        if primary_button(ui, "Gửi lệnh").clicked() {
            app.send_selected_server_command();
        }

        if ui.button("Xoá màn hình").clicked() {
            app.servers.server_logs.remove(&srv.id);
        }
    });
}

fn render_tab_files(app: &mut LauncherAppState, ui: &mut egui::Ui, _srv: &ManagedServer) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("🏠 Gốc (/)").clicked() {
            app.servers.current_rel_dir.clear();
            app.servers.selected_entries.clear();
            app.refresh_selected_server_tabs();
        }
        if !app.servers.current_rel_dir.is_empty() && ui.button("⬅ Lên cấp trên").clicked() {
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

        if ui.button("🔄 Làm mới").clicked() {
            app.refresh_selected_server_tabs();
        }
        if primary_button(ui, "📤 Tải tệp lên").clicked() {
            app.import_external_file_into_server();
        }

        ui.separator();
        ui.add(
            egui::TextEdit::singleline(&mut app.servers.archive_name_input)
                .hint_text("backup.zip")
                .desired_width(120.0),
        );
        if ui.button("📦 Nén mục chọn").clicked() {
            app.compress_selected_server_files();
        }
        if !app.servers.selected_entries.is_empty()
            && danger_button(
                ui,
                format!("🗑 Xóa ({})", app.servers.selected_entries.len()),
            )
            .clicked()
        {
            app.delete_selected_server_files();
        }
    });

    ui.add_space(6.0);

    if let Some(editing_rel) = app.servers.editing_file_rel.clone() {
        Frame::group(ui.style())
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("📝 Đang chỉnh sửa: {editing_rel}"))
                            .strong()
                            .color(ACCENT_EMERALD),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✖ Đóng trình sửa").clicked() {
                            app.servers.editing_file_rel = None;
                        }
                        if primary_button(ui, "💾 Lưu tệp").clicked() {
                            app.save_server_file_editor();
                        }
                    });
                });
                ui.add_space(4.0);
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
            });
        ui.add_space(6.0);
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
                Frame::group(ui.style())
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut checked =
                                app.servers.selected_entries.contains(&entry.rel_path);
                            if ui.checkbox(&mut checked, "").changed() {
                                if checked {
                                    app.servers.selected_entries.insert(entry.rel_path.clone());
                                } else {
                                    app.servers.selected_entries.remove(&entry.rel_path);
                                }
                            }

                            if entry.is_dir {
                                if ui
                                    .link(RichText::new(format!("📁  {}", entry.name)).strong())
                                    .clicked()
                                {
                                    app.servers.current_rel_dir = entry.rel_path.clone();
                                    app.servers.selected_entries.clear();
                                    app.refresh_selected_server_tabs();
                                }
                            } else {
                                ui.label(RichText::new(format!("📄  {}", entry.name)));
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if !entry.is_dir {
                                        let low = entry.name.to_ascii_lowercase();
                                        if low.ends_with(".zip")
                                            && ui.button("📂 Giải nén").clicked()
                                        {
                                            app.extract_server_archive(&entry.rel_path);
                                        }
                                        if is_editable_text_filename(&low)
                                            && ui.button("✏ Sửa").clicked()
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
                    });
                ui.add_space(3.0);
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
        if primary_button(ui, "🔍 Tìm kiếm").clicked() {
            app.search_server_addons();
        }
    });

    ui.separator();

    let hits = app.servers.addon_hits.clone();
    ScrollArea::vertical()
        .id_salt("server_addons_hits_scroll")
        .show(ui, |ui| {
            if hits.is_empty() {
                ui.label(
                    RichText::new("Nhập từ khóa và nhấn 'Tìm kiếm' để tải Plugin/Mod trực tiếp vào máy chủ.")
                        .weak(),
                );
                return;
            }

            for hit in hits {
                Frame::group(ui.style())
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
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
                                    if primary_button(ui, "⬇ Cài đặt").clicked() {
                                        app.install_server_addon_item(hit);
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(4.0);
            }
        });
}

fn render_tab_properties(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Thêm khoá mới:").strong());
        ui.add(
            egui::TextEdit::singleline(&mut app.servers.new_prop_key)
                .hint_text("Ví dụ: online-mode")
                .desired_width(170.0),
        );
        ui.add(
            egui::TextEdit::singleline(&mut app.servers.new_prop_val)
                .hint_text("false")
                .desired_width(170.0),
        );
        if primary_button(ui, "💾 Lưu server.properties").clicked() {
            app.save_server_properties_edits();
        }
    });

    ui.separator();

    let keys: Vec<String> = app.servers.properties_map.keys().cloned().collect();
    ScrollArea::vertical()
        .id_salt("server_properties_scroll")
        .show(ui, |ui| {
            egui::Grid::new("server_properties_grid")
                .num_columns(2)
                .spacing([16.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    for key in keys {
                        ui.label(RichText::new(&key).monospace().size(12.5));
                        if let Some(val) = app.servers.properties_map.get_mut(&key) {
                            ui.add(
                                egui::TextEdit::singleline(val)
                                    .desired_width(320.0),
                            );
                        }
                        ui.end_row();
                    }
                });
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
            if primary_button(ui, "➕ Thêm").clicked() {
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
                    Frame::group(ui.style())
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
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
                                        if danger_button(ui, "Gỡ").clicked() {
                                            app.remove_server_whitelist(&item.name);
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(3.0);
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
            if danger_button(ui, "🔨 Cấm").clicked() {
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
                    Frame::group(ui.style())
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
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
                                        if ui.button("Ân xá (Pardon)").clicked() {
                                            app.remove_server_ban(&item.name);
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(3.0);
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

    Frame::group(ui.style())
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.heading(RichText::new("Kết nối Mạng Nội bộ (Local & LAN)").size(16.0).strong());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Địa chỉ trên máy này:");
                badge(ui, &local_addr, ACCENT_EMERALD);
                if ui.button("📋 Sao chép").clicked() {
                    ui.ctx().copy_text(local_addr.clone());
                }
            });
            if let Some(lan) = lan_addr {
                ui.horizontal(|ui| {
                    ui.label("Địa chỉ mạng LAN nội bộ:");
                    badge(ui, &lan, ACCENT_BLUE);
                    if ui.button("📋 Sao chép LAN").clicked() {
                        ui.ctx().copy_text(lan);
                    }
                });
            }
        });

    ui.add_space(10.0);

    let tunnel_status = app
        .servers
        .server_tunnels
        .get(&srv.id)
        .cloned()
        .unwrap_or_else(|| app.server_supervisor.server_tunnel_status(&srv.id));

    Frame::group(ui.style())
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
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

            match tunnel_status {
                TunnelStatus::Idle | TunnelStatus::Stopped => {
                    if primary_button(ui, "🌐 Mở Bore Tunnel cho Máy chủ này").clicked() {
                        app.toggle_selected_server_tunnel();
                    }
                }
                TunnelStatus::Connecting { local_port } => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!(
                            "Đang thiết lập kết nối từ cổng nội bộ {local_port} tới bore.pub..."
                        ));
                        if danger_button(ui, "Huỷ").clicked() {
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
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Địa chỉ công khai:").strong());
                        badge(ui, &public_endpoint, ACCENT_EMERALD);
                        if primary_button(ui, "📋 Sao chép địa chỉ").clicked() {
                            ui.ctx().copy_text(public_endpoint.clone());
                        }
                        if danger_button(ui, "⏹ Đóng Tunnel").clicked() {
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
                    if primary_button(ui, "🔄 Thử kết nối lại").clicked() {
                        app.toggle_selected_server_tunnel();
                    }
                }
            }
        });
}
