use crate::fx3d::{
    gaming_card_3d, paint_glowing_progress_bar, paint_icon_3d, paint_vector_icon,
    sliding_pill_tabs, tactile_button_3d, ButtonStyle3d, Icon3dKind, VectorIcon,
};
use crate::state::{LauncherAppState, ModpackDetailTab, MODPACKS_PER_PAGE};
use crate::theme::{
    ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_ORANGE, ACCENT_RED,
    ACCENT_VIOLET,
};
use crate::views::{
    accent_icon_button, badge, danger_icon_button, format_bytes, primary_icon_button,
    secondary_button, secondary_icon_button,
};
use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, Frame, Key, Layout, Margin, Order, Rect,
    RichText, ScrollArea, Sense, Stroke, StrokeKind, Vec2,
};
use egui_commonmark::CommonMarkViewer;
use thk_core::models::{ContentKind, LoaderType, ProviderSource};
use thk_mods::{DependencyRelation, ProjectDetail, ProjectSummary, ProjectVersion, SearchSort};

const FALLBACK_MC_RELEASES: &[&str] = &[
    "1.21.4", "1.21.3", "1.21.1", "1.21", "1.20.6", "1.20.4", "1.20.2", "1.20.1", "1.19.4",
    "1.19.2", "1.18.2", "1.16.5", "1.12.2", "1.8.9", "1.7.10",
];

const MC_MAJOR_FAMILIES: &[(&str, &str)] = &[
    ("all", "Tất cả nhánh"),
    ("1.21", "MC 1.21.x"),
    ("1.20", "MC 1.20.x"),
    ("1.19", "MC 1.19.x"),
    ("1.18", "MC 1.18.x"),
    ("1.16", "MC 1.16.x"),
    ("1.12", "MC 1.12.x"),
    ("1.7", "MC 1.7.x"),
];

fn provider_accent(p: ProviderSource) -> Color32 {
    match p {
        ProviderSource::Modrinth => ACCENT_EMERALD,
        ProviderSource::CurseForge => ACCENT_ORANGE,
        ProviderSource::Ftb => ACCENT_CYAN,
        ProviderSource::Technic => ACCENT_AMBER,
        ProviderSource::Spiget => ACCENT_VIOLET,
        ProviderSource::Local => ACCENT_BLUE,
    }
}

fn category_chip_color(tag: &str, fallback: Color32) -> Color32 {
    if tag.eq_ignore_ascii_case("fabric") {
        ACCENT_EMERALD
    } else if tag.eq_ignore_ascii_case("neoforge") {
        ACCENT_ORANGE
    } else if tag.eq_ignore_ascii_case("forge") {
        ACCENT_AMBER
    } else if tag.eq_ignore_ascii_case("quilt") {
        ACCENT_VIOLET
    } else if tag.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        ACCENT_CYAN
    } else {
        fallback
    }
}

fn content_kind_label(app: &LauncherAppState, kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Modpack => app.i18n.tr("Modpack", "Modpacks"),
        ContentKind::Mod => app.i18n.tr("Mod lẻ", "Mods"),
        ContentKind::Shader => app.i18n.tr("Shader", "Shaders"),
        ContentKind::ResourcePack => app.i18n.tr("Gói tài nguyên", "Resource Packs"),
        ContentKind::Plugin => "Plugins",
    }
}

pub fn render_modpacks_view(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    render_top_command_bar(app, ui);
    ui.add_space(6.0);

    if app.modpacks.detail_loading || app.modpacks.selected.is_some() {
        render_project_detail_view(app, ui);
        return;
    }

    if matches!(
        app.modpacks.provider,
        ProviderSource::Modrinth | ProviderSource::CurseForge
    ) {
        render_content_subtabs(app, ui);
        ui.add_space(6.0);
    }

    let show_sidebar = app.modpacks.show_filter_sidebar && ui.available_width() >= 680.0;
    if show_sidebar {
        egui::Panel::left("thk_modpacks_filter_sidebar")
            .resizable(false)
            .exact_size(224.0)
            .frame(Frame::NONE.inner_margin(Margin {
                left: 0,
                right: 12,
                top: 0,
                bottom: 0,
            }))
            .show(ui, |ui| {
                render_filter_sidebar(app, ui);
            });
    }

    render_results_workspace(app, ui);
}

fn render_top_command_bar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        paint_icon_3d(ui, 34.0, Icon3dKind::ModpackCrate, true);
        ui.heading(RichText::new(app.i18n.t("nav.modpacks")).size(20.5).strong());
        ui.add_space(6.0);

        let prov_tabs = [
            (
                ProviderSource::Modrinth,
                VectorIcon::Cube,
                ProviderSource::Modrinth.display_name().to_string(),
            ),
            (
                ProviderSource::CurseForge,
                VectorIcon::Sparkles,
                ProviderSource::CurseForge.display_name().to_string(),
            ),
            (
                ProviderSource::Ftb,
                VectorIcon::Shield,
                ProviderSource::Ftb.display_name().to_string(),
            ),
            (
                ProviderSource::Technic,
                VectorIcon::Globe,
                ProviderSource::Technic.display_name().to_string(),
            ),
        ];

        if let Some(next_prov) =
            sliding_pill_tabs(ui, "modpack_providers_pill", &prov_tabs, app.modpacks.provider)
        {
            app.click_sfx();
            app.modpacks.provider = next_prov;
            if !matches!(
                next_prov,
                ProviderSource::Modrinth | ProviderSource::CurseForge
            ) {
                app.modpacks.content_kind = ContentKind::Modpack;
            }
            app.modpacks.selected = None;
            app.modpacks.selected_preview = None;
            app.modpacks.detail_loading = false;
            app.search_modpacks();
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let list_style = if !app.modpacks.view_grid {
                ButtonStyle3d::PrimaryEmerald
            } else {
                ButtonStyle3d::SecondaryGlass
            };
            if tactile_button_3d(
                ui,
                Some(VectorIcon::List),
                "",
                list_style,
                vec2(34.0, 32.0),
            )
            .on_hover_text(app.i18n.tr("Chế độ danh sách", "List view"))
            .clicked()
            {
                app.click_sfx();
                app.modpacks.view_grid = false;
            }

            let grid_style = if app.modpacks.view_grid {
                ButtonStyle3d::PrimaryEmerald
            } else {
                ButtonStyle3d::SecondaryGlass
            };
            if tactile_button_3d(
                ui,
                Some(VectorIcon::Grid),
                "",
                grid_style,
                vec2(34.0, 32.0),
            )
            .on_hover_text(app.i18n.tr("Chế độ lưới thẻ 3D", "3D Showcase Grid"))
            .clicked()
            {
                app.click_sfx();
                app.modpacks.view_grid = true;
            }

            let active_filters = usize::from(!app.modpacks.mc_version_filter.trim().is_empty())
                + usize::from(app.modpacks.loader_filter.is_some());
            let filter_lbl = if active_filters > 0 {
                format!(
                    "{} ({active_filters})",
                    app.i18n.tr("Bộ lọc", "Filters")
                )
            } else {
                app.i18n.tr("Bộ lọc", "Filters").to_string()
            };
            let filter_style = if app.modpacks.show_filter_sidebar || active_filters > 0 {
                ButtonStyle3d::AccentViolet
            } else {
                ButtonStyle3d::SecondaryGlass
            };
            if tactile_button_3d(
                ui,
                Some(VectorIcon::Sliders),
                filter_lbl,
                filter_style,
                Vec2::ZERO,
            )
            .clicked()
            {
                app.click_sfx();
                app.modpacks.show_filter_sidebar = !app.modpacks.show_filter_sidebar;
            }

            if secondary_icon_button(
                ui,
                VectorIcon::Upload,
                app.i18n.tr("Nhập .mrpack / .zip", "Import .mrpack / .zip"),
            )
            .clicked()
            {
                app.click_sfx();
                app.import_modpack_from_disk();
            }

            if app
                .modpacks
                .install_modal
                .as_ref()
                .is_some_and(|m| m.installing && m.minimized)
                && accent_icon_button(
                    ui,
                    VectorIcon::Download,
                    app.i18n.tr("Đang cài đặt...", "Installing..."),
                )
                .clicked()
            {
                app.click_sfx();
                if let Some(m) = app.modpacks.install_modal.as_mut() {
                    m.minimized = false;
                }
            }
        });
    });
}

