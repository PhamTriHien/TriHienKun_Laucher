use crate::fx3d::{
    gaming_card_3d, paint_3d_isometric_bars, paint_icon_3d, sliding_pill_tabs, tactile_button_3d,
    ButtonStyle3d, Icon3dKind, VectorIcon,
};
use crate::state::{HomeTab, LauncherAppState};
use crate::theme::{
    ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_ORANGE, ACCENT_RED,
    ACCENT_VIOLET,
};
use crate::views::{
    accent_icon_button, badge, danger_icon_button, format_bytes, format_duration_hms,
    primary_icon_button, render_motd_spans, secondary_icon_button,
};
use egui::{vec2, Color32, Frame, Margin, RichText, ScrollArea, Sense, Vec2};
use thk_core::models::{ContentKind, LoaderType, ProviderSource};
use thk_launcher::InstanceKey;

fn loader_accent_color(loader: LoaderType) -> Color32 {
    match loader {
        LoaderType::Fabric => ACCENT_EMERALD,
        LoaderType::NeoForge => ACCENT_ORANGE,
        LoaderType::Forge => ACCENT_AMBER,
        LoaderType::Quilt => ACCENT_VIOLET,
        LoaderType::Vanilla => ACCENT_CYAN,
    }
}

pub fn render_home_view(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    render_state: Option<&egui_wgpu::RenderState>,
) {
    egui::Panel::left("thk_home_profiles_sidebar")
        .resizable(true)
        .default_size(262.0)
        .size_range(215.0..=345.0)
        .frame(Frame::NONE.inner_margin(Margin {
            left: 0,
            right: 12,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.heading(RichText::new("Profiles").size(17.5).strong());
            ui.add_space(6.0);

            let btn_w = ui.available_width();
            if tactile_button_3d(
                ui,
                Some(VectorIcon::Plus),
                app.i18n.t("home.new_profile"),
                ButtonStyle3d::PrimaryEmerald,
                vec2(btn_w, 34.0),
            )
            .clicked()
            {
                app.click_sfx();
                let mc = app.home.new_draft.game_version.clone();
                let ldr = app.home.new_draft.loader;
                app.ensure_loader_versions(&mc, ldr);
                app.show_new_profile_modal = true;
            }

            ui.add_space(4.0);
            if tactile_button_3d(
                ui,
                Some(VectorIcon::Download),
                app.i18n.t("home.import_pack"),
                ButtonStyle3d::SecondaryGlass,
                vec2(btn_w, 32.0),
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
                        let ldr_col = loader_accent_color(prof.loader);

                        let (_, card_resp) = gaming_card_3d(
                            ui,
                            ("home_prof_card", &prof.id),
                            is_sel,
                            ldr_col,
                            Margin::symmetric(10, 8),
                            |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new(&prof.name).strong().size(13.6));
                                ui.add_space(2.0);
                                ui.horizontal_wrapped(|ui| {
                                    badge(ui, &prof.game_version, ACCENT_BLUE);
                                    badge(ui, prof.loader.display_name(), ldr_col);
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
                            },
                        );

                        if card_resp.interact(Sense::click()).clicked() {
                            app.click_sfx();
                            app.select_profile(prof.id);
                        }
                        ui.add_space(5.0);
                    }
                });
        });

    let Some(profile) = app.selected_profile().cloned() else {
        ui.vertical_centered(|ui| {
            ui.add_space(56.0);
            paint_icon_3d(ui, 76.0, Icon3dKind::BrandVoxelCore, true);
            ui.add_space(10.0);
            ui.heading(RichText::new(app.i18n.t("app.name")).size(22.0).strong());
            ui.label(RichText::new(app.i18n.t("home.no_profiles")).weak());
            ui.add_space(14.0);
            if primary_icon_button(ui, VectorIcon::Plus, app.i18n.t("home.new_profile")).clicked() {
                app.show_new_profile_modal = true;
            }
        });
        return;
    };

    render_hero_launch_banner(app, ui, &profile, render_state);
    ui.add_space(10.0);
    render_profile_tabs_bar(app, ui);
    ui.add_space(8.0);

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
    render_state: Option<&egui_wgpu::RenderState>,
) {
    app.sync_3d_viewport_textures(render_state);
    let hero_accent = loader_accent_color(profile.loader);

    gaming_card_3d(
        ui,
        ("home_hero_arena", &profile.id),
        true,
        hero_accent,
        Margin::symmetric(16, 12),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if let (Some(rs), Some(vp)) = (render_state, app.accounts.skin_viewport.as_mut()) {
                    vp.show(ui, rs, vec2(142.0, 154.0));
                    ui.add_space(8.0);
                }

                let right_reserve = 208.0;
                let mid_w = (ui.available_width() - right_reserve).max(180.0);

                ui.allocate_ui_with_layout(
                    vec2(mid_w, 150.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.horizontal(|ui| {
                            paint_icon_3d(ui, 32.0, Icon3dKind::LaunchPrism, true);
                            ui.heading(RichText::new(&profile.name).size(22.0).strong());
                        });

                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            badge(ui, &format!("MC {}", profile.game_version), ACCENT_BLUE);
                            let ldr_txt = match profile.loader_version {
                                Some(ref lv) => format!("{} {lv}", profile.loader.display_name()),
                                None => profile.loader.display_name().to_string(),
                            };
                            badge(ui, &ldr_txt, hero_accent);
                            badge(ui, &format!("{} MB RAM", profile.ram_mb), ACCENT_AMBER);
                            if profile.total_play_seconds > 0 {
                                badge(
                                    ui,
                                    &format_duration_hms(profile.total_play_seconds),
                                    ACCENT_CYAN,
                                );
                            }
                        });

                        ui.add_space(10.0);
                        ui.horizontal_wrapped(|ui| {
                            let selected_acct = app.selected_account().cloned();
                            let accts = app.accounts.accounts.clone();
                            let current_name = selected_acct
                                .map(|a| format!("{} ({})", a.username, a.account_type.as_str()))
                                .unwrap_or_else(|| {
                                    app.i18n
                                        .tr("Chọn tài khoản chơi", "Select Account")
                                        .to_string()
                                });

                            egui::ComboBox::from_id_salt("hero_account_combo")
                                .width(190.0)
                                .selected_text(current_name)
                                .show_ui(ui, |ui| {
                                    for a in accts {
                                        let label =
                                            format!("{} ({})", a.username, a.account_type.as_str());
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

                            if secondary_icon_button(
                                ui,
                                VectorIcon::Folder,
                                app.i18n.t("home.open_folder"),
                            )
                            .clicked()
                            {
                                let _ = std::fs::create_dir_all(&profile.instance_path);
                                let _ = open::that(&profile.instance_path);
                            }
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
                                    let pid_txt =
                                        s.pid.map(|p| format!("PID {p}")).unwrap_or_default();
                                    badge(
                                        ui,
                                        &format!(
                                            "LIVE: {uname} ({pid_txt} • {})",
                                            format_duration_hms(s.uptime_secs)
                                        ),
                                        ACCENT_EMERALD,
                                    );
                                }
                            });
                        }
                    },
                );

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
                            if tactile_button_3d(
                                ui,
                                Some(VectorIcon::Stop),
                                app.i18n.t("home.stop"),
                                ButtonStyle3d::HeroStop,
                                vec2(182.0, 50.0),
                            )
                            .clicked()
                            {
                                app.instance_supervisor.stop_instance(&key);
                            }
                        }
                    } else if tactile_button_3d(
                        ui,
                        Some(VectorIcon::Play),
                        app.i18n.t("home.play"),
                        ButtonStyle3d::HeroLaunch,
                        vec2(194.0, 50.0),
                    )
                    .clicked()
                    {
                        app.launch_selected_profile(None);
                    }
                });
            });
        },
    );
}

