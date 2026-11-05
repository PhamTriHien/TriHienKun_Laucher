use crate::state::{HomeTab, LauncherAppState};
use crate::theme::{ACCENT_AMBER, ACCENT_BLUE, ACCENT_EMERALD, ACCENT_EMERALD_DIM, ACCENT_RED};
use crate::views::{
    badge, danger_button, format_bytes, format_duration_hms, primary_button, render_motd_spans,
};
use egui::{Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, Vec2};
use thk_core::models::{ContentKind, ProviderSource};
use thk_launcher::InstanceKey;

pub fn render_home_view(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    egui::Panel::left("thk_home_profiles_sidebar")
        .resizable(true)
        .default_size(255.0)
        .size_range(210.0..=340.0)
        .frame(Frame::NONE.inner_margin(Margin {
            left: 0,
            right: 12,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.heading(RichText::new("Profiles").size(17.0).strong());
            ui.add_space(6.0);

            if primary_button(ui, format!("➕  {}", app.i18n.t("home.new_profile"))).clicked() {
                app.click_sfx();
                let mc = app.home.new_draft.game_version.clone();
                let ldr = app.home.new_draft.loader;
                app.ensure_loader_versions(&mc, ldr);
                app.show_new_profile_modal = true;
            }

            if ui
                .add_sized(
                    [ui.available_width(), 31.0],
                    egui::Button::new(format!("📥  {}", app.i18n.t("home.import_pack"))),
                )
                .clicked()
            {
                app.click_sfx();
                app.import_modpack_from_disk();
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            let profiles_snapshot = app.home.profiles.clone();
            let selected_id = app.home.selected_profile_id.clone();

            ScrollArea::vertical()
                .id_salt("home_profiles_scroll")
                .show(ui, |ui| {
                    if profiles_snapshot.is_empty() {
                        ui.label(RichText::new(app.i18n.t("home.no_profiles")).weak());
                    }
                    for prof in profiles_snapshot {
                        let is_sel = selected_id.as_deref() == Some(&prof.id);
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
                                ui.label(RichText::new(&prof.name).strong().size(13.5));
                                ui.horizontal(|ui| {
                                    badge(ui, &prof.game_version, ACCENT_BLUE);
                                    badge(ui, prof.loader.display_name(), ACCENT_EMERALD);
                                    if prof.total_play_seconds > 0 {
                                        ui.label(
                                            RichText::new(format_duration_hms(
                                                prof.total_play_seconds,
                                            ))
                                            .size(11.0)
                                            .weak(),
                                        );
                                    }
                                });
                            })
                            .response
                            .interact(egui::Sense::click());

                        if resp.clicked() {
                            app.click_sfx();
                            app.select_profile(prof.id);
                        }
                        ui.add_space(4.0);
                    }
                });
        });

    let Some(profile) = app.selected_profile().cloned() else {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading(app.i18n.t("app.name"));
            ui.label(app.i18n.t("home.no_profiles"));
            ui.add_space(12.0);
            if primary_button(ui, format!("➕  {}", app.i18n.t("home.new_profile"))).clicked() {
                app.show_new_profile_modal = true;
            }
        });
        return;
    };

    render_hero_launch_banner(app, ui, &profile);
    ui.add_space(10.0);
    render_profile_tabs_bar(app, ui);
    ui.separator();
    ui.add_space(6.0);

    match app.home.active_tab {
        HomeTab::General => render_tab_general(app, ui, &profile),
        HomeTab::Mods => render_tab_content(app, ui, &profile, ContentKind::Mod),
        HomeTab::Shaders => render_tab_content(app, ui, &profile, ContentKind::Shader),
        HomeTab::ResourcePacks => render_tab_content(app, ui, &profile, ContentKind::ResourcePack),
        HomeTab::Worlds => render_tab_worlds(app, ui, &profile),
        HomeTab::Bookmarks => render_tab_bookmarks(app, ui),
        HomeTab::Logs => render_tab_logs(app, ui, &profile),
        HomeTab::Analytics => render_tab_analytics(app, ui, &profile),
    }
}