fn render_content_subtabs(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let sub_tabs = [
        (
            ContentKind::Modpack,
            VectorIcon::Cube,
            app.i18n.tr("Modpack trọn gói", "Modpacks").to_string(),
        ),
        (
            ContentKind::Mod,
            VectorIcon::Wrench,
            app.i18n.tr("Kho Mod lẻ", "Mods").to_string(),
        ),
        (
            ContentKind::Shader,
            VectorIcon::Sparkles,
            app.i18n.tr("Gói Shader", "Shaders").to_string(),
        ),
        (
            ContentKind::ResourcePack,
            VectorIcon::Image,
            app.i18n
                .tr("Gói Tài nguyên", "Resource Packs")
                .to_string(),
        ),
    ];

    ui.horizontal_wrapped(|ui| {
        if let Some(next_kind) = sliding_pill_tabs(
            ui,
            "modpack_content_kind_subtabs",
            &sub_tabs,
            app.modpacks.content_kind,
        ) {
            app.click_sfx();
            app.modpacks.content_kind = next_kind;
            if !matches!(next_kind, ContentKind::Modpack | ContentKind::Mod) {
                app.modpacks.loader_filter = None;
            }
            app.search_modpacks();
        }

        if app.modpacks.content_kind != ContentKind::Modpack {
            ui.add_space(6.0);
            let hint = if let Some(prof) = app.selected_profile() {
                format!(
                    "{}: {} ({} • {})",
                    app.i18n.tr("Profile nhận mặc định", "Default target"),
                    prof.name,
                    prof.game_version,
                    prof.loader.display_name()
                )
            } else {
                app.i18n
                    .tr(
                        "Chọn Profile khi bấm Cài đặt",
                        "Choose target profile on install",
                    )
                    .to_string()
            };
            badge(ui, &hint, ACCENT_CYAN);
        }
    });
}

fn render_filter_sidebar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let p_col = provider_accent(app.modpacks.provider);
    let supports_loader = matches!(
        app.modpacks.content_kind,
        ContentKind::Modpack | ContentKind::Mod
    ) && matches!(
        app.modpacks.provider,
        ProviderSource::Modrinth | ProviderSource::CurseForge
    );

    gaming_card_3d(
        ui,
        "modpacks_left_filter_panel",
        false,
        p_col,
        Margin::symmetric(12, 10),
        |ui| {
            ui.set_width(ui.available_width());
            ScrollArea::vertical()
                .id_salt("modpacks_sidebar_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (ic_r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        paint_vector_icon(ui.painter(), ic_r, VectorIcon::Sliders, p_col);
                        ui.label(
                            RichText::new(app.i18n.tr("Bộ lọc thông minh", "Smart Filters"))
                                .size(13.5)
                                .strong(),
                        );
                    });

                    let has_any = !app.modpacks.query.trim().is_empty()
                        || !app.modpacks.mc_version_filter.trim().is_empty()
                        || app.modpacks.loader_filter.is_some()
                        || app.modpacks.sort != SearchSort::Downloads;

                    if has_any {
                        ui.add_space(4.0);
                        let w = ui.available_width();
                        if tactile_button_3d(
                            ui,
                            Some(VectorIcon::Refresh),
                            app.i18n.tr("Đặt lại bộ lọc", "Reset filters"),
                            ButtonStyle3d::SecondaryGlass,
                            vec2(w, 28.0),
                        )
                        .clicked()
                        {
                            app.click_sfx();
                            app.reset_modpack_filters();
                        }
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);

                    ui.label(
                        RichText::new(app.i18n.tr("Sắp xếp theo", "Sort by"))
                            .size(11.8)
                            .weak()
                            .strong(),
                    );
                    ui.add_space(4.0);

                    let sorts = [
                        (
                            SearchSort::Downloads,
                            app.i18n.tr("Lượt tải cao nhất", "Most Downloads"),
                        ),
                        (
                            SearchSort::Relevance,
                            app.i18n.tr("Độ liên quan", "Relevance"),
                        ),
                        (
                            SearchSort::Updated,
                            app.i18n.tr("Mới cập nhật", "Recently Updated"),
                        ),
                        (
                            SearchSort::Newest,
                            app.i18n.tr("Mới phát hành", "Newest"),
                        ),
                    ];

                    for (srt, label) in sorts {
                        let active = app.modpacks.sort == srt;
                        let style = if active {
                            ButtonStyle3d::PrimaryEmerald
                        } else {
                            ButtonStyle3d::SecondaryGlass
                        };
                        if tactile_button_3d(
                            ui,
                            None,
                            label,
                            style,
                            vec2(ui.available_width(), 27.0),
                        )
                        .clicked()
                            && !active
                        {
                            app.click_sfx();
                            app.modpacks.sort = srt;
                            app.search_modpacks();
                        }
                        ui.add_space(2.0);
                    }

                    if supports_loader {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(app.i18n.tr("Trình nạp Mod (Loader)", "Mod Loader"))
                                .size(11.8)
                                .weak()
                                .strong(),
                        );
                        ui.add_space(4.0);

                        let loaders: [(Option<LoaderType>, &str); 5] = [
                            (None, app.i18n.tr("Tất cả Loader", "All Loaders")),
                            (Some(LoaderType::Fabric), "Fabric"),
                            (Some(LoaderType::Forge), "Forge"),
                            (Some(LoaderType::NeoForge), "NeoForge"),
                            (Some(LoaderType::Quilt), "Quilt"),
                        ];

                        for (ldr_opt, lbl) in loaders {
                            let active = app.modpacks.loader_filter == ldr_opt;
                            let style = if active {
                                ButtonStyle3d::AccentViolet
                            } else {
                                ButtonStyle3d::SecondaryGlass
                            };
                            if tactile_button_3d(
                                ui,
                                None,
                                lbl,
                                style,
                                vec2(ui.available_width(), 27.0),
                            )
                            .clicked()
                                && !active
                            {
                                app.click_sfx();
                                app.modpacks.loader_filter = ldr_opt;
                                app.search_modpacks();
                            }
                            ui.add_space(2.0);
                        }
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(app.i18n.tr("Phiên bản Minecraft", "Minecraft Version"))
                            .size(11.8)
                            .weak()
                            .strong(),
                    );
                    ui.add_space(4.0);

                    let current_group_lbl = MC_MAJOR_FAMILIES
                        .iter()
                        .find(|(k, _)| *k == app.modpacks.mc_group_filter)
                        .map(|(k, vi_lbl)| {
                            if *k == "all" {
                                app.i18n.tr(vi_lbl, "All branches")
                            } else {
                                vi_lbl
                            }
                        })
                        .unwrap_or("All");

                    egui::ComboBox::from_id_salt("modpacks_mc_family_combo")
                        .width(ui.available_width())
                        .selected_text(current_group_lbl)
                        .show_ui(ui, |ui| {
                            for &(fam_key, fam_vi) in MC_MAJOR_FAMILIES {
                                let txt = if fam_key == "all" {
                                    app.i18n.tr(fam_vi, "All branches")
                                } else {
                                    fam_vi
                                };
                                ui.selectable_value(
                                    &mut app.modpacks.mc_group_filter,
                                    fam_key.to_string(),
                                    txt,
                                );
                            }
                        });

                    ui.add_space(4.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut app.modpacks.mc_version_search)
                            .hint_text(app.i18n.tr("Lọc nhanh (VD: 1.20.1)...", "Filter (e.g. 1.20.1)..."))
                            .desired_width(ui.available_width()),
                    );
                    ui.add_space(4.0);

                    let all_ver_active = app.modpacks.mc_version_filter.trim().is_empty();
                    if tactile_button_3d(
                        ui,
                        None,
                        app.i18n.tr("Mọi phiên bản MC", "Any MC Version"),
                        if all_ver_active {
                            ButtonStyle3d::PrimaryEmerald
                        } else {
                            ButtonStyle3d::SecondaryGlass
                        },
                        vec2(ui.available_width(), 26.0),
                    )
                    .clicked()
                        && !all_ver_active
                    {
                        app.click_sfx();
                        app.modpacks.mc_version_filter.clear();
                        app.search_modpacks();
                    }
                    ui.add_space(3.0);

                    let release_list: Vec<String> = if app.home.mojang_versions.is_empty() {
                        FALLBACK_MC_RELEASES
                            .iter()
                            .map(|s| (*s).to_string())
                            .collect()
                    } else {
                        app.home
                            .mojang_versions
                            .iter()
                            .filter(|v| v.release_type == "release")
                            .map(|v| v.id.clone())
                            .collect()
                    };

                    let fam = app.modpacks.mc_group_filter.clone();
                    let needle = app.modpacks.mc_version_search.trim().to_ascii_lowercase();

                    for ver_id in release_list
                        .into_iter()
                        .filter(|v| {
                            let fam_ok = fam == "all"
                                || v == &fam
                                || v.starts_with(&format!("{fam}."));
                            let search_ok =
                                needle.is_empty() || v.to_ascii_lowercase().contains(&needle);
                            fam_ok && search_ok
                        })
                        .take(28)
                    {
                        let active = app.modpacks.mc_version_filter.trim() == ver_id;
                        if ui
                            .selectable_label(
                                active,
                                RichText::new(format!("Minecraft {ver_id}")).size(12.0),
                            )
                            .clicked()
                        {
                            app.click_sfx();
                            if active {
                                app.modpacks.mc_version_filter.clear();
                            } else {
                                app.modpacks.mc_version_filter = ver_id;
                            }
                            app.search_modpacks();
                        }
                    }
                });
        },
    );
}

