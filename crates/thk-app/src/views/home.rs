use crate::fx3d::{
    gaming_card_3d, paint_3d_isometric_bars, paint_icon_3d, sliding_pill_tabs, tactile_button_3d,
    ButtonStyle3d, Icon3dKind, VectorIcon,
};
use crate::state::{HomeTab, LauncherAppState, ToastKind};
use crate::theme::{
    ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_ORANGE, ACCENT_RED,
    ACCENT_VIOLET,
};
use crate::views::{
    accent_icon_button, badge, danger_icon_button, format_bytes, format_duration_hms,
    primary_icon_button, render_motd_spans, secondary_button, secondary_icon_button,
};
use egui::{
    pos2, vec2, Color32, CornerRadius, Frame, Margin, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, Vec2,
};
use std::collections::{HashMap, HashSet};
use thk_core::models::{ContentKind, Language, LoaderType, ProviderSource};
use thk_launcher::InstanceKey;
use thk_mods::ContentUpdateCandidate;

fn loader_accent_color(loader: LoaderType) -> Color32 {
    match loader {
        LoaderType::Fabric => ACCENT_EMERALD,
        LoaderType::NeoForge => ACCENT_ORANGE,
        LoaderType::Forge => ACCENT_AMBER,
        LoaderType::Quilt => ACCENT_VIOLET,
        LoaderType::Vanilla => ACCENT_CYAN,
    }
}

fn channel_badge_color(channel: &str) -> Color32 {
    if channel.eq_ignore_ascii_case("beta") {
        ACCENT_AMBER
    } else if channel.eq_ignore_ascii_case("alpha") {
        ACCENT_RED
    } else {
        ACCENT_EMERALD
    }
}

fn paint_square_thumbnail(
    ui: &mut egui::Ui,
    tex_opt: Option<&egui::TextureHandle>,
    size: f32,
    accent: Color32,
    fallback_kind: Icon3dKind,
    active_fallback: bool,
) {
    if let Some(tex) = tex_opt {
        let radius = CornerRadius::same((size * 0.22).round().clamp(5.0, 10.0) as u8);
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
        paint_icon_3d(ui, size, fallback_kind, active_fallback);
    }
}

fn is_editable_profile_text_file(lower_name: &str) -> bool {
    [
        ".properties",
        ".yml",
        ".yaml",
        ".json",
        ".json5",
        ".toml",
        ".txt",
        ".conf",
        ".cfg",
        ".ini",
        ".log",
        ".mcmeta",
        ".snbt",
        ".lang",
    ]
    .iter()
    .any(|ext| lower_name.ends_with(ext))
}