fn render_hero_launch_banner(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
) {
    Frame::group(ui.style())
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.heading(RichText::new(&profile.name).size(21.0).strong());
                    ui.horizontal(|ui| {
                        badge(ui, &format!("MC {}", profile.game_version), ACCENT_BLUE);
                        let ldr_txt = match profile.loader_version {
                            Some(ref lv) => format!("{} {lv}", profile.loader.display_name()),
                            None => profile.loader.display_name().to_string(),
                        };
                        badge(ui, &ldr_txt, ACCENT_EMERALD);
                        badge(ui, &format!("{} MB RAM", profile.ram_mb), ACCENT_AMBER);
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let selected_acct = app.selected_account().cloned();
                    let active_key = selected_acct
                        .as_ref()
                        .map(|a| InstanceKey::new(profile.id.clone(), a.id.clone()));
                    let is_running = active_key
                        .as_ref()
                        .is_some_and(|k| app.instance_supervisor.is_running(k));

                    if is_running {
                        if let Some(key) = active_key {
                            let stop_btn = egui::Button::new(
                                RichText::new(format!("⏹  {}", app.i18n.t("home.stop")))
                                    .size(15.0)
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(185, 28, 28))
                            .min_size(Vec2::new(170.0, 42.0));
                            if ui.add(stop_btn).clicked() {
                                app.instance_supervisor.stop_instance(&key);
                            }
                        }
                    } else {
                        let play_btn = egui::Button::new(
                            RichText::new(format!("▶  {}", app.i18n.t("home.play")))
                                .size(15.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(ACCENT_EMERALD_DIM)
                        .stroke(Stroke::new(1.5, ACCENT_EMERALD))
                        .min_size(Vec2::new(185.0, 42.0));
                        if ui.add(play_btn).clicked() {
                            app.launch_selected_profile(None);
                        }
                    }

                    if ui
                        .button(format!("📂 {}", app.i18n.t("home.open_folder")))
                        .clicked()
                    {
                        let _ = std::fs::create_dir_all(&profile.instance_path);
                        let _ = open::that(&profile.instance_path);
                    }

                    let accts = app.accounts.accounts.clone();
                    let current_name = selected_acct
                        .map(|a| format!("👤 {}", a.username))
                        .unwrap_or_else(|| "👤 Chọn tài khoản".into());
                    egui::ComboBox::from_id_salt("hero_account_combo")
                        .selected_text(current_name)
                        .show_ui(ui, |ui| {
                            for a in accts {
                                let label = format!("{} ({})", a.username, a.account_type.as_str());
                                if ui
                                    .selectable_label(
                                        app.accounts.selected_account_id.as_deref()
                                            == Some(a.id.as_str()),
                                        label,
                                    )
                                    .clicked()
                                {
                                    app.select_account(a.id);
                                }
                            }
                        });
                });
            });

            let snaps = app.instance_supervisor.running_snapshots();
            let matching_snaps: Vec<_> = snaps
                .into_iter()
                .filter(|s| s.key.profile_id == profile.id)
                .collect();
            if !matching_snaps.is_empty() {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for s in matching_snaps {
                        let uname = app
                            .accounts
                            .accounts
                            .iter()
                            .find(|a| a.id == s.key.account_id)
                            .map(|a| a.username.as_str())
                            .unwrap_or("Player");
                        let pid_txt = s.pid.map(|p| format!("PID {p}")).unwrap_or_default();
                        badge(
                            ui,
                            &format!(
                                "Đang chạy: {uname} ({pid_txt} • {})",
                                format_duration_hms(s.uptime_secs)
                            ),
                            ACCENT_EMERALD,
                        );
                    }
                });
            }
        });
}