fn render_results_workspace(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let prov_col = provider_accent(app.modpacks.provider);

    gaming_card_3d(
        ui,
        "modpacks_search_deck",
        false,
        prov_col,
        Margin::symmetric(12, 9),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let kind_name = content_kind_label(app, app.modpacks.content_kind);
                let hint = format!(
                    "{} {kind_name} {} {}...",
                    app.i18n.tr("Tìm kiếm", "Search"),
                    app.i18n.tr("trên", "on"),
                    app.modpacks.provider.display_name()
                );

                let text_w = (ui.available_width() - 220.0).clamp(180.0, 520.0);
                let q_resp = ui.add(
                    egui::TextEdit::singleline(&mut app.modpacks.query)
                        .hint_text(hint)
                        .desired_width(text_w),
                );
                if q_resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    app.search_modpacks();
                }

                if !app.modpacks.query.is_empty()
                    && tactile_button_3d(
                        ui,
                        Some(VectorIcon::Close),
                        "",
                        ButtonStyle3d::SecondaryGlass,
                        vec2(30.0, 28.0),
                    )
                    .on_hover_text(app.i18n.tr("Xóa từ khóa", "Clear query"))
                    .clicked()
                {
                    app.click_sfx();
                    app.modpacks.query.clear();
                    app.search_modpacks();
                }

                if primary_icon_button(
                    ui,
                    VectorIcon::Search,
                    app.i18n.tr("Tìm kiếm", "Search"),
                )
                .clicked()
                {
                    app.click_sfx();
                    app.search_modpacks();
                }

                if let Some(ldr) = app.modpacks.loader_filter {
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Close,
                        format!("Loader: {}", ldr.display_name()),
                    )
                    .clicked()
                    {
                        app.click_sfx();
                        app.modpacks.loader_filter = None;
                        app.search_modpacks();
                    }
                }

                let mc_f = app.modpacks.mc_version_filter.trim().to_string();
                if !mc_f.is_empty()
                    && secondary_icon_button(ui, VectorIcon::Close, format!("MC {mc_f}")).clicked()
                {
                    app.click_sfx();
                    app.modpacks.mc_version_filter.clear();
                    app.search_modpacks();
                }
            });
        },
    );

    ui.add_space(8.0);

    if let Some(err_text) = app.modpacks.last_error.clone() {
        gaming_card_3d(
            ui,
            "modpacks_error_banner",
            true,
            ACCENT_RED,
            Margin::symmetric(14, 10),
            |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    let (ic_r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    paint_vector_icon(ui.painter(), ic_r, VectorIcon::Shield, ACCENT_RED);
                    ui.label(
                        RichText::new(format!(
                            "{}: {err_text}",
                            app.i18n.tr("Không thể tải dữ liệu", "Could not load content")
                        ))
                        .color(ACCENT_RED)
                        .strong(),
                    );
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Refresh,
                        app.i18n.tr("Thử lại", "Retry"),
                    )
                    .clicked()
                    {
                        app.click_sfx();
                        app.search_modpacks();
                    }
                    if app.modpacks.provider == ProviderSource::CurseForge
                        && accent_icon_button(
                            ui,
                            VectorIcon::Key,
                            app.i18n.tr("Cấu hình CurseForge API Key", "Set CurseForge API Key"),
                        )
                        .clicked()
                    {
                        app.click_sfx();
                        app.show_settings_modal = true;
                    }
                });
            },
        );
        ui.add_space(8.0);
    }

    if app.modpacks.searching && app.modpacks.hits.is_empty() {
        gaming_card_3d(
            ui,
            "modpacks_loading_stage",
            false,
            prov_col,
            Margin::symmetric(20, 28),
            |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    paint_icon_3d(ui, 62.0, Icon3dKind::ModpackCrate, true);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "{} {}...",
                            app.i18n
                                .tr("Đang đồng bộ kho nội dung từ", "Synchronizing catalog from"),
                            app.modpacks.provider.display_name()
                        ))
                        .size(14.5)
                        .strong(),
                    );
                    ui.add_space(8.0);
                    let t = ui.input(|i| i.time) as f32;
                    let pulse = 0.25 + ((t * 1.9).sin() * 0.5 + 0.5) * 0.65;
                    paint_glowing_progress_bar(ui, pulse, 280.0, 14.0);
                });
            },
        );
        return;
    }

    let hits = app.modpacks.hits.clone();
    if hits.is_empty() {
        gaming_card_3d(
            ui,
            "modpacks_empty_stage",
            false,
            prov_col,
            Margin::symmetric(20, 26),
            |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    paint_icon_3d(ui, 54.0, Icon3dKind::ModpackCrate, false);
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Không tìm thấy gói nội dung nào khớp với bộ lọc hiện tại.",
                            "No projects matched your current filters.",
                        ))
                        .size(14.0)
                        .strong(),
                    );
                    ui.add_space(8.0);
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Refresh,
                        app.i18n.tr("Đặt lại bộ lọc", "Reset filters"),
                    )
                    .clicked()
                    {
                        app.click_sfx();
                        app.reset_modpack_filters();
                    }
                });
            },
        );
        return;
    }

    let total_hits = app.modpacks.total_hits.max(hits.len());
    let total_pages = pagination_total_pages(total_hits, MODPACKS_PER_PAGE);

    ScrollArea::vertical()
        .id_salt("modpacks_showcase_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let start_idx = app.modpacks.page * MODPACKS_PER_PAGE + 1;
                let end_idx = (start_idx + hits.len().saturating_sub(1)).min(total_hits);
                ui.label(
                    RichText::new(format!(
                        "{} {start_idx}–{end_idx} / {} {} ({})",
                        app.i18n.tr("Hiển thị", "Showing"),
                        format_downloads(total_hits as u64),
                        content_kind_label(app, app.modpacks.content_kind),
                        app.modpacks.provider.display_name(),
                    ))
                    .size(12.0)
                    .weak(),
                );
                if app.modpacks.searching {
                    ui.spinner();
                }
            });
            ui.add_space(6.0);

            if app.modpacks.view_grid {
                let avail_w = ui.available_width();
                let cols: usize = if avail_w >= 960.0 {
                    3
                } else if avail_w >= 540.0 {
                    2
                } else {
                    1
                };
                let gap = 10.0;
                let col_w =
                    ((avail_w - gap * (cols.saturating_sub(1) as f32)) / (cols as f32)).max(240.0);

                for row_hits in hits.chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for hit in row_hits {
                            ui.allocate_ui_with_layout(
                                vec2(col_w, 146.0),
                                Layout::top_down(Align::LEFT),
                                |ui| {
                                    render_showcase_grid_card(app, ui, hit, col_w, prov_col);
                                },
                            );
                        }
                    });
                    ui.add_space(8.0);
                }
            } else {
                for hit in &hits {
                    render_showcase_list_card(app, ui, hit, prov_col);
                    ui.add_space(6.0);
                }
            }

            ui.add_space(6.0);
            render_pagination_footer(app, ui, hits.len(), total_hits, total_pages, prov_col);
            ui.add_space(10.0);
        });
}

fn render_showcase_grid_card(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    hit: &ProjectSummary,
    col_w: f32,
    prov_col: Color32,
) {
    let tex_opt = hit
        .icon_url
        .as_deref()
        .and_then(|u| app.request_modpack_texture(u, 128));

    let title_limit = if col_w < 310.0 { 28 } else { 38 };
    let desc_limit = if col_w < 310.0 { 78 } else { 104 };

    let mut quick_clicked = false;
    let (_, card_resp) = gaming_card_3d(
        ui,
        ("mp_grid_card", &hit.project_id),
        false,
        prov_col,
        Margin::symmetric(12, 10),
        |ui| {
            ui.set_min_size(vec2((col_w - 24.0).max(180.0), 124.0));
            ui.set_max_width((col_w - 24.0).max(180.0));

            if let Some(ref tex) = tex_opt {
                let full_r = ui.max_rect();
                let glow_r = Rect::from_min_max(
                    pos2(full_r.right() - full_r.width() * 0.36, full_r.top()),
                    full_r.max,
                );
                paint_cover_texture(
                    ui.painter(),
                    glow_r,
                    tex,
                    CornerRadius::same(8),
                    Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                );
            }

            ui.horizontal(|ui| {
                paint_pack_thumbnail(ui, tex_opt.as_ref(), 54.0, prov_col, false);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        RichText::new(truncate_chars(&hit.title, title_limit))
                            .size(14.2)
                            .strong(),
                    );
                    let author_txt = if hit.author.trim().is_empty() {
                        hit.provider.display_name().to_string()
                    } else {
                        format!(
                            "{} {}",
                            app.i18n.tr("bởi", "by"),
                            truncate_chars(&hit.author, 24)
                        )
                    };
                    ui.label(RichText::new(author_txt).size(11.3).weak());
                    if hit.downloads > 0 {
                        ui.label(
                            RichText::new(format!(
                                "{} {}",
                                format_downloads(hit.downloads),
                                app.i18n.tr("lượt tải", "downloads")
                            ))
                            .size(11.0)
                            .color(ACCENT_CYAN),
                        );
                    }
                });
            });

            ui.add_space(5.0);
            let summary_line = if hit.summary.trim().is_empty() {
                app.i18n
                    .tr("Nhấn để xem mô tả và phiên bản.", "Click to view description & versions.")
                    .to_string()
            } else {
                truncate_chars(&hit.summary, desc_limit)
            };
            ui.label(RichText::new(summary_line).size(11.8).weak());

            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                ui.horizontal(|ui| {
                    for cat in hit.categories.iter().take(2) {
                        let c_col = category_chip_color(cat, prov_col);
                        badge(ui, &truncate_chars(cat, 13), c_col);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if tactile_button_3d(
                            ui,
                            Some(VectorIcon::Download),
                            app.i18n.tr("Cài", "Install"),
                            ButtonStyle3d::PrimaryEmerald,
                            vec2(68.0, 25.0),
                        )
                        .clicked()
                        {
                            quick_clicked = true;
                        }
                    });
                });
            });
        },
    );

    if quick_clicked {
        app.click_sfx();
        app.quick_install_from_card(hit.clone());
    } else if card_resp
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        app.click_sfx();
        app.open_modpack_detail(hit.clone());
    }
}