fn render_profile_tabs_bar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let tabs = [
        (
            HomeTab::General,
            VectorIcon::Sliders,
            app.i18n.t("home.tab.general").to_string(),
        ),
        (
            HomeTab::Mods,
            VectorIcon::Cube,
            app.i18n.t("home.tab.mods").to_string(),
        ),
        (
            HomeTab::Shaders,
            VectorIcon::Sparkles,
            app.i18n.t("home.tab.shaders").to_string(),
        ),
        (
            HomeTab::ResourcePacks,
            VectorIcon::Folder,
            app.i18n.t("home.tab.resourcepacks").to_string(),
        ),
        (
            HomeTab::Worlds,
            VectorIcon::Globe,
            app.i18n.t("home.tab.worlds").to_string(),
        ),
        (
            HomeTab::Bookmarks,
            VectorIcon::Play,
            app.i18n.t("home.tab.bookmarks").to_string(),
        ),
        (
            HomeTab::Logs,
            VectorIcon::Terminal,
            app.i18n.t("home.tab.logs").to_string(),
        ),
        (
            HomeTab::Analytics,
            VectorIcon::Chart,
            app.i18n.t("home.tab.analytics").to_string(),
        ),
    ];

    if let Some(next_tab) = sliding_pill_tabs(ui, "home_profile_tabs_pill", &tabs, app.home.active_tab) {
        app.click_sfx();
        app.home.active_tab = next_tab;
        app.home.show_online_browser = false;
        app.refresh_selected_profile_tabs();
        if next_tab == HomeTab::Bookmarks {
            app.ping_all_bookmarks();
        }
    }
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
            gaming_card_3d(
                ui,
                "home_general_cfg_card",
                false,
                ACCENT_EMERALD,
                Margin::same(16),
                |ui| {
                    ui.set_width(ui.available_width());
                    egui::Grid::new("profile_edit_grid")
                        .num_columns(2)
                        .spacing([16.0, 11.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new(app.i18n.t("profile.name")).strong());
                            ui.add(
                                egui::TextEdit::singleline(&mut app.home.edit_draft.name)
                                    .desired_width(340.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new(app.i18n.t("profile.ram")).strong());
                            ui.add(
                                egui::Slider::new(&mut app.home.edit_draft.ram_mb, 1024..=32768)
                                    .step_by(256.0)
                                    .suffix(" MB"),
                            );
                            ui.end_row();

                            ui.label(RichText::new(app.i18n.t("profile.java_path")).strong());
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut app.home.edit_draft.java_path_str,
                                    )
                                    .hint_text("Mặc định tự động chọn Mojang Java chuẩn")
                                    .desired_width(280.0),
                                );
                                if secondary_icon_button(ui, VectorIcon::Folder, "Chọn...").clicked()
                                {
                                    if let Some(f) = rfd::FileDialog::new().pick_file() {
                                        app.home.edit_draft.java_path_str =
                                            f.to_string_lossy().to_string();
                                    }
                                }
                                if secondary_icon_button(ui, VectorIcon::Refresh, "Mặc định").clicked()
                                {
                                    app.home.edit_draft.java_path_str.clear();
                                }
                            });
                            ui.end_row();

                            ui.label(RichText::new(app.i18n.t("profile.jvm_args")).strong());
                            ui.add(
                                egui::TextEdit::singleline(&mut app.home.edit_draft.jvm_args)
                                    .hint_text("-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions")
                                    .desired_width(380.0),
                            );
                            ui.end_row();
                        });

                    if profile.loader == LoaderType::Fabric {
                        ui.add_space(8.0);
                        ui.checkbox(
                            &mut app.home.edit_draft.auto_perf_mods,
                            app.i18n.t("profile.auto_perf"),
                        );
                    }

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if primary_icon_button(ui, VectorIcon::Save, app.i18n.t("profile.save"))
                            .clicked()
                        {
                            app.save_profile_edits();
                        }
                        if danger_icon_button(ui, VectorIcon::Trash, app.i18n.t("profile.delete"))
                            .clicked()
                        {
                            app.delete_selected_profile();
                        }
                    });
                },
            );
        });
}