fn render_profile_tabs_bar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let tabs = [
        (HomeTab::General, app.i18n.t("home.tab.general").to_string()),
        (HomeTab::Mods, app.i18n.t("home.tab.mods").to_string()),
        (HomeTab::Shaders, app.i18n.t("home.tab.shaders").to_string()),
        (
            HomeTab::ResourcePacks,
            app.i18n.t("home.tab.resourcepacks").to_string(),
        ),
        (HomeTab::Worlds, app.i18n.t("home.tab.worlds").to_string()),
        (
            HomeTab::Bookmarks,
            app.i18n.t("home.tab.bookmarks").to_string(),
        ),
        (HomeTab::Logs, app.i18n.t("home.tab.logs").to_string()),
        (
            HomeTab::Analytics,
            app.i18n.t("home.tab.analytics").to_string(),
        ),
    ];

    ui.horizontal_wrapped(|ui| {
        for (tab, label) in tabs {
            if ui
                .selectable_label(app.home.active_tab == tab, RichText::new(label).strong())
                .clicked()
            {
                app.click_sfx();
                app.home.active_tab = tab;
                app.home.show_online_browser = false;
                app.refresh_selected_profile_tabs();
                if tab == HomeTab::Bookmarks {
                    app.ping_all_bookmarks();
                }
            }
        }
    });
}

fn render_tab_general(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
) {
    if app.home.edit_draft.loaded_for_id != profile.id {
        app.sync_profile_edit_draft();
    }

    ScrollArea::vertical()
        .id_salt("home_tab_general")
        .show(ui, |ui| {
            Frame::group(ui.style())
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    egui::Grid::new("profile_edit_grid")
                        .num_columns(2)
                        .spacing([16.0, 10.0])
                        .show(ui, |ui| {
                            ui.label(app.i18n.t("profile.name"));
                            ui.add(
                                egui::TextEdit::singleline(&mut app.home.edit_draft.name)
                                    .desired_width(340.0),
                            );
                            ui.end_row();

                            ui.label(app.i18n.t("profile.ram"));
                            ui.add(
                                egui::Slider::new(&mut app.home.edit_draft.ram_mb, 1024..=32768)
                                    .step_by(256.0)
                                    .suffix(" MB"),
                            );
                            ui.end_row();

                            ui.label(app.i18n.t("profile.java_path"));
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut app.home.edit_draft.java_path_str,
                                    )
                                    .hint_text("Mặc định tự động chọn Mojang Java chuẩn")
                                    .desired_width(280.0),
                                );
                                if ui.button("Chọn...").clicked() {
                                    if let Some(f) = rfd::FileDialog::new().pick_file() {
                                        app.home.edit_draft.java_path_str =
                                            f.to_string_lossy().to_string();
                                    }
                                }
                                if ui.button("Mặc định").clicked() {
                                    app.home.edit_draft.java_path_str.clear();
                                }
                            });
                            ui.end_row();

                            ui.label(app.i18n.t("profile.jvm_args"));
                            ui.add(
                                egui::TextEdit::singleline(&mut app.home.edit_draft.jvm_args)
                                    .hint_text("-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions")
                                    .desired_width(380.0),
                            );
                            ui.end_row();
                        });

                    if profile.loader == thk_core::models::LoaderType::Fabric {
                        ui.add_space(6.0);
                        ui.checkbox(
                            &mut app.home.edit_draft.auto_perf_mods,
                            app.i18n.t("profile.auto_perf"),
                        );
                    }

                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if primary_button(ui, format!("💾  {}", app.i18n.t("profile.save")))
                            .clicked()
                        {
                            app.save_profile_edits();
                        }
                        if danger_button(ui, format!("🗑  {}", app.i18n.t("profile.delete")))
                            .clicked()
                        {
                            app.delete_selected_profile();
                        }
                    });
                });
        });
}