fn render_showcase_list_card(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    hit: &ProjectSummary,
    prov_col: Color32,
) {
    let tex_opt = hit
        .icon_url
        .as_deref()
        .and_then(|u| app.request_modpack_texture(u, 128));

    let mut detail_clicked = false;
    let mut quick_clicked = false;

    let (_, card_resp) = gaming_card_3d(
        ui,
        ("mp_list_card", &hit.project_id),
        false,
        prov_col,
        Margin::symmetric(14, 10),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                paint_pack_thumbnail(ui, tex_opt.as_ref(), 52.0, prov_col, false);
                ui.add_space(6.0);

                let text_w = (ui.available_width() - 196.0).max(160.0);
                ui.vertical(|ui| {
                    ui.set_max_width(text_w);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(truncate_chars(&hit.title, 58))
                                .size(15.0)
                                .strong(),
                        );
                        if !hit.author.trim().is_empty() {
                            ui.label(
                                RichText::new(format!(
                                    "{} {}",
                                    app.i18n.tr("bởi", "by"),
                                    truncate_chars(&hit.author, 28)
                                ))
                                .size(11.8)
                                .weak(),
                            );
                        }
                        if hit.downloads > 0 {
                            badge(
                                ui,
                                &format!(
                                    "{} {}",
                                    format_downloads(hit.downloads),
                                    app.i18n.tr("tải", "dl")
                                ),
                                ACCENT_BLUE,
                            );
                        }
                        if let Some(dt) = short_date(hit.updated_at.as_deref()) {
                            badge(ui, dt, ACCENT_VIOLET);
                        }
                        for cat in hit.categories.iter().take(4) {
                            badge(ui, &truncate_chars(cat, 15), category_chip_color(cat, prov_col));
                        }
                    });
                    ui.label(
                        RichText::new(truncate_chars(&hit.summary, 165))
                            .size(12.2)
                            .weak(),
                    );
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if primary_icon_button(
                        ui,
                        VectorIcon::Download,
                        app.i18n.tr("Cài đặt", "Install"),
                    )
                    .clicked()
                    {
                        quick_clicked = true;
                    }
                    if secondary_icon_button(
                        ui,
                        VectorIcon::ArrowRight,
                        app.i18n.tr("Chi tiết", "Details"),
                    )
                    .clicked()
                    {
                        detail_clicked = true;
                    }
                });
            });
        },
    );

    if quick_clicked {
        app.click_sfx();
        app.quick_install_from_card(hit.clone());
    } else if detail_clicked
        || card_resp
            .interact(Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    {
        app.click_sfx();
        app.open_modpack_detail(hit.clone());
    }
}

fn render_pagination_footer(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    loaded_count: usize,
    total_hits: usize,
    total_pages: usize,
    prov_col: Color32,
) {
    gaming_card_3d(
        ui,
        "modpacks_pagination_bar",
        false,
        prov_col,
        Margin::symmetric(12, 8),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let can_prev = app.modpacks.page > 0 && !app.modpacks.searching;
                ui.add_enabled_ui(can_prev, |ui| {
                    if secondary_icon_button(
                        ui,
                        VectorIcon::ArrowLeft,
                        app.i18n.tr("Trang trước", "Prev Page"),
                    )
                    .clicked()
                    {
                        app.click_sfx();
                        let prev = app.modpacks.page.saturating_sub(1);
                        app.search_modpacks_page(prev, false);
                    }
                });

                badge(
                    ui,
                    &format!(
                        "{} {} / {}",
                        app.i18n.tr("Trang", "Page"),
                        app.modpacks.page + 1,
                        total_pages
                    ),
                    prov_col,
                );

                let can_next =
                    (app.modpacks.page + 1) < total_pages && !app.modpacks.searching;
                ui.add_enabled_ui(can_next, |ui| {
                    if secondary_icon_button(
                        ui,
                        VectorIcon::ArrowRight,
                        app.i18n.tr("Trang sau", "Next Page"),
                    )
                    .clicked()
                    {
                        app.click_sfx();
                        let next = app.modpacks.page + 1;
                        app.search_modpacks_page(next, false);
                    }
                });

                let next_append_offset = (app.modpacks.page + 1) * MODPACKS_PER_PAGE;
                if next_append_offset < total_hits && loaded_count < total_hits {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_enabled_ui(!app.modpacks.searching, |ui| {
                            if accent_icon_button(
                                ui,
                                VectorIcon::Plus,
                                app.i18n
                                    .tr("Tải thêm vào danh sách", "Load More Results"),
                            )
                            .clicked()
                            {
                                app.click_sfx();
                                let next = app.modpacks.page + 1;
                                app.search_modpacks_page(next, true);
                            }
                        });
                    });
                }
            });
        },
    );
}

fn render_project_detail_view(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    if app.modpacks.detail_loading && app.modpacks.selected.is_none() {
        let preview = app.modpacks.selected_preview.clone();
        let p_col = preview
            .as_ref()
            .map(|p| provider_accent(p.provider))
            .unwrap_or(ACCENT_EMERALD);

        if secondary_icon_button(
            ui,
            VectorIcon::ArrowLeft,
            app.i18n.tr("Quay lại danh sách", "Back to Catalog"),
        )
        .clicked()
        {
            app.click_sfx();
            app.modpacks.detail_seq += 1;
            app.modpacks.detail_loading = false;
            app.modpacks.selected_preview = None;
            return;
        }

        ui.add_space(10.0);
        gaming_card_3d(
            ui,
            "modpacks_detail_loading_card",
            true,
            p_col,
            Margin::symmetric(20, 28),
            |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    paint_icon_3d(ui, 64.0, Icon3dKind::ModpackCrate, true);
                    ui.add_space(8.0);
                    let title = preview
                        .as_ref()
                        .map(|p| p.title.as_str())
                        .unwrap_or("Modpack");
                    ui.heading(RichText::new(title).size(19.0).strong());
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Đang tải mô tả chi tiết, ảnh chụp màn hình và danh sách phiên bản...",
                            "Loading full description, screenshots, and release versions...",
                        ))
                        .weak(),
                    );
                    ui.add_space(10.0);
                    let t = ui.input(|i| i.time) as f32;
                    let pulse = 0.3 + ((t * 2.1).sin() * 0.5 + 0.5) * 0.6;
                    paint_glowing_progress_bar(ui, pulse, 300.0, 14.0);
                });
            },
        );
        return;
    }

    let Some((detail, versions)) = app.modpacks.selected.clone() else {
        return;
    };
    let p_col = provider_accent(detail.summary.provider);

    ui.horizontal_wrapped(|ui| {
        if secondary_icon_button(
            ui,
            VectorIcon::ArrowLeft,
            app.i18n.tr("Quay lại kho tìm kiếm", "Back to Catalog"),
        )
        .clicked()
        {
            app.click_sfx();
            app.modpacks.selected = None;
            app.modpacks.selected_preview = None;
            app.modpacks.lightbox_idx = None;
            return;
        }

        badge(ui, detail.summary.provider.display_name(), p_col);
        badge(
            ui,
            content_kind_label(app, detail.summary.content_kind),
            ACCENT_VIOLET,
        );
        if let Some(ref src_url) = detail.source_url {
            if secondary_icon_button(
                ui,
                VectorIcon::Globe,
                app.i18n.tr("Mở trang gốc", "Open Website"),
            )
            .clicked()
            {
                app.click_sfx();
                let _ = open::that(src_url);
            }
        }
    });

    if app.modpacks.selected.is_none() {
        return;
    }

    ui.add_space(6.0);

    let hero_tex = detail
        .summary
        .icon_url
        .as_deref()
        .and_then(|u| app.request_modpack_texture(u, 192));

    gaming_card_3d(
        ui,
        "modpack_detail_hero_stage",
        true,
        p_col,
        Margin::symmetric(16, 12),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                paint_pack_thumbnail(ui, hero_tex.as_ref(), 74.0, p_col, true);
                ui.add_space(8.0);

                let info_w = (ui.available_width() - 205.0).max(180.0);
                ui.vertical(|ui| {
                    ui.set_max_width(info_w);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.horizontal_wrapped(|ui| {
                        ui.heading(RichText::new(&detail.summary.title).size(20.0).strong());
                        if !detail.summary.author.trim().is_empty() {
                            ui.label(
                                RichText::new(format!(
                                    "{} {}",
                                    app.i18n.tr("bởi", "by"),
                                    detail.summary.author
                                ))
                                .size(13.0)
                                .weak(),
                            );
                        }
                    });

                    if !detail.summary.summary.trim().is_empty() {
                        ui.label(
                            RichText::new(truncate_chars(&detail.summary.summary, 220))
                                .size(12.5)
                                .weak(),
                        );
                    }

                    ui.horizontal_wrapped(|ui| {
                        if detail.summary.downloads > 0 {
                            badge(
                                ui,
                                &format!(
                                    "{} {}",
                                    format_downloads(detail.summary.downloads),
                                    app.i18n.tr("lượt tải", "downloads")
                                ),
                                ACCENT_BLUE,
                            );
                        }
                        if let Some(dt) = short_date(detail.summary.updated_at.as_deref()) {
                            badge(
                                ui,
                                &format!("{}: {dt}", app.i18n.tr("Cập nhật", "Updated")),
                                ACCENT_CYAN,
                            );
                        }
                        for ldr in detail.loaders.iter().take(4) {
                            badge(ui, ldr, category_chip_color(ldr, p_col));
                        }
                        for cat in detail.summary.categories.iter().take(4) {
                            if !detail
                                .loaders
                                .iter()
                                .any(|l| l.eq_ignore_ascii_case(cat))
                            {
                                badge(ui, cat, category_chip_color(cat, p_col));
                            }
                        }
                    });
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let cta_label = if detail.summary.content_kind == ContentKind::Modpack {
                        app.i18n.tr("Cài đặt Modpack", "Install Modpack")
                    } else {
                        app.i18n.tr("Cài vào Profile", "Install to Profile")
                    };
                    if tactile_button_3d(
                        ui,
                        Some(VectorIcon::Download),
                        cta_label,
                        ButtonStyle3d::HeroLaunch,
                        vec2(182.0, 42.0),
                    )
                    .clicked()
                    {
                        app.click_sfx();
                        app.open_modpack_install_modal(None);
                    }
                });
            });
        },
    );

    ui.add_space(8.0);

    let detail_tabs = [
        (
            ModpackDetailTab::Overview,
            VectorIcon::Sparkles,
            app.i18n.tr("Mô tả chi tiết", "Overview").to_string(),
        ),
        (
            ModpackDetailTab::Versions,
            VectorIcon::Download,
            format!(
                "{} ({})",
                app.i18n.tr("Phiên bản", "Versions"),
                versions.len()
            ),
        ),
        (
            ModpackDetailTab::Gallery,
            VectorIcon::Image,
            format!(
                "{} ({})",
                app.i18n.tr("Thư viện ảnh", "Gallery"),
                detail.gallery.len()
            ),
        ),
    ];

    if let Some(next_tab) = sliding_pill_tabs(
        ui,
        "modpack_detail_subtabs_pill",
        &detail_tabs,
        app.modpacks.detail_tab,
    ) {
        app.click_sfx();
        app.modpacks.detail_tab = next_tab;
    }

    ui.add_space(8.0);

    match app.modpacks.detail_tab {
        ModpackDetailTab::Overview => render_detail_overview_tab(app, ui, &detail, p_col),
        ModpackDetailTab::Versions => render_detail_versions_tab(app, ui, &versions, p_col),
        ModpackDetailTab::Gallery => render_detail_gallery_tab(app, ui, &detail, p_col),
    }
}