fn render_tab_content(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
    kind: ContentKind,
) {
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.home.local_filter)
                .hint_text("Lọc tệp đã cài đặt...")
                .desired_width(220.0),
        );
        if secondary_icon_button(ui, VectorIcon::Refresh, "Làm mới").clicked() {
            app.refresh_selected_profile_tabs();
        }
        if secondary_icon_button(ui, VectorIcon::Folder, "Mở thư mục").clicked() {
            if let Ok(sub) = thk_core::SafePath::resolve_child(
                &profile.instance_path,
                kind.instance_subdir(),
            ) {
                let _ = std::fs::create_dir_all(&sub);
                let _ = open::that(&sub);
            }
        }
        let toggle_label = if app.home.show_online_browser {
            "Đóng kho tải trực tuyến"
        } else {
            "Tìm & Cài trực tuyến (Modrinth / CurseForge)"
        };
        if accent_icon_button(ui, VectorIcon::Globe, toggle_label).clicked() {
            app.home.show_online_browser = !app.home.show_online_browser;
            if app.home.show_online_browser && app.home.online_hits.is_empty() {
                app.search_inline_profile_content(kind);
            }
        }
    });

    ui.add_space(8.0);

    if app.home.show_online_browser {
        gaming_card_3d(
            ui,
            "inline_online_browser_card",
            true,
            ACCENT_VIOLET,
            Margin::same(12),
            |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
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
                    if primary_icon_button(ui, VectorIcon::Search, "Tìm kiếm").clicked() {
                        app.search_inline_profile_content(kind);
                    }
                    if app.home.online_selected.is_some()
                        && secondary_icon_button(ui, VectorIcon::ArrowLeft, "Quay lại kết quả")
                            .clicked()
                    {
                        app.home.online_selected = None;
                    }
                });

                ui.add_space(8.0);
                ScrollArea::vertical()
                    .id_salt("inline_online_results")
                    .max_height(215.0)
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
                                    if primary_icon_button(
                                        ui,
                                        VectorIcon::Download,
                                        app.i18n.t("mods.install"),
                                    )
                                    .clicked()
                                    {
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
                                let item_id = item.project_id.clone();
                                gaming_card_3d(
                                    ui,
                                    ("inline_hit", item_id),
                                    false,
                                    ACCENT_CYAN,
                                    Margin::symmetric(10, 7),
                                    |ui| {
                                        ui.set_width(ui.available_width());
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
                                                    if primary_icon_button(
                                                        ui,
                                                        VectorIcon::ArrowRight,
                                                        "Chọn bản cài",
                                                    )
                                                    .clicked()
                                                    {
                                                        app.load_inline_project_versions(item, kind);
                                                    }
                                                },
                                            );
                                        });
                                    },
                                );
                                ui.add_space(4.0);
                            }
                        }
                    });
            },
        );
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
                gaming_card_3d(
                    ui,
                    ("local_item_card", &item.file_name),
                    false,
                    if item.enabled {
                        ACCENT_EMERALD
                    } else {
                        Color32::GRAY
                    },
                    Margin::symmetric(12, 8),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let mut enabled = item.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                app.toggle_local_item(kind, &item.file_name, enabled);
                            }
                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
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
                                    if danger_icon_button(ui, VectorIcon::Trash, "Xóa").clicked() {
                                        app.delete_local_item(kind, &item.file_name);
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
                gaming_card_3d(
                    ui,
                    ("world_card", &w.folder_name),
                    false,
                    ACCENT_CYAN,
                    Margin::symmetric(12, 9),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&w.folder_name).strong().size(14.0));
                                ui.label(
                                    RichText::new(format!(
                                        "Dung lượng: {}",
                                        format_bytes(w.size_bytes)
                                    ))
                                    .size(12.0)
                                    .weak(),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if danger_icon_button(ui, VectorIcon::Trash, "Xóa").clicked() {
                                        app.delete_world_folder(&w.folder_name);
                                    }
                                    if secondary_icon_button(ui, VectorIcon::Folder, "Mở").clicked()
                                    {
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
                    },
                );
                ui.add_space(5.0);
            }
        });
}