fn render_tab_content(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
    kind: ContentKind,
) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.home.local_filter)
                .hint_text("Lọc tệp đã cài đặt...")
                .desired_width(220.0),
        );
        if ui.button("🔄 Làm mới").clicked() {
            app.refresh_selected_profile_tabs();
        }
        if ui.button("📂 Mở thư mục").clicked() {
            if let Ok(sub) = thk_core::SafePath::resolve_child(
                &profile.instance_path,
                kind.instance_subdir(),
            ) {
                let _ = std::fs::create_dir_all(&sub);
                let _ = open::that(&sub);
            }
        }
        let toggle_label = if app.home.show_online_browser {
            "✖ Đóng kho tải trực tuyến"
        } else {
            "🌐 Tìm & Cài trực tuyến (Modrinth / CurseForge)"
        };
        if primary_button(ui, toggle_label).clicked() {
            app.home.show_online_browser = !app.home.show_online_browser;
            if app.home.show_online_browser && app.home.online_hits.is_empty() {
                app.search_inline_profile_content(kind);
            }
        }
    });

    ui.add_space(6.0);

    if app.home.show_online_browser {
        Frame::group(ui.style())
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut app.home.online_provider,
                        ProviderSource::Modrinth,
                        "Modrinth",
                    );
                    ui.selectable_value(
                        &mut app.home.online_provider,
                        ProviderSource::CurseForge,
                        "CurseForge",
                    );
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut app.home.online_query)
                            .hint_text(app.i18n.t("mods.search_placeholder"))
                            .desired_width(260.0),
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        app.search_inline_profile_content(kind);
                    }
                    if primary_button(ui, "🔍 Tìm kiếm").clicked() {
                        app.search_inline_profile_content(kind);
                    }
                    if app.home.online_selected.is_some() && ui.button("⬅ Quay lại kết quả").clicked()
                    {
                        app.home.online_selected = None;
                    }
                });

                ui.add_space(6.0);
                ScrollArea::vertical()
                    .id_salt("inline_online_results")
                    .max_height(210.0)
                    .show(ui, |ui| {
                        if let Some((detail, versions)) = app.home.online_selected.clone() {
                            ui.label(
                                RichText::new(format!(
                                    "Chọn phiên bản của '{}' tương thích MC {}:",
                                    detail.summary.title, profile.game_version
                                ))
                                .strong(),
                            );
                            if versions.is_empty() {
                                ui.label(
                                    RichText::new("Không có bản phát hành khớp phiên bản/loader này.")
                                        .color(ACCENT_RED),
                                );
                            }
                            for ver in versions.into_iter().take(12) {
                                ui.horizontal(|ui| {
                                    badge(ui, &ver.release_channel, ACCENT_EMERALD);
                                    ui.label(RichText::new(&ver.version_number).strong());
                                    ui.label(RichText::new(&ver.name).weak());
                                    if primary_button(ui, app.i18n.t("mods.install")).clicked() {
                                        app.install_inline_version(
                                            detail.summary.clone(),
                                            ver,
                                            kind,
                                        );
                                    }
                                });
                            }
                        } else {
                            let hits = app.home.online_hits.clone();
                            for item in hits {
                                Frame::NONE
                                    .fill(ui.visuals().faint_bg_color)
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(Margin::symmetric(10, 6))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.vertical(|ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(RichText::new(&item.title).strong());
                                                    ui.label(
                                                        RichText::new(format!("by {}", item.author))
                                                            .size(11.5)
                                                            .weak(),
                                                    );
                                                });
                                                ui.label(RichText::new(&item.summary).size(12.0));
                                            });
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if primary_button(ui, "Chọn bản cài").clicked() {
                                                        app.load_inline_project_versions(item, kind);
                                                    }
                                                },
                                            );
                                        });
                                    });
                                ui.add_space(4.0);
                            }
                        }
                    });
            });
        ui.add_space(8.0);
    }

    let filter_low = app.home.local_filter.trim().to_ascii_lowercase();
    let local_snapshot = app.home.local_items.clone();

    ScrollArea::vertical()
        .id_salt("local_installed_content_scroll")
        .show(ui, |ui| {
            if local_snapshot.is_empty() {
                ui.label(RichText::new("Chưa có tệp nào trong thư mục này.").weak());
                return;
            }
            for item in local_snapshot {
                if !filter_low.is_empty()
                    && !item.title.to_ascii_lowercase().contains(&filter_low)
                    && !item.file_name.to_ascii_lowercase().contains(&filter_low)
                {
                    continue;
                }
                Frame::group(ui.style())
                    .inner_margin(Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut enabled = item.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                app.toggle_local_item(kind, &item.file_name, enabled);
                            }
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&item.title).strong());
                                    if !item.version_number.is_empty() {
                                        badge(ui, &item.version_number, ACCENT_BLUE);
                                    }
                                    badge(ui, item.provider.display_name(), ACCENT_EMERALD);
                                });
                                ui.label(
                                    RichText::new(format!(
                                        "{} • {}",
                                        item.file_name,
                                        format_bytes(item.size_bytes)
                                    ))
                                    .size(11.5)
                                    .weak(),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if danger_button(ui, "Xóa").clicked() {
                                        app.delete_local_item(kind, &item.file_name);
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(4.0);
            }
        });
}