fn render_detail_overview_tab(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    detail: &ProjectDetail,
    p_col: Color32,
) {
    let md_body = normalize_project_markdown(&detail.body_markdown, &detail.summary.summary);
    ScrollArea::vertical()
        .id_salt("modpack_detail_md_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            gaming_card_3d(
                ui,
                "modpack_detail_md_card",
                false,
                p_col,
                Margin::same(16),
                |ui| {
                    ui.set_width(ui.available_width());
                    CommonMarkViewer::new().show(ui, &mut app.modpacks.md_cache, &md_body);
                },
            );
            ui.add_space(10.0);
        });
}

fn render_detail_versions_tab(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    versions: &[ProjectVersion],
    p_col: Color32,
) {
    let mut unique_mc: Vec<String> = Vec::new();
    for v in versions {
        for gv in &v.game_versions {
            if !unique_mc.contains(gv) {
                unique_mc.push(gv.clone());
            }
        }
    }

    gaming_card_3d(
        ui,
        "modpack_versions_filter_bar",
        false,
        p_col,
        Margin::symmetric(12, 8),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(app.i18n.tr("Lọc phiên bản:", "Filter versions:"))
                        .size(12.2)
                        .strong(),
                );

                let chan_lbl = match app.modpacks.detail_channel_filter.as_str() {
                    "release" => "Release",
                    "beta" => "Beta",
                    "alpha" => "Alpha",
                    _ => app.i18n.tr("Mọi kênh phát hành", "All Channels"),
                };
                egui::ComboBox::from_id_salt("mp_detail_channel_combo")
                    .selected_text(chan_lbl)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut app.modpacks.detail_channel_filter,
                            "all".into(),
                            app.i18n.tr("Mọi kênh phát hành", "All Channels"),
                        );
                        ui.selectable_value(
                            &mut app.modpacks.detail_channel_filter,
                            "release".into(),
                            "Release",
                        );
                        ui.selectable_value(
                            &mut app.modpacks.detail_channel_filter,
                            "beta".into(),
                            "Beta",
                        );
                        ui.selectable_value(
                            &mut app.modpacks.detail_channel_filter,
                            "alpha".into(),
                            "Alpha",
                        );
                    });

                let ldr_lbl = app
                    .modpacks
                    .detail_loader_filter
                    .map(|l| l.display_name())
                    .unwrap_or_else(|| app.i18n.tr("Mọi Loader", "All Loaders"));
                egui::ComboBox::from_id_salt("mp_detail_loader_combo")
                    .selected_text(ldr_lbl)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut app.modpacks.detail_loader_filter,
                            None,
                            app.i18n.tr("Mọi Loader", "All Loaders"),
                        );
                        for ldr in [
                            LoaderType::Fabric,
                            LoaderType::Forge,
                            LoaderType::NeoForge,
                            LoaderType::Quilt,
                        ] {
                            ui.selectable_value(
                                &mut app.modpacks.detail_loader_filter,
                                Some(ldr),
                                ldr.display_name(),
                            );
                        }
                    });

                if !unique_mc.is_empty() {
                    let mc_lbl = if app.modpacks.detail_mc_filter == "all" {
                        app.i18n.tr("Mọi bản MC", "All MC Versions").to_string()
                    } else {
                        format!("MC {}", app.modpacks.detail_mc_filter)
                    };
                    egui::ComboBox::from_id_salt("mp_detail_mc_combo")
                        .selected_text(mc_lbl)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut app.modpacks.detail_mc_filter,
                                "all".into(),
                                app.i18n.tr("Mọi bản MC", "All MC Versions"),
                            );
                            for gv in unique_mc.iter().take(40) {
                                ui.selectable_value(
                                    &mut app.modpacks.detail_mc_filter,
                                    gv.clone(),
                                    format!("MC {gv}"),
                                );
                            }
                        });
                }
            });
        },
    );

    ui.add_space(6.0);

    let ch_f = app.modpacks.detail_channel_filter.clone();
    let ldr_f = app.modpacks.detail_loader_filter;
    let mc_f = app.modpacks.detail_mc_filter.clone();

    let filtered: Vec<&ProjectVersion> = versions
        .iter()
        .filter(|v| version_matches_detail_filters(v, &ch_f, ldr_f, &mc_f))
        .collect();

    ScrollArea::vertical()
        .id_salt("modpack_detail_versions_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if filtered.is_empty() {
                gaming_card_3d(
                    ui,
                    "mp_versions_empty_card",
                    false,
                    p_col,
                    Margin::same(18),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(app.i18n.tr(
                                "Không có phiên bản nào khớp với bộ lọc hiện tại.",
                                "No versions match the selected filters.",
                            ))
                            .weak(),
                        );
                    },
                );
                return;
            }

            let mut install_ver_id: Option<String> = None;
            for ver in filtered {
                let chan_col = if ver.release_channel.eq_ignore_ascii_case("release") {
                    ACCENT_EMERALD
                } else if ver.release_channel.eq_ignore_ascii_case("beta") {
                    ACCENT_AMBER
                } else {
                    ACCENT_ORANGE
                };
                let primary_size = ver
                    .files
                    .iter()
                    .find(|f| f.primary)
                    .or_else(|| ver.files.first())
                    .map(|f| f.size)
                    .unwrap_or(0);
                let req_deps = ver
                    .dependencies
                    .iter()
                    .filter(|d| d.relation == DependencyRelation::Required)
                    .count();

                gaming_card_3d(
                    ui,
                    ("mp_ver_row", &ver.id),
                    false,
                    chan_col,
                    Margin::symmetric(14, 9),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.set_max_width((ui.available_width() - 140.0).max(160.0));
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(truncate_chars(&ver.name, 54))
                                            .size(13.8)
                                            .strong(),
                                    );
                                    badge(ui, &ver.release_channel, chan_col);
                                    if ver.version_number != ver.name
                                        && !ver.version_number.is_empty()
                                    {
                                        badge(
                                            ui,
                                            &truncate_chars(&ver.version_number, 22),
                                            ACCENT_BLUE,
                                        );
                                    }
                                });
                                ui.horizontal_wrapped(|ui| {
                                    for ldr in ver.loaders.iter().take(3) {
                                        badge(ui, ldr, category_chip_color(ldr, p_col));
                                    }
                                    for gv in ver.game_versions.iter().take(4) {
                                        badge(ui, &format!("MC {gv}"), ACCENT_CYAN);
                                    }
                                    if ver.game_versions.len() > 4 {
                                        badge(
                                            ui,
                                            &format!("+{}", ver.game_versions.len() - 4),
                                            ACCENT_CYAN,
                                        );
                                    }
                                    if primary_size > 0 {
                                        ui.label(
                                            RichText::new(format_bytes(primary_size))
                                                .size(11.4)
                                                .weak(),
                                        );
                                    }
                                    if let Some(dt) = short_date(ver.published_at.as_deref()) {
                                        ui.label(RichText::new(dt).size(11.4).weak());
                                    }
                                    if req_deps > 0 {
                                        badge(
                                            ui,
                                            &format!(
                                                "{req_deps} {}",
                                                app.i18n.tr("phụ thuộc", "deps")
                                            ),
                                            ACCENT_VIOLET,
                                        );
                                    }
                                });
                            });

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if primary_icon_button(
                                    ui,
                                    VectorIcon::Download,
                                    app.i18n.tr("Cài bản này", "Install"),
                                )
                                .clicked()
                                {
                                    install_ver_id = Some(ver.id.clone());
                                }
                            });
                        });
                    },
                );
                ui.add_space(5.0);
            }

            if let Some(vid) = install_ver_id {
                app.click_sfx();
                app.open_modpack_install_modal(Some(&vid));
            }
            ui.add_space(8.0);
        });
}