fn render_tab_bookmarks(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
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
        if primary_icon_button(ui, VectorIcon::Plus, "Thêm Server").clicked() {
            app.add_bookmark();
        }
        if secondary_icon_button(ui, VectorIcon::Refresh, "Ping tất cả").clicked() {
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
                let is_online = matches!(pings.get(&bm.id), Some(Ok(_)));
                gaming_card_3d(
                    ui,
                    ("bm_card", &bm.id),
                    is_online,
                    if is_online { ACCENT_EMERALD } else { ACCENT_BLUE },
                    Margin::symmetric(14, 10),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&bm.name).strong().size(14.5));
                                    ui.label(RichText::new(&bm.address).monospace().weak());
                                });
                                match pings.get(&bm.id) {
                                    Some(Ok(st)) => {
                                        ui.horizontal_wrapped(|ui| {
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
                                        ui.label(
                                            RichText::new("Đang chờ Ping...").size(12.0).weak(),
                                        );
                                    }
                                }
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if danger_icon_button(ui, VectorIcon::Trash, "Xóa").clicked() {
                                        app.delete_bookmark(&bm.id);
                                    }
                                    if primary_icon_button(ui, VectorIcon::Play, "Vào ngay").clicked()
                                    {
                                        app.launch_selected_profile(Some(bm.address.clone()));
                                    }
                                },
                            );
                        });
                    },
                );
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

    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.home.log_filter)
                .hint_text("Lọc dòng nhật ký...")
                .desired_width(240.0),
        );
        ui.checkbox(&mut app.home.auto_scroll_logs, "Tự cuộn xuống cuối");
        if secondary_icon_button(ui, VectorIcon::Copy, "Sao chép toàn bộ").clicked() {
            ui.ctx().copy_text(lines.join("\n"));
            app.push_toast(
                crate::state::ToastKind::Info,
                "Đã sao chép nhật ký vào Clipboard.",
            );
        }
    });

    ui.add_space(6.0);
    let filter_low = app.home.log_filter.trim().to_ascii_lowercase();
    let filtered: Vec<&String> = lines
        .iter()
        .filter(|l| filter_low.is_empty() || l.to_ascii_lowercase().contains(&filter_low))
        .collect();

    Frame::canvas(ui.style())
        .inner_margin(Margin::same(10))
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
        gaming_card_3d(
            &mut cols[0],
            "kpi_playtime",
            false,
            ACCENT_EMERALD,
            Margin::same(14),
            |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Tổng thời gian chơi").weak());
                ui.heading(
                    RichText::new(format_duration_hms(profile.total_play_seconds))
                        .size(22.0)
                        .strong()
                        .color(ACCENT_EMERALD),
                );
            },
        );
        gaming_card_3d(
            &mut cols[1],
            "kpi_sessions",
            false,
            ACCENT_CYAN,
            Margin::same(14),
            |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Tổng số phiên mở game").weak());
                ui.heading(
                    RichText::new(profile.session_count.to_string())
                        .size(22.0)
                        .strong()
                        .color(ACCENT_CYAN),
                );
            },
        );
        gaming_card_3d(
            &mut cols[2],
            "kpi_crashes",
            false,
            ACCENT_RED,
            Margin::same(14),
            |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Số lần Crash ghi nhận").weak());
                ui.heading(
                    RichText::new(profile.crash_count.to_string())
                        .size(22.0)
                        .strong()
                        .color(ACCENT_RED),
                );
            },
        );
    });

    ui.add_space(12.0);
    gaming_card_3d(
        ui,
        "analytics_3d_bars_card",
        false,
        ACCENT_EMERALD,
        Margin::same(16),
        |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new("Biểu đồ Voxel 3D giờ chơi 14 ngày gần nhất")
                    .size(14.5)
                    .strong(),
            );
            ui.add_space(8.0);
            let stats = &app.home.daily_stats;
            if stats.is_empty() {
                ui.label(RichText::new("Chưa có dữ liệu phiên chơi theo ngày.").weak());
                return;
            }

            let bars: Vec<(String, u64, String)> = stats
                .iter()
                .map(|st| {
                    let short_day = st
                        .day_yyyy_mm_dd
                        .get(5..)
                        .unwrap_or(&st.day_yyyy_mm_dd)
                        .to_string();
                    let tip = format!(
                        "{}: {}",
                        st.day_yyyy_mm_dd,
                        format_duration_hms(st.seconds_played)
                    );
                    (short_day, st.seconds_played, tip)
                })
                .collect();

            paint_3d_isometric_bars(ui, Vec2::new(1.0, 185.0).y, &bars, ACCENT_EMERALD);
        },
    );
}