fn render_tab_worlds(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
) {
    let worlds = app.home.worlds.clone();
    ScrollArea::vertical()
        .id_salt("home_worlds_scroll")
        .show(ui, |ui| {
            if worlds.is_empty() {
                ui.label(RichText::new("Chưa có thế giới Singleplayer nào.").weak());
                return;
            }
            for w in worlds {
                Frame::group(ui.style())
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&w.folder_name).strong().size(14.0));
                                ui.label(
                                    RichText::new(format!("Dung lượng: {}", format_bytes(w.size_bytes)))
                                        .size(12.0)
                                        .weak(),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if danger_button(ui, "Xóa").clicked() {
                                        app.delete_world_folder(&w.folder_name);
                                    }
                                    if ui.button("📂 Mở").clicked() {
                                        if let Ok(p) = thk_core::SafePath::resolve_child(
                                            &profile.instance_path.join("saves"),
                                            &w.folder_name,
                                        ) {
                                            let _ = open::that(p);
                                        }
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(4.0);
            }
        });
}

fn render_tab_bookmarks(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.home.new_bm_name)
                .hint_text("Tên máy chủ (VD: Hypixel)")
                .desired_width(170.0),
        );
        ui.add(
            egui::TextEdit::singleline(&mut app.home.new_bm_addr)
                .hint_text("Địa chỉ IP / Domain (VD: mc.hypixel.net)")
                .desired_width(240.0),
        );
        if primary_button(ui, "➕ Thêm Server").clicked() {
            app.add_bookmark();
        }
        if ui.button("📡 Ping tất cả").clicked() {
            app.ping_all_bookmarks();
        }
    });

    ui.add_space(8.0);
    let bms = app.home.bookmarks.clone();
    let pings = app.home.bookmark_pings.clone();

    ScrollArea::vertical()
        .id_salt("home_bookmarks_scroll")
        .show(ui, |ui| {
            for bm in bms {
                Frame::group(ui.style())
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&bm.name).strong().size(14.5));
                                    ui.label(RichText::new(&bm.address).monospace().weak());
                                });
                                match pings.get(&bm.id) {
                                    Some(Ok(st)) => {
                                        ui.horizontal(|ui| {
                                            badge(
                                                ui,
                                                &format!("{} ms", st.latency_ms.unwrap_or(0)),
                                                ACCENT_EMERALD,
                                            );
                                            badge(
                                                ui,
                                                &format!(
                                                    "{}/{} người chơi",
                                                    st.online_players, st.max_players
                                                ),
                                                ACCENT_BLUE,
                                            );
                                            ui.label(
                                                RichText::new(&st.version_name).size(11.5).weak(),
                                            );
                                        });
                                        render_motd_spans(ui, &st.motd_spans);
                                    }
                                    Some(Err(err)) => {
                                        ui.label(
                                            RichText::new(format!("Offline: {err}"))
                                                .size(12.0)
                                                .color(ACCENT_RED),
                                        );
                                    }
                                    None => {
                                        ui.label(RichText::new("Đang chờ Ping...").size(12.0).weak());
                                    }
                                }
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if danger_button(ui, "Xóa").clicked() {
                                        app.delete_bookmark(&bm.id);
                                    }
                                    if primary_button(ui, "▶ Vào ngay").clicked() {
                                        app.launch_selected_profile(Some(bm.address.clone()));
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(6.0);
            }
        });
}