fn render_detail_gallery_tab(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    detail: &ProjectDetail,
    p_col: Color32,
) {
    if detail.gallery.is_empty() {
        gaming_card_3d(
            ui,
            "mp_gallery_empty_card",
            false,
            p_col,
            Margin::symmetric(20, 26),
            |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    let (ic_r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                    paint_vector_icon(ui.painter(), ic_r, VectorIcon::Image, p_col);
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Tác giả chưa đăng tải ảnh chụp màn hình cho dự án này.",
                            "No gallery screenshots were provided for this project.",
                        ))
                        .weak(),
                    );
                });
            },
        );
        return;
    }

    ScrollArea::vertical()
        .id_salt("modpack_detail_gallery_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let avail_w = ui.available_width();
            let cols: usize = if avail_w >= 620.0 { 2 } else { 1 };
            let gap = 12.0;
            let col_w =
                ((avail_w - gap * (cols.saturating_sub(1) as f32)) / (cols as f32)).max(240.0);
            let img_h = ((col_w - 20.0) * 9.0 / 16.0).clamp(135.0, 260.0);

            let mut clicked_idx: Option<usize> = None;

            for (chunk_i, row_imgs) in detail.gallery.chunks(cols).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (local_i, img) in row_imgs.iter().enumerate() {
                        let global_idx = chunk_i * cols + local_i;
                        let tex_opt = app.request_modpack_texture(&img.url, 960);

                        ui.allocate_ui_with_layout(
                            vec2(col_w, img_h + 44.0),
                            Layout::top_down(Align::LEFT),
                            |ui| {
                                let (_, resp) = gaming_card_3d(
                                    ui,
                                    ("mp_gallery_card", global_idx),
                                    false,
                                    p_col,
                                    Margin::same(10),
                                    |ui| {
                                        let inner_w = (col_w - 20.0).max(160.0);
                                        let (img_rect, _) = ui.allocate_exact_size(
                                            vec2(inner_w, img_h),
                                            Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            img_rect,
                                            CornerRadius::same(8),
                                            Color32::from_rgb(13, 18, 30),
                                        );
                                        if let Some(ref tex) = tex_opt {
                                            paint_cover_texture(
                                                ui.painter(),
                                                img_rect,
                                                tex,
                                                CornerRadius::same(8),
                                                Color32::WHITE,
                                            );
                                        } else {
                                            let c_rect = Rect::from_center_size(
                                                img_rect.center(),
                                                vec2(26.0, 26.0),
                                            );
                                            paint_vector_icon(
                                                ui.painter(),
                                                c_rect,
                                                VectorIcon::Image,
                                                p_col,
                                            );
                                        }
                                        ui.painter().rect_stroke(
                                            img_rect,
                                            CornerRadius::same(8),
                                            Stroke::new(
                                                1.0,
                                                Color32::from_rgba_unmultiplied(
                                                    p_col.r(),
                                                    p_col.g(),
                                                    p_col.b(),
                                                    95,
                                                ),
                                            ),
                                            StrokeKind::Inside,
                                        );

                                        ui.add_space(4.0);
                                        let caption = if img.title.trim().is_empty() {
                                            format!(
                                                "{} #{}",
                                                app.i18n.tr("Ảnh chụp màn hình", "Screenshot"),
                                                global_idx + 1
                                            )
                                        } else {
                                            truncate_chars(&img.title, 48)
                                        };
                                        ui.label(RichText::new(caption).size(12.0).strong());
                                    },
                                );
                                if resp
                                    .interact(Sense::click())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    clicked_idx = Some(global_idx);
                                }
                            },
                        );
                    }
                });
                ui.add_space(10.0);
            }

            if let Some(idx) = clicked_idx {
                app.click_sfx();
                app.modpacks.lightbox_idx = Some(idx);
            }
        });
}

pub fn render_modpack_modals(app: &mut LauncherAppState, ctx: &egui::Context) {
    render_gallery_lightbox_modal(app, ctx);
    render_install_studio_modal(app, ctx);
}

fn render_gallery_lightbox_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let Some(idx) = app.modpacks.lightbox_idx else {
        return;
    };
    let Some((ref detail, _)) = app.modpacks.selected else {
        app.modpacks.lightbox_idx = None;
        return;
    };
    if detail.gallery.is_empty() {
        app.modpacks.lightbox_idx = None;
        return;
    }

    let count = detail.gallery.len();
    let safe_idx = idx.min(count - 1);
    let img = detail.gallery[safe_idx].clone();
    let p_col = provider_accent(detail.summary.provider);
    let tex_opt = app.request_modpack_texture(&img.url, 1280);

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.modpacks.lightbox_idx = None;
        return;
    }

    let screen = ctx.content_rect();
    let modal_w = (screen.width() - 96.0).clamp(440.0, 980.0);
    let modal_h = (screen.height() - 120.0).clamp(300.0, 620.0);

    let mut close_lightbox = false;
    let mut next_idx = safe_idx;

    egui::Window::new(app.i18n.tr("Xem ảnh chụp màn hình", "Screenshot Viewer"))
        .id(egui::Id::unique("thk_modpack_gallery_lightbox"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .collapsible(false)
        .resizable(false)
        .order(Order::Foreground)
        .fixed_size(vec2(modal_w, modal_h + 62.0))
        .show(ctx, |ui| {
            let (stage_rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), modal_h), Sense::hover());
            ui.painter().rect_filled(
                stage_rect,
                CornerRadius::same(10),
                Color32::from_rgb(9, 12, 22),
            );
            if let Some(ref tex) = tex_opt {
                paint_contain_texture(ui.painter(), stage_rect.shrink(6.0), tex);
            } else {
                ui.painter().text(
                    stage_rect.center(),
                    Align2::CENTER_CENTER,
                    app.i18n.tr("Đang tải ảnh độ phân giải cao...", "Loading high-res image..."),
                    egui::FontId::proportional(14.0),
                    ACCENT_CYAN,
                );
            }
            ui.painter().rect_stroke(
                stage_rect,
                CornerRadius::same(10),
                Stroke::new(1.2, p_col),
                StrokeKind::Inside,
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if count > 1
                    && secondary_icon_button(
                        ui,
                        VectorIcon::ArrowLeft,
                        app.i18n.tr("Ảnh trước", "Prev"),
                    )
                    .clicked()
                {
                    app.click_sfx();
                    next_idx = if safe_idx == 0 {
                        count - 1
                    } else {
                        safe_idx - 1
                    };
                }

                badge(ui, &format!("{} / {count}", safe_idx + 1), p_col);
                if !img.title.trim().is_empty() {
                    ui.label(
                        RichText::new(truncate_chars(&img.title, 52))
                            .size(12.5)
                            .strong(),
                    );
                }

                if count > 1
                    && secondary_icon_button(
                        ui,
                        VectorIcon::ArrowRight,
                        app.i18n.tr("Ảnh tiếp", "Next"),
                    )
                    .clicked()
                {
                    app.click_sfx();
                    next_idx = (safe_idx + 1) % count;
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if danger_icon_button(
                        ui,
                        VectorIcon::Close,
                        app.i18n.tr("Đóng", "Close"),
                    )
                    .clicked()
                    {
                        app.click_sfx();
                        close_lightbox = true;
                    }
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Globe,
                        app.i18n.tr("Mở ảnh gốc", "Open Original"),
                    )
                    .clicked()
                    {
                        app.click_sfx();
                        let _ = open::that(&img.url);
                    }
                });
            });
        });

    if close_lightbox {
        app.modpacks.lightbox_idx = None;
    } else if next_idx != safe_idx {
        app.modpacks.lightbox_idx = Some(next_idx);
    }
}