pub fn render_home_view(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    render_state: Option<&egui_wgpu::RenderState>,
) {
    egui::Panel::left("thk_home_profiles_sidebar")
        .resizable(true)
        .default_size(266.0)
        .size_range(218.0..=350.0)
        .frame(Frame::NONE.inner_margin(Margin {
            left: 0,
            right: 12,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.heading(
                RichText::new(app.i18n.tr("Danh sách Profile", "Profiles"))
                    .size(17.5)
                    .strong(),
            );
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
                        let icon_tex = prof
                            .icon_url
                            .as_deref()
                            .and_then(|u| app.request_modpack_texture(u, 64));

                        let (_, card_resp) = gaming_card_3d(
                            ui,
                            ("home_prof_card", &prof.id),
                            is_sel,
                            ldr_col,
                            Margin::symmetric(10, 8),
                            |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    paint_square_thumbnail(
                                        ui,
                                        icon_tex.as_ref(),
                                        32.0,
                                        ldr_col,
                                        Icon3dKind::LaunchPrism,
                                        is_sel,
                                    );
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(&prof.name).strong().size(13.5));
                                        ui.horizontal_wrapped(|ui| {
                                            badge(ui, &prof.game_version, ACCENT_BLUE);
                                            badge(ui, prof.loader.display_name(), ldr_col);
                                            if prof.total_play_seconds > 0 {
                                                ui.label(
                                                    RichText::new(format_duration_hms(
                                                        prof.total_play_seconds,
                                                    ))
                                                    .size(10.8)
                                                    .weak(),
                                                );
                                            }
                                        });
                                    });
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
        HomeTab::Files => render_tab_files(app, ui, &profile),
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
    let hero_icon_tex = profile
        .icon_url
        .as_deref()
        .and_then(|u| app.request_modpack_texture(u, 96));

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
                            paint_square_thumbnail(
                                ui,
                                hero_icon_tex.as_ref(),
                                38.0,
                                hero_accent,
                                Icon3dKind::LaunchPrism,
                                true,
                            );
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
                            if profile.fullscreen {
                                badge(
                                    ui,
                                    app.i18n.tr("Toàn màn hình", "Fullscreen"),
                                    ACCENT_VIOLET,
                                );
                            } else if let (Some(w), Some(h)) =
                                (profile.window_width, profile.window_height)
                            {
                                badge(ui, &format!("{w}×{h}"), ACCENT_VIOLET);
                            }
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
            VectorIcon::Image,
            app.i18n.t("home.tab.resourcepacks").to_string(),
        ),
        (
            HomeTab::Worlds,
            VectorIcon::Globe,
            app.i18n.t("home.tab.worlds").to_string(),
        ),
        (
            HomeTab::Files,
            VectorIcon::Folder,
            app.i18n.t("home.tab.files").to_string(),
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

    if let Some(next_tab) =
        sliding_pill_tabs(ui, "home_profile_tabs_pill", &tabs, app.home.active_tab)
    {
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
                                    .hint_text(app.i18n.tr(
                                        "Mặc định tự động chọn Mojang Java chuẩn",
                                        "Auto-select compatible Mojang Java runtime",
                                    ))
                                    .desired_width(280.0),
                                );
                                if secondary_icon_button(
                                    ui,
                                    VectorIcon::Folder,
                                    app.i18n.tr("Chọn...", "Browse..."),
                                )
                                .clicked()
                                {
                                    if let Some(f) = rfd::FileDialog::new().pick_file() {
                                        app.home.edit_draft.java_path_str =
                                            f.to_string_lossy().to_string();
                                    }
                                }
                                if secondary_icon_button(
                                    ui,
                                    VectorIcon::Refresh,
                                    app.i18n.tr("Mặc định", "Auto"),
                                )
                                .clicked()
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

                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Cửa sổ & Độ phân giải",
                                    "Window & Resolution",
                                ))
                                .strong(),
                            );
                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.checkbox(
                                        &mut app.home.edit_draft.fullscreen,
                                        app.i18n.tr(
                                            "Khởi động Toàn màn hình (Fullscreen)",
                                            "Launch in Fullscreen",
                                        ),
                                    );
                                    ui.checkbox(
                                        &mut app.home.edit_draft.use_custom_resolution,
                                        app.i18n.tr(
                                            "Độ phân giải cửa sổ tùy chỉnh",
                                            "Custom Window Resolution",
                                        ),
                                    );
                                });
                                if app.home.edit_draft.use_custom_resolution {
                                    ui.add_space(4.0);
                                    ui.horizontal_wrapped(|ui| {
                                        ui.add(
                                            egui::DragValue::new(
                                                &mut app.home.edit_draft.window_width,
                                            )
                                            .range(640..=7680)
                                            .suffix(" px"),
                                        );
                                        ui.label("×");
                                        ui.add(
                                            egui::DragValue::new(
                                                &mut app.home.edit_draft.window_height,
                                            )
                                            .range(480..=4320)
                                            .suffix(" px"),
                                        );
                                        for (w, h, lbl) in [
                                            (1280, 720, "720p"),
                                            (1600, 900, "900p"),
                                            (1920, 1080, "1080p"),
                                            (2560, 1440, "1440p"),
                                        ] {
                                            if secondary_button(ui, lbl).clicked() {
                                                app.home.edit_draft.window_width = w;
                                                app.home.edit_draft.window_height = h;
                                            }
                                        }
                                    });
                                }
                            });
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
                    ui.horizontal_wrapped(|ui| {
                        if primary_icon_button(ui, VectorIcon::Save, app.i18n.t("profile.save"))
                            .clicked()
                        {
                            app.save_profile_edits();
                        }
                        if secondary_icon_button(
                            ui,
                            VectorIcon::Copy,
                            app.i18n.tr("Nhân bản Profile", "Duplicate Profile"),
                        )
                        .clicked()
                        {
                            app.duplicate_selected_profile();
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
    let update_count = app.home.content_updates.len();

    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.home.local_filter)
                .hint_text(
                    app.i18n
                        .tr("Lọc tệp đã cài đặt...", "Filter installed files..."),
                )
                .desired_width(210.0),
        );
        if secondary_icon_button(ui, VectorIcon::Refresh, app.i18n.tr("Làm mới", "Refresh"))
            .clicked()
        {
            app.refresh_selected_profile_tabs();
        }
        if secondary_icon_button(ui, VectorIcon::Folder, app.i18n.t("home.open_folder")).clicked() {
            if let Ok(sub) = thk_core::SafePath::resolve_child(
                &profile.instance_path,
                kind.instance_subdir(),
            ) {
                let _ = std::fs::create_dir_all(&sub);
                let _ = open::that(&sub);
            }
        }

        if app.home.checking_updates {
            ui.spinner();
            ui.label(
                RichText::new(app.i18n.tr("Đang kiểm tra...", "Checking..."))
                    .size(12.0)
                    .color(ACCENT_CYAN),
            );
        } else if secondary_icon_button(
            ui,
            VectorIcon::Download,
            app.i18n.tr("Kiểm tra cập nhật", "Check Updates"),
        )
        .clicked()
        {
            app.check_profile_content_updates(kind);
        }

        if update_count > 0 {
            let upd_all_lbl = if app.settings.language == Language::Vi {
                format!("Cập nhật tất cả ({update_count})")
            } else {
                format!("Update All ({update_count})")
            };
            if primary_icon_button(ui, VectorIcon::Sparkles, upd_all_lbl).clicked() {
                app.apply_all_content_updates(kind);
            }
        }

        let toggle_label = if app.home.show_online_browser {
            app.i18n
                .tr("Đóng kho trực tuyến", "Close Online Browser")
        } else {
            app.i18n.tr(
                "Tải thêm trực tuyến (Modrinth / CurseForge)",
                "Browse Online (Modrinth / CurseForge)",
            )
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
        render_inline_online_browser(app, ui, profile, kind);
        ui.add_space(8.0);
    }

    let filter_low = app.home.local_filter.trim().to_ascii_lowercase();
    let local_snapshot = app.home.local_items.clone();
    let updates_by_file: HashMap<String, ContentUpdateCandidate> = app
        .home
        .content_updates
        .iter()
        .flat_map(|c| {
            [
                (c.file_name.clone(), c.clone()),
                (
                    c.file_name
                        .strip_suffix(".disabled")
                        .unwrap_or(&c.file_name)
                        .to_string(),
                    c.clone(),
                ),
            ]
        })
        .collect();

    ScrollArea::vertical()
        .id_salt("local_installed_content_scroll")
        .show(ui, |ui| {
            if local_snapshot.is_empty() {
                ui.label(
                    RichText::new(app.i18n.tr(
                        "Chưa có tệp nào trong thư mục này.",
                        "No installed packages found in this folder.",
                    ))
                    .weak(),
                );
                return;
            }
            for item in local_snapshot {
                if !filter_low.is_empty()
                    && !item.title.to_ascii_lowercase().contains(&filter_low)
                    && !item.file_name.to_ascii_lowercase().contains(&filter_low)
                {
                    continue;
                }
                let item_tex = item
                    .icon_url
                    .as_deref()
                    .and_then(|u| app.request_modpack_texture(u, 64));
                let upd_cand = updates_by_file
                    .get(&item.file_name)
                    .or_else(|| updates_by_file.get(&item.base_file_name))
                    .cloned();
                let card_accent = if upd_cand.is_some() {
                    ACCENT_AMBER
                } else if item.enabled {
                    ACCENT_EMERALD
                } else {
                    Color32::GRAY
                };

                gaming_card_3d(
                    ui,
                    ("local_item_card", &item.file_name),
                    upd_cand.is_some(),
                    card_accent,
                    Margin::symmetric(12, 8),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let mut enabled = item.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                app.toggle_local_item(kind, &item.file_name, enabled);
                            }
                            paint_square_thumbnail(
                                ui,
                                item_tex.as_ref(),
                                34.0,
                                card_accent,
                                Icon3dKind::ModpackCrate,
                                false,
                            );
                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(&item.title).strong());
                                    if !item.version_number.is_empty() {
                                        badge(ui, &item.version_number, ACCENT_BLUE);
                                    }
                                    badge(ui, item.provider.display_name(), ACCENT_EMERALD);
                                    if let Some(ref cand) = upd_cand {
                                        badge(
                                            ui,
                                            &format!(
                                                "{} → {}",
                                                app.i18n.t("mods.update"),
                                                cand.latest_version.version_number
                                            ),
                                            ACCENT_AMBER,
                                        );
                                    }
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
                                    if danger_icon_button(
                                        ui,
                                        VectorIcon::Trash,
                                        app.i18n.tr("Xóa", "Delete"),
                                    )
                                    .clicked()
                                    {
                                        app.delete_local_item(kind, &item.file_name);
                                    }
                                    if let Some(cand) = upd_cand {
                                        let btn_txt = format!(
                                            "{} {}",
                                            app.i18n.t("mods.update"),
                                            cand.latest_version.version_number
                                        );
                                        if primary_icon_button(ui, VectorIcon::Download, btn_txt)
                                            .clicked()
                                        {
                                            app.apply_single_content_update(cand, kind);
                                        }
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

fn render_inline_online_browser(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
    kind: ContentKind,
) {
    let installed_ids: HashSet<String> = app
        .home
        .local_items
        .iter()
        .filter_map(|i| i.project_id.clone())
        .collect();
    let installed_titles: HashSet<String> = app
        .home
        .local_items
        .iter()
        .map(|i| i.title.to_ascii_lowercase())
        .collect();

    gaming_card_3d(
        ui,
        "inline_online_browser_card",
        true,
        ACCENT_VIOLET,
        Margin::same(12),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(
                        app.home.online_provider == ProviderSource::Modrinth,
                        "Modrinth",
                    )
                    .clicked()
                    && app.home.online_provider != ProviderSource::Modrinth
                {
                    app.home.online_provider = ProviderSource::Modrinth;
                    app.home.online_selected = None;
                    app.search_inline_profile_content(kind);
                }
                if ui
                    .selectable_label(
                        app.home.online_provider == ProviderSource::CurseForge,
                        "CurseForge",
                    )
                    .clicked()
                    && app.home.online_provider != ProviderSource::CurseForge
                {
                    app.home.online_provider = ProviderSource::CurseForge;
                    app.home.online_selected = None;
                    app.search_inline_profile_content(kind);
                }

                let resp = ui.add(
                    egui::TextEdit::singleline(&mut app.home.online_query)
                        .hint_text(app.i18n.t("mods.search_placeholder"))
                        .desired_width(260.0),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    app.search_inline_profile_content(kind);
                }
                if primary_icon_button(
                    ui,
                    VectorIcon::Search,
                    app.i18n.tr("Tìm kiếm", "Search"),
                )
                .clicked()
                {
                    app.search_inline_profile_content(kind);
                }
                if app.home.online_selected.is_some()
                    && secondary_icon_button(
                        ui,
                        VectorIcon::ArrowLeft,
                        app.i18n.tr("Quay lại kết quả", "Back to Results"),
                    )
                    .clicked()
                {
                    app.home.online_selected = None;
                }
            });

            ui.add_space(8.0);
            ScrollArea::vertical()
                .id_salt("inline_online_results")
                .max_height(245.0)
                .show(ui, |ui| {
                    if let Some((detail, versions)) = app.home.online_selected.clone() {
                        let sel_tex = detail
                            .summary
                            .icon_url
                            .as_deref()
                            .and_then(|u| app.request_modpack_texture(u, 64));

                        ui.horizontal(|ui| {
                            paint_square_thumbnail(
                                ui,
                                sel_tex.as_ref(),
                                36.0,
                                ACCENT_VIOLET,
                                Icon3dKind::ModpackCrate,
                                false,
                            );
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(if app.settings.language == Language::Vi {
                                        format!(
                                            "Chọn phiên bản '{}' tương thích MC {}:",
                                            detail.summary.title, profile.game_version
                                        )
                                    } else {
                                        format!(
                                            "Select '{}' version for MC {}:",
                                            detail.summary.title, profile.game_version
                                        )
                                    })
                                    .strong(),
                                );
                                ui.label(RichText::new(&detail.summary.summary).size(11.5).weak());
                            });
                        });

                        ui.add_space(6.0);
                        if versions.is_empty() {
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Không có bản phát hành tương thích phiên bản/loader này.",
                                    "No compatible builds found for this MC version/loader.",
                                ))
                                .color(ACCENT_RED),
                            );
                            return;
                        }

                        if app.home.online_chosen_ver_idx >= versions.len() {
                            app.home.online_chosen_ver_idx = 0;
                        }
                        let cur_ver = &versions[app.home.online_chosen_ver_idx];
                        let combo_text = format!(
                            "[{}] {} — {}",
                            cur_ver.release_channel.to_uppercase(),
                            cur_ver.version_number,
                            cur_ver.name
                        );
                        ui.horizontal_wrapped(|ui| {
                            egui::ComboBox::from_id_salt("inline_version_picker_combo")
                                .width(340.0)
                                .selected_text(combo_text)
                                .show_ui(ui, |ui| {
                                    for (idx, v) in versions.iter().enumerate() {
                                        let lbl = format!(
                                            "[{}] {} — {}",
                                            v.release_channel.to_uppercase(),
                                            v.version_number,
                                            v.name
                                        );
                                        ui.selectable_value(
                                            &mut app.home.online_chosen_ver_idx,
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
                                let chosen = versions[app.home.online_chosen_ver_idx].clone();
                                app.install_inline_version(detail.summary.clone(), chosen, kind);
                            }
                        });

                        ui.add_space(4.0);
                        for ver in versions.into_iter().take(15) {
                            ui.horizontal(|ui| {
                                badge(
                                    ui,
                                    &ver.release_channel.to_uppercase(),
                                    channel_badge_color(&ver.release_channel),
                                );
                                ui.label(RichText::new(&ver.version_number).strong());
                                ui.label(RichText::new(&ver.name).weak());
                                if secondary_icon_button(
                                    ui,
                                    VectorIcon::Download,
                                    app.i18n.t("mods.install"),
                                )
                                .clicked()
                                {
                                    app.install_inline_version(detail.summary.clone(), ver, kind);
                                }
                            });
                        }
                    } else {
                        let hits = app.home.online_hits.clone();
                        for item in &hits {
                            let item_id = item.project_id.clone();
                            let hit_tex = item
                                .icon_url
                                .as_deref()
                                .and_then(|u| app.request_modpack_texture(u, 64));
                            let is_installed = installed_ids.contains(&item.project_id)
                                || installed_titles.contains(&item.title.to_ascii_lowercase());

                            gaming_card_3d(
                                ui,
                                ("inline_hit", item_id),
                                is_installed,
                                if is_installed {
                                    ACCENT_EMERALD
                                } else {
                                    ACCENT_CYAN
                                },
                                Margin::symmetric(10, 7),
                                |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        paint_square_thumbnail(
                                            ui,
                                            hit_tex.as_ref(),
                                            36.0,
                                            ACCENT_CYAN,
                                            Icon3dKind::ModpackCrate,
                                            false,
                                        );
                                        ui.vertical(|ui| {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label(RichText::new(&item.title).strong());
                                                ui.label(
                                                    RichText::new(format!("by {}", item.author))
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
                                            ui.label(RichText::new(&item.summary).size(12.0));
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if primary_icon_button(
                                                    ui,
                                                    VectorIcon::ArrowRight,
                                                    app.i18n
                                                        .tr("Chọn bản cài", "Select Version"),
                                                )
                                                .clicked()
                                                {
                                                    app.load_inline_project_versions(
                                                        item.clone(),
                                                        kind,
                                                    );
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                            ui.add_space(4.0);
                        }

                        if !hits.is_empty() && hits.len() < app.home.online_total_hits {
                            ui.add_space(4.0);
                            let more_lbl = if app.settings.language == Language::Vi {
                                format!(
                                    "Tải thêm kết quả ({}/{})",
                                    hits.len(),
                                    app.home.online_total_hits
                                )
                            } else {
                                format!(
                                    "Load More ({}/{})",
                                    hits.len(),
                                    app.home.online_total_hits
                                )
                            };
                            if secondary_icon_button(ui, VectorIcon::Download, more_lbl).clicked() {
                                let next_page = app.home.online_page + 1;
                                app.search_inline_profile_content_page(kind, next_page, true);
                            }
                        }
                    }
                });
        },
    );
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
                ui.label(
                    RichText::new(app.i18n.tr(
                        "Chưa có thế giới Singleplayer nào.",
                        "No Singleplayer worlds found yet.",
                    ))
                    .weak(),
                );
                return;
            }
            for w in worlds {
                let world_tex = w
                    .icon_path
                    .as_deref()
                    .and_then(|p| app.request_local_image_texture(p, 64));

                gaming_card_3d(
                    ui,
                    ("world_card", &w.folder_name),
                    false,
                    ACCENT_CYAN,
                    Margin::symmetric(12, 9),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            paint_square_thumbnail(
                                ui,
                                world_tex.as_ref(),
                                40.0,
                                ACCENT_CYAN,
                                Icon3dKind::HomePortal,
                                false,
                            );
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&w.display_name).strong().size(14.0));
                                ui.label(
                                    RichText::new(format!(
                                        "{} • {}",
                                        w.folder_name,
                                        format_bytes(w.size_bytes)
                                    ))
                                    .size(12.0)
                                    .weak(),
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
                                        app.delete_world_folder(&w.folder_name);
                                    }
                                    if secondary_icon_button(
                                        ui,
                                        VectorIcon::Folder,
                                        app.i18n.tr("Mở", "Open"),
                                    )
                                    .clicked()
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

fn render_tab_files(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
) {
    ui.horizontal_wrapped(|ui| {
        if secondary_icon_button(
            ui,
            VectorIcon::Folder,
            app.i18n.tr("Gốc (/)", "Root (/)"),
        )
        .clicked()
        {
            app.home.current_rel_dir.clear();
            app.home.selected_entries.clear();
            app.refresh_selected_profile_tabs();
        }
        if !app.home.current_rel_dir.is_empty()
            && secondary_icon_button(
                ui,
                VectorIcon::ArrowLeft,
                app.i18n.tr("Lên cấp trên", "Up"),
            )
            .clicked()
        {
            let parent = app
                .home
                .current_rel_dir
                .rsplit_once('/')
                .map(|(p, _)| p.to_string())
                .unwrap_or_default();
            app.home.current_rel_dir = parent;
            app.home.selected_entries.clear();
            app.refresh_selected_profile_tabs();
        }

        let display_rel = if app.home.current_rel_dir.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", app.home.current_rel_dir)
        };
        badge(ui, &display_rel, ACCENT_BLUE);

        if secondary_icon_button(ui, VectorIcon::Refresh, app.i18n.tr("Làm mới", "Refresh"))
            .clicked()
        {
            app.refresh_selected_profile_tabs();
        }
        if primary_icon_button(
            ui,
            VectorIcon::Upload,
            app.i18n.tr("Tải tệp lên", "Upload File"),
        )
        .clicked()
        {
            app.import_external_file_into_profile();
        }
        if secondary_icon_button(ui, VectorIcon::Folder, app.i18n.t("home.open_folder")).clicked() {
            let target = if app.home.current_rel_dir.is_empty() {
                profile.instance_path.clone()
            } else {
                thk_core::SafePath::resolve_child(&profile.instance_path, &app.home.current_rel_dir)
                    .unwrap_or_else(|_| profile.instance_path.clone())
            };
            let _ = std::fs::create_dir_all(&target);
            let _ = open::that(&target);
        }

        ui.separator();
        ui.add(
            egui::TextEdit::singleline(&mut app.home.archive_name_input)
                .hint_text("instance_backup.zip")
                .desired_width(135.0),
        );
        if secondary_icon_button(
            ui,
            VectorIcon::Cube,
            app.i18n.tr("Nén .zip", "Compress .zip"),
        )
        .clicked()
        {
            app.compress_selected_profile_files();
        }
        if !app.home.selected_entries.is_empty() {
            let del_lbl = format!(
                "{} ({})",
                app.i18n.tr("Xóa", "Delete"),
                app.home.selected_entries.len()
            );
            if danger_icon_button(ui, VectorIcon::Trash, del_lbl).clicked() {
                app.delete_selected_profile_files();
            }
        }
    });

    ui.add_space(8.0);

    if let Some(editing_rel) = app.home.editing_file_rel.clone() {
        gaming_card_3d(
            ui,
            "profile_inline_editor_card",
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
                        if secondary_button(ui, app.i18n.tr("Đóng", "Close")).clicked() {
                            app.home.editing_file_rel = None;
                        }
                        if primary_icon_button(
                            ui,
                            VectorIcon::Save,
                            app.i18n.tr("Lưu tệp", "Save File"),
                        )
                        .clicked()
                        {
                            app.save_profile_file_editor();
                        }
                    });
                });
                ui.add_space(6.0);
                ScrollArea::vertical()
                    .id_salt("profile_file_editor_scroll")
                    .max_height(240.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut app.home.editing_file_text)
                                .code_editor()
                                .desired_rows(12)
                                .desired_width(f32::INFINITY),
                        );
                    });
            },
        );
        ui.add_space(8.0);
    }

    let entries = app.home.dir_entries.clone();
    ScrollArea::vertical()
        .id_salt("profile_files_list_scroll")
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
                let checked_init = app.home.selected_entries.contains(&entry.rel_path);
                gaming_card_3d(
                    ui,
                    ("prof_file_entry", &entry.rel_path),
                    checked_init,
                    if entry.is_dir { ACCENT_CYAN } else { ACCENT_BLUE },
                    Margin::symmetric(10, 6),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let mut checked = checked_init;
                            if ui.checkbox(&mut checked, "").changed() {
                                if checked {
                                    app.home.selected_entries.insert(entry.rel_path.clone());
                                } else {
                                    app.home.selected_entries.remove(&entry.rel_path);
                                }
                            }

                            if entry.is_dir {
                                badge(ui, "DIR", ACCENT_CYAN);
                                if ui.link(RichText::new(&entry.name).strong()).clicked() {
                                    app.home.current_rel_dir = entry.rel_path.clone();
                                    app.home.selected_entries.clear();
                                    app.refresh_selected_profile_tabs();
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
                                            app.extract_profile_archive(&entry.rel_path);
                                        }
                                        if is_editable_profile_text_file(&low)
                                            && secondary_icon_button(
                                                ui,
                                                VectorIcon::Wrench,
                                                app.i18n.tr("Sửa", "Edit"),
                                            )
                                            .clicked()
                                        {
                                            app.open_profile_file_editor(&entry.rel_path);
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

fn render_tab_bookmarks(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.home.new_bm_name)
                .hint_text(
                    app.i18n
                        .tr("Tên máy chủ (VD: Hypixel)", "Server Name (e.g. Hypixel)"),
                )
                .desired_width(175.0),
        );
        ui.add(
            egui::TextEdit::singleline(&mut app.home.new_bm_addr)
                .hint_text(app.i18n.tr(
                    "Địa chỉ IP / Domain (VD: mc.hypixel.net)",
                    "IP / Domain (e.g. mc.hypixel.net)",
                ))
                .desired_width(240.0),
        );
        if primary_icon_button(
            ui,
            VectorIcon::Plus,
            app.i18n.tr("Thêm Server", "Add Server"),
        )
        .clicked()
        {
            app.add_bookmark();
        }
        if secondary_icon_button(
            ui,
            VectorIcon::Refresh,
            app.i18n.tr("Ping tất cả", "Ping All"),
        )
        .clicked()
        {
            app.ping_all_bookmarks();
        }
    });

    ui.add_space(8.0);
    let bms = app.home.bookmarks.clone();
    let pings = app.home.bookmark_pings.clone();

    ScrollArea::vertical()
        .id_salt("home_bookmarks_scroll")
        .show(ui, |ui| {
            if bms.is_empty() {
                ui.label(
                    RichText::new(app.i18n.tr(
                        "Chưa có máy chủ yêu thích nào.",
                        "No favorite servers saved yet.",
                    ))
                    .weak(),
                );
                return;
            }
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
                                            let players_txt = format!(
                                                "{}/{} {}",
                                                st.online_players,
                                                st.max_players,
                                                app.i18n.tr("người chơi", "players")
                                            );
                                            badge(ui, &players_txt, ACCENT_BLUE);
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
                                            RichText::new(
                                                app.i18n.tr("Đang chờ Ping...", "Pinging..."),
                                            )
                                            .size(12.0)
                                            .weak(),
                                        );
                                    }
                                }
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
                                        app.delete_bookmark(&bm.id);
                                    }
                                    if primary_icon_button(
                                        ui,
                                        VectorIcon::Play,
                                        app.i18n.tr("Vào ngay", "Join Now"),
                                    )
                                    .clicked()
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
                .hint_text(
                    app.i18n
                        .tr("Lọc dòng nhật ký...", "Filter log lines..."),
                )
                .desired_width(240.0),
        );
        ui.checkbox(
            &mut app.home.auto_scroll_logs,
            app.i18n.tr("Tự cuộn xuống cuối", "Auto-scroll to bottom"),
        );
        if secondary_icon_button(
            ui,
            VectorIcon::Copy,
            app.i18n.tr("Sao chép toàn bộ", "Copy All"),
        )
        .clicked()
        {
            ui.ctx().copy_text(lines.join("\n"));
            let msg = app
                .i18n
                .tr(
                    "Đã sao chép nhật ký vào Clipboard.",
                    "Copied logs to clipboard.",
                )
                .to_string();
            app.push_toast(ToastKind::Info, msg);
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
    let lbl_playtime = app
        .i18n
        .tr("Tổng thời gian chơi", "Total Playtime")
        .to_string();
    let lbl_sessions = app
        .i18n
        .tr("Tổng số phiên mở game", "Total Sessions")
        .to_string();
    let lbl_crashes = app
        .i18n
        .tr("Số lần Crash ghi nhận", "Recorded Crashes")
        .to_string();

    ui.columns(3, |cols| {
        gaming_card_3d(
            &mut cols[0],
            "kpi_playtime",
            false,
            ACCENT_EMERALD,
            Margin::same(14),
            |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&lbl_playtime).weak());
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
                ui.label(RichText::new(&lbl_sessions).weak());
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
                ui.label(RichText::new(&lbl_crashes).weak());
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
                RichText::new(app.i18n.tr(
                    "Biểu đồ Voxel 3D giờ chơi 14 ngày gần nhất",
                    "3D Voxel Playtime Chart (Last 14 Days)",
                ))
                .size(14.5)
                .strong(),
            );
            ui.add_space(8.0);
            let stats = &app.home.daily_stats;
            if stats.is_empty() {
                ui.label(
                    RichText::new(app.i18n.tr(
                        "Chưa có dữ liệu phiên chơi theo ngày.",
                        "No daily session history recorded yet.",
                    ))
                    .weak(),
                );
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