fn render_tab_logs(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
) {
    let selected_key = app
        .selected_account()
        .map(|a| InstanceKey::new(profile.id.clone(), a.id.clone()));

    let lines: Vec<String> = selected_key
        .as_ref()
        .and_then(|k| app.home.instance_logs.get(k))
        .cloned()
        .or_else(|| {
            app.home
                .instance_logs
                .iter()
                .find(|(k, _)| k.profile_id == profile.id)
                .map(|(_, v)| v.clone())
        })
        .unwrap_or_default();

    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.home.log_filter)
                .hint_text("Lọc dòng nhật ký...")
                .desired_width(240.0),
        );
        ui.checkbox(&mut app.home.auto_scroll_logs, "Tự cuộn xuống cuối");
        if ui.button("📋 Sao chép toàn bộ").clicked() {
            ui.ctx().copy_text(lines.join("\n"));
            app.push_toast(crate::state::ToastKind::Info, "Đã sao chép nhật ký vào Clipboard.");
        }
    });

    ui.add_space(6.0);
    let filter_low = app.home.log_filter.trim().to_ascii_lowercase();
    let filtered: Vec<&String> = lines
        .iter()
        .filter(|l| filter_low.is_empty() || l.to_ascii_lowercase().contains(&filter_low))
        .collect();

    Frame::canvas(ui.style())
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            let row_h = 16.5;
            ScrollArea::vertical()
                .id_salt("virtualized_game_logs")
                .stick_to_bottom(app.home.auto_scroll_logs)
                .auto_shrink([false, false])
                .show_rows(ui, row_h, filtered.len(), |ui, range| {
                    for idx in range {
                        let line = filtered[idx];
                        let col = if line.contains("ERROR") || line.contains("Exception") {
                            ACCENT_RED
                        } else if line.contains("WARN") {
                            ACCENT_AMBER
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.label(RichText::new(line).monospace().size(11.5).color(col));
                    }
                });
        });
}

fn render_tab_analytics(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
) {
    ui.columns(3, |cols| {
        Frame::group(cols[0].style())
            .inner_margin(Margin::same(12))
            .show(&mut cols[0], |ui| {
                ui.label(RichText::new("Tổng thời gian chơi").weak());
                ui.heading(
                    RichText::new(format_duration_hms(profile.total_play_seconds))
                        .color(ACCENT_EMERALD),
                );
            });
        Frame::group(cols[1].style())
            .inner_margin(Margin::same(12))
            .show(&mut cols[1], |ui| {
                ui.label(RichText::new("Tổng số phiên mở game").weak());
                ui.heading(RichText::new(profile.session_count.to_string()).color(ACCENT_BLUE));
            });
        Frame::group(cols[2].style())
            .inner_margin(Margin::same(12))
            .show(&mut cols[2], |ui| {
                ui.label(RichText::new("Số lần Crash ghi nhận").weak());
                ui.heading(RichText::new(profile.crash_count.to_string()).color(ACCENT_RED));
            });
    });

    ui.add_space(12.0);
    Frame::group(ui.style())
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.label(RichText::new("Biểu đồ giờ chơi 14 ngày gần nhất").strong());
            ui.add_space(8.0);
            let stats = &app.home.daily_stats;
            if stats.is_empty() {
                ui.label(RichText::new("Chưa có dữ liệu phiên chơi theo ngày.").weak());
                return;
            }

            let desired = Vec2::new(ui.available_width(), 165.0);
            let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
            let painter = ui.painter_at(rect);
            let max_secs = stats
                .iter()
                .map(|s| s.seconds_played)
                .max()
                .unwrap_or(1)
                .max(60) as f32;

            let n = stats.len().max(1) as f32;
            let slot_w = rect.width() / n;
            let bar_w = (slot_w * 0.62).clamp(10.0, 44.0);

            for (i, st) in stats.iter().enumerate() {
                let ratio = (st.seconds_played as f32 / max_secs).clamp(0.02, 1.0);
                let h = (rect.height() - 26.0) * ratio;
                let cx = rect.left() + slot_w * (i as f32 + 0.5);
                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(cx - bar_w * 0.5, rect.bottom() - 20.0 - h),
                    egui::pos2(cx + bar_w * 0.5, rect.bottom() - 20.0),
                );
                painter.rect_filled(bar_rect, CornerRadius::same(4), ACCENT_EMERALD);
                let short_day = st
                    .day_yyyy_mm_dd
                    .get(5..)
                    .unwrap_or(&st.day_yyyy_mm_dd);
                painter.text(
                    egui::pos2(cx, rect.bottom() - 8.0),
                    egui::Align2::CENTER_CENTER,
                    short_day,
                    egui::FontId::proportional(10.5),
                    ui.visuals().text_color(),
                );
            }
        });
}