fn render_install_studio_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let is_visible = app
        .modpacks
        .install_modal
        .as_ref()
        .is_some_and(|m| !m.minimized);
    if !is_visible {
        return;
    }

    let Some(modal_snap) = app.modpacks.install_modal.clone() else {
        return;
    };

    let p_col = provider_accent(modal_snap.detail.summary.provider);
    let icon_tex = modal_snap
        .detail
        .summary
        .icon_url
        .as_deref()
        .and_then(|u| app.request_modpack_texture(u, 128));

    let mut close_modal = false;
    let mut minimize_modal = false;
    let mut trigger_start = false;
    let mut trigger_cancel = false;
    let mut trigger_pick_dir = false;
    let mut clear_custom_dir = false;

    let mut updated_ver_idx = modal_snap.chosen_version_idx;
    let mut updated_profile_name = modal_snap.profile_name.clone();
    let mut updated_ram_mb = modal_snap.ram_mb;
    let mut updated_target_profile = modal_snap.target_profile_id.clone();

    let win_title = if modal_snap.content_kind == ContentKind::Modpack {
        app.i18n.tr("Trạm Cài Đặt Modpack 3D", "3D Modpack Install Studio")
    } else {
        app.i18n.tr("Cài Đặt Nội Dung Vào Profile", "Install Content to Profile")
    };

    egui::Window::new(win_title)
        .id(egui::Id::unique("thk_modpack_install_studio_window"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .collapsible(false)
        .resizable(false)
        .order(Order::Foreground)
        .default_width(530.0)
        .show(ctx, |ui| {
            ui.set_width(516.0);
            gaming_card_3d(
                ui,
                "mp_install_modal_header",
                true,
                p_col,
                Margin::symmetric(12, 10),
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        paint_pack_thumbnail(ui, icon_tex.as_ref(), 48.0, p_col, true);
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(truncate_chars(
                                    &modal_snap.detail.summary.title,
                                    48,
                                ))
                                .size(15.5)
                                .strong(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                badge(ui, modal_snap.detail.summary.provider.display_name(), p_col);
                                badge(
                                    ui,
                                    content_kind_label(app, modal_snap.content_kind),
                                    ACCENT_VIOLET,
                                );
                            });
                        });
                    });
                },
            );

            ui.add_space(10.0);

            if let Some(ref err) = modal_snap.error_msg {
                gaming_card_3d(
                    ui,
                    "mp_install_modal_err",
                    false,
                    ACCENT_RED,
                    Margin::symmetric(12, 8),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(err).color(ACCENT_RED).size(12.2).strong());
                    },
                );
                ui.add_space(8.0);
            }

            if modal_snap.installing {
                gaming_card_3d(
                    ui,
                    "mp_install_live_progress_card",
                    true,
                    ACCENT_EMERALD,
                    Margin::same(14),
                    |ui| {
                        ui.set_width(ui.available_width());
                        if let Some(ref prog) = app.active_progress {
                            let frac = (prog.percent / 100.0).clamp(0.0, 1.0);
                            ui.horizontal(|ui| {
                                badge(ui, &prog.phase, ACCENT_CYAN);
                                ui.label(
                                    RichText::new(format!("{:.1}%", prog.percent))
                                        .size(13.5)
                                        .strong()
                                        .color(ACCENT_EMERALD),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    badge(
                                        ui,
                                        &format!("{}/s", format_bytes(prog.bytes_per_sec)),
                                        ACCENT_EMERALD,
                                    );
                                });
                            });
                            ui.add_space(6.0);
                            paint_glowing_progress_bar(
                                ui,
                                frac,
                                ui.available_width().max(200.0),
                                18.0,
                            );
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!(
                                    "{} ({}/{})",
                                    truncate_chars(&prog.message, 62),
                                    prog.completed_files,
                                    prog.total_files
                                ))
                                .size(12.0)
                                .weak(),
                            );
                        } else {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new(app.i18n.tr(
                                        "Đang giải nén gói cài đặt và phân giải phụ thuộc...",
                                        "Extracting manifest and resolving dependencies...",
                                    ))
                                    .strong(),
                                );
                            });
                        }
                    },
                );

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if secondary_icon_button(
                        ui,
                        VectorIcon::ArrowLeft,
                        app.i18n.tr("Thu gọn xuống thanh dưới", "Minimize to Status Bar"),
                    )
                    .clicked()
                    {
                        minimize_modal = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if danger_icon_button(
                            ui,
                            VectorIcon::Close,
                            app.i18n.tr("Hủy cài đặt", "Cancel Install"),
                        )
                        .clicked()
                        {
                            trigger_cancel = true;
                        }
                    });
                });
                return;
            }

            ui.label(
                RichText::new(app.i18n.tr("Phiên bản phát hành", "Release Version"))
                    .size(12.2)
                    .strong(),
            );
            ui.add_space(3.0);

            let sel_ver_lbl = modal_snap
                .versions
                .get(updated_ver_idx)
                .map(format_version_combo_label)
                .unwrap_or_else(|| "—".into());

            egui::ComboBox::from_id_salt("mp_studio_version_combo")
                .width(ui.available_width())
                .selected_text(sel_ver_lbl)
                .show_ui(ui, |ui| {
                    for (idx, v) in modal_snap.versions.iter().enumerate() {
                        ui.selectable_value(
                            &mut updated_ver_idx,
                            idx,
                            format_version_combo_label(v),
                        );
                    }
                });

            ui.add_space(10.0);

            if modal_snap.content_kind == ContentKind::Modpack {
                ui.label(
                    RichText::new(app.i18n.tr("Tên Profile mới", "New Profile Name"))
                        .size(12.2)
                        .strong(),
                );
                ui.add_space(3.0);
                ui.add(
                    egui::TextEdit::singleline(&mut updated_profile_name)
                        .desired_width(ui.available_width()),
                );

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}: {} MB ({:.1} GB)",
                            app.i18n.tr("Bộ nhớ RAM cấp phát", "Allocated RAM"),
                            updated_ram_mb,
                            updated_ram_mb as f32 / 1024.0
                        ))
                        .size(12.2)
                        .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for preset_gb in [12u32, 8, 6, 4] {
                            let mb = preset_gb * 1024;
                            let active = updated_ram_mb == mb;
                            if tactile_button_3d(
                                ui,
                                None,
                                format!("{preset_gb}G"),
                                if active {
                                    ButtonStyle3d::PrimaryEmerald
                                } else {
                                    ButtonStyle3d::SecondaryGlass
                                },
                                vec2(40.0, 23.0),
                            )
                            .clicked()
                            {
                                updated_ram_mb = mb;
                            }
                        }
                    });
                });
                ui.add(
                    egui::Slider::new(&mut updated_ram_mb, 2048..=16384)
                        .step_by(512.0)
                        .show_value(false),
                );

                ui.add_space(8.0);
                ui.label(
                    RichText::new(app.i18n.tr("Thư mục lưu trữ Instance", "Instance Directory"))
                        .size(12.2)
                        .strong(),
                );
                ui.add_space(3.0);
                ui.horizontal_wrapped(|ui| {
                    let dir_txt = match modal_snap.custom_dir {
                        Some(ref p) => truncate_chars(&p.display().to_string(), 46),
                        None => app
                            .i18n
                            .tr("Mặc định (Thư mục instances chuẩn)", "Default (Managed instances folder)")
                            .to_string(),
                    };
                    badge(ui, &dir_txt, ACCENT_CYAN);
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Folder,
                        app.i18n.tr("Chọn thư mục...", "Browse..."),
                    )
                    .clicked()
                    {
                        trigger_pick_dir = true;
                    }
                    if modal_snap.custom_dir.is_some()
                        && secondary_button(ui, app.i18n.tr("Mặc định", "Reset")).clicked()
                    {
                        clear_custom_dir = true;
                    }
                });
            } else {
                ui.label(
                    RichText::new(app.i18n.tr("Chọn Profile nhận nội dung", "Target Profile"))
                        .size(12.2)
                        .strong(),
                );
                ui.add_space(3.0);

                if app.home.profiles.is_empty() {
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Bạn chưa có Profile nào. Hãy tạo một Profile tại mục Trang chủ trước!",
                            "No profiles exist yet. Create a profile on the Home screen first!",
                        ))
                        .color(ACCENT_AMBER),
                    );
                } else {
                    let current_prof_lbl = updated_target_profile
                        .as_deref()
                        .and_then(|pid| app.home.profiles.iter().find(|p| p.id == pid))
                        .map(|p| {
                            format!(
                                "{} (MC {} • {})",
                                p.name,
                                p.game_version,
                                p.loader.display_name()
                            )
                        })
                        .unwrap_or_else(|| {
                            app.i18n.tr("Chọn một Profile...", "Select a profile...").into()
                        });

                    egui::ComboBox::from_id_salt("mp_studio_target_profile_combo")
                        .width(ui.available_width())
                        .selected_text(current_prof_lbl)
                        .show_ui(ui, |ui| {
                            for prof in &app.home.profiles {
                                let lbl = format!(
                                    "{} (MC {} • {})",
                                    prof.name,
                                    prof.game_version,
                                    prof.loader.display_name()
                                );
                                ui.selectable_value(
                                    &mut updated_target_profile,
                                    Some(prof.id.clone()),
                                    lbl,
                                );
                            }
                        });

                    if modal_snap.content_kind == ContentKind::Mod {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(app.i18n.tr(
                                "Tự động kiểm tra và tải kèm các Mod phụ thuộc bắt buộc (Required Dependencies).",
                                "Required library dependencies will be resolved and installed automatically.",
                            ))
                            .size(11.4)
                            .weak(),
                        );
                    }
                }
            }

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if secondary_icon_button(
                    ui,
                    VectorIcon::Close,
                    app.i18n.tr("Đóng", "Close"),
                )
                .clicked()
                {
                    close_modal = true;
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let can_install = modal_snap.content_kind == ContentKind::Modpack
                        || updated_target_profile.is_some();
                    ui.add_enabled_ui(can_install, |ui| {
                        if tactile_button_3d(
                            ui,
                            Some(VectorIcon::Download),
                            app.i18n.tr("Bắt đầu cài đặt", "Start Installation"),
                            ButtonStyle3d::HeroLaunch,
                            vec2(178.0, 36.0),
                        )
                        .clicked()
                        {
                            trigger_start = true;
                        }
                    });
                });
            });
        });

    if let Some(modal) = app.modpacks.install_modal.as_mut() {
        modal.chosen_version_idx = updated_ver_idx;
        modal.profile_name = updated_profile_name;
        modal.ram_mb = updated_ram_mb;
        modal.target_profile_id = updated_target_profile;
        if clear_custom_dir {
            modal.custom_dir = None;
        }
        if minimize_modal {
            modal.minimized = true;
        }
    }
    if minimize_modal {
        app.click_sfx();
    }

    if close_modal {
        app.click_sfx();
        app.modpacks.install_modal = None;
    } else if trigger_pick_dir {
        app.click_sfx();
        app.pick_modpack_custom_dir();
    } else if trigger_cancel {
        app.click_sfx();
        app.cancel_modpack_modal_install();
    } else if trigger_start {
        app.click_sfx();
        app.start_modpack_modal_install();
    }
}

fn paint_pack_thumbnail(
    ui: &mut egui::Ui,
    tex_opt: Option<&egui::TextureHandle>,
    size: f32,
    accent: Color32,
    active_3d_fallback: bool,
) {
    if let Some(tex) = tex_opt {
        let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), Color32::from_rgb(14, 19, 32));
        paint_cover_texture(
            ui.painter(),
            rect,
            tex,
            CornerRadius::same(10),
            Color32::WHITE,
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(10),
            Stroke::new(
                1.1,
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 150),
            ),
            StrokeKind::Inside,
        );
    } else {
        paint_icon_3d(ui, size, Icon3dKind::ModpackCrate, active_3d_fallback);
    }
}

fn paint_cover_texture(
    painter: &egui::Painter,
    rect: Rect,
    tex: &egui::TextureHandle,
    _radius: CornerRadius,
    tint: Color32,
) {
    let sz = tex.size_vec2();
    if sz.x <= 0.0 || sz.y <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
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
    painter.with_clip_rect(rect).image(tex.id(), rect, uv, tint);
}

fn paint_contain_texture(painter: &egui::Painter, bounds: Rect, tex: &egui::TextureHandle) {
    let sz = tex.size_vec2();
    if sz.x <= 0.0 || sz.y <= 0.0 || bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return;
    }
    let scale = (bounds.width() / sz.x).min(bounds.height() / sz.y);
    let draw_sz = vec2(sz.x * scale, sz.y * scale);
    let draw_rect = Rect::from_center_size(bounds.center(), draw_sz);
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    painter.image(tex.id(), draw_rect, uv, Color32::WHITE);
}

fn format_version_combo_label(v: &ProjectVersion) -> String {
    let mc = v.game_versions.first().map(|s| s.as_str()).unwrap_or("—");
    let ldr = v.loaders.first().map(|s| s.as_str()).unwrap_or("");
    if ldr.is_empty() {
        format!(
            "{} [{}] • MC {mc}",
            truncate_chars(&v.name, 38),
            v.release_channel
        )
    } else {
        format!(
            "{} [{}] • {ldr} • MC {mc}",
            truncate_chars(&v.name, 34),
            v.release_channel
        )
    }
}

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if max_chars == 0 {
        return String::new();
    }
    let mut chars = collapsed.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}…", head.trim_end())
    } else {
        head
    }
}

pub(crate) fn pagination_total_pages(total_hits: usize, per_page: usize) -> usize {
    total_hits.div_ceil(per_page.max(1)).max(1)
}

pub(crate) fn version_matches_detail_filters(
    ver: &ProjectVersion,
    channel: &str,
    loader: Option<LoaderType>,
    mc_ver: &str,
) -> bool {
    let ch_ok = channel == "all" || ver.release_channel.eq_ignore_ascii_case(channel);
    let ldr_ok = match loader {
        Some(ldr) => ver
            .loaders
            .iter()
            .any(|l| l.eq_ignore_ascii_case(ldr.as_str())),
        None => true,
    };
    let mc_ok = mc_ver == "all"
        || mc_ver.is_empty()
        || ver.game_versions.iter().any(|g| g == mc_ver);
    ch_ok && ldr_ok && mc_ok
}

pub(crate) fn normalize_project_markdown(raw: &str, fallback_summary: &str) -> String {
    let mut s = raw
        .replace("\r\n", "\n")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<BR>", "\n");

    for tag in [
        "<center>",
        "</center>",
        "<div>",
        "</div>",
        "<p>",
        "</p>",
        "<span>",
        "</span>",
    ] {
        s = s.replace(tag, "\n");
    }

    let cleaned = convert_inline_html_img_tags(&s);
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        fallback_summary.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn convert_inline_html_img_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.to_ascii_lowercase().find("<img") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start..];
        if let Some(end_rel) = after_start.find('>') {
            let tag = &after_start[..=end_rel];
            if let Some(src) = extract_html_attr(tag, "src") {
                let alt = extract_html_attr(tag, "alt").unwrap_or_default();
                out.push_str(&format!("![{alt}]({src})"));
            }
            rest = &after_start[end_rel + 1..];
        } else {
            out.push_str(after_start);
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn extract_html_attr<'a>(tag: &'a str, attr_name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    for quote in ['"', '\''] {
        let pat = format!("{attr_name}={quote}");
        if let Some(pos) = lower.find(&pat) {
            let val_start = pos + pat.len();
            let tail = &tag[val_start..];
            if let Some(end_q) = tail.find(quote) {
                return Some(&tail[..end_q]);
            }
        }
    }
    None
}

fn short_date(iso: Option<&str>) -> Option<&str> {
    let s = iso?.trim();
    if s.is_empty() {
        return None;
    }
    s.split('T').next().filter(|d| !d.is_empty())
}

fn format_downloads(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_truncation_handles_vietnamese_diacritics_safely() {
        let vi = "Gói Modpack tối ưu hiệu năng\ncho máy yếu cực mượt mà";
        let short = truncate_chars(vi, 22);
        assert!(short.ends_with('…'));
        assert!(!short.contains('\n'));
        assert_eq!(truncate_chars("Cobblemon", 20), "Cobblemon");
    }

    #[test]
    fn markdown_normalizer_converts_html_breaks_and_img_tags() {
        let raw = "<center><img src=\"https://cdn.modrinth.com/banner.png\" alt=\"Banner\"></center><br/>Hello **World**";
        let norm = normalize_project_markdown(raw, "Fallback");
        assert!(norm.contains("![Banner](https://cdn.modrinth.com/banner.png)"));
        assert!(norm.contains("Hello **World**"));
        assert!(!norm.contains("<center>"));
        assert_eq!(normalize_project_markdown("   ", "Tóm tắt dự phòng"), "Tóm tắt dự phòng");
    }

    #[test]
    fn pagination_and_version_filtering_are_accurate() {
        assert_eq!(pagination_total_pages(0, 24), 1);
        assert_eq!(pagination_total_pages(24, 24), 1);
        assert_eq!(pagination_total_pages(25, 24), 2);

        let ver = ProjectVersion {
            id: "v1".into(),
            project_id: "p1".into(),
            provider: ProviderSource::Modrinth,
            name: "1.0.0".into(),
            version_number: "1.0.0".into(),
            release_channel: "release".into(),
            published_at: Some("2025-05-01T12:00:00Z".into()),
            game_versions: vec!["1.21.1".into(), "1.21.4".into()],
            loaders: vec!["fabric".into()],
            files: Vec::new(),
            dependencies: Vec::new(),
        };

        assert!(version_matches_detail_filters(
            &ver,
            "release",
            Some(LoaderType::Fabric),
            "1.21.1"
        ));
        assert!(!version_matches_detail_filters(
            &ver,
            "beta",
            Some(LoaderType::Fabric),
            "1.21.1"
        ));
        assert!(!version_matches_detail_filters(
            &ver,
            "release",
            Some(LoaderType::Forge),
            "1.21.1"
        ));
        assert!(!version_matches_detail_filters(
            &ver,
            "release",
            None,
            "1.20.1"
        ));

        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let r = Rect::from_min_size(pos2(4.0, 4.0), vec2(16.0, 16.0));
            for ic in [
                VectorIcon::Grid,
                VectorIcon::List,
                VectorIcon::Image,
                VectorIcon::Close,
            ] {
                paint_vector_icon(ui.painter(), r, ic, ACCENT_EMERALD);
            }
        });
        out.textures_delta.clear();
    }
}
