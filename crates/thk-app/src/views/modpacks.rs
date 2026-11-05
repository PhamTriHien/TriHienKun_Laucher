use crate::fx3d::{
    gaming_card_3d, paint_icon_3d, sliding_pill_tabs, Icon3dKind, VectorIcon,
};
use crate::state::LauncherAppState;
use crate::theme::{
    ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_ORANGE, ACCENT_VIOLET,
};
use crate::views::{badge, primary_icon_button, secondary_icon_button};
use egui::{Color32, Margin, RichText, ScrollArea};
use egui_commonmark::CommonMarkViewer;
use thk_core::models::ProviderSource;
use thk_mods::SearchSort;

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

pub fn render_modpacks_view(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        paint_icon_3d(ui, 34.0, Icon3dKind::ModpackCrate, true);
        ui.heading(RichText::new(app.i18n.t("nav.modpacks")).size(20.5).strong());
        ui.add_space(10.0);

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
            app.modpacks.selected = None;
            app.search_modpacks();
        }
    });

    ui.add_space(8.0);
    gaming_card_3d(
        ui,
        "modpacks_search_toolbar",
        false,
        provider_accent(app.modpacks.provider),
        Margin::symmetric(12, 9),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let q_resp = ui.add(
                    egui::TextEdit::singleline(&mut app.modpacks.query)
                        .hint_text(
                            "Tìm kiếm Modpack (VD: Better MC, All the Mods, Cobblemon, RLCraft...)",
                        )
                        .desired_width(340.0),
                );
                if q_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    app.search_modpacks();
                }

                ui.add(
                    egui::TextEdit::singleline(&mut app.modpacks.mc_version_filter)
                        .hint_text("Phiên bản MC (VD: 1.21.1)")
                        .desired_width(138.0),
                );

                egui::ComboBox::from_id_salt("modpack_sort_combo")
                    .selected_text(match app.modpacks.sort {
                        SearchSort::Downloads => app.i18n.tr("Lượt tải cao nhất", "Most Downloads"),
                        SearchSort::Relevance => app.i18n.tr("Độ liên quan", "Relevance"),
                        SearchSort::Updated => app.i18n.tr("Mới cập nhật", "Recently Updated"),
                        SearchSort::Newest => app.i18n.tr("Mới phát hành", "Newest"),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut app.modpacks.sort,
                            SearchSort::Downloads,
                            app.i18n.tr("Lượt tải cao nhất", "Most Downloads"),
                        );
                        ui.selectable_value(
                            &mut app.modpacks.sort,
                            SearchSort::Relevance,
                            app.i18n.tr("Độ liên quan", "Relevance"),
                        );
                        ui.selectable_value(
                            &mut app.modpacks.sort,
                            SearchSort::Updated,
                            app.i18n.tr("Mới cập nhật", "Recently Updated"),
                        );
                        ui.selectable_value(
                            &mut app.modpacks.sort,
                            SearchSort::Newest,
                            app.i18n.tr("Mới phát hành", "Newest"),
                        );
                    });

                if primary_icon_button(
                    ui,
                    VectorIcon::Search,
                    app.i18n.tr("Tìm kiếm", "Search"),
                )
                .clicked()
                {
                    app.search_modpacks();
                }
            });
        },
    );

    ui.add_space(10.0);

    if let Some((detail, versions)) = app.modpacks.selected.clone() {
        let p_col = provider_accent(detail.summary.provider);
        ui.horizontal_wrapped(|ui| {
            if secondary_icon_button(ui, VectorIcon::ArrowLeft, "Quay lại danh sách").clicked() {
                app.modpacks.selected = None;
            }
            ui.heading(RichText::new(&detail.summary.title).size(19.5).strong());
            badge(ui, detail.summary.provider.display_name(), p_col);
            badge(
                ui,
                &format!("{} lượt tải", format_downloads(detail.summary.downloads)),
                ACCENT_BLUE,
            );
            if let Some(ref src_url) = detail.source_url {
                if secondary_icon_button(ui, VectorIcon::Globe, "Mở trang gốc").clicked() {
                    let _ = open::that(src_url);
                }
            }
        });

        ui.add_space(8.0);
        gaming_card_3d(
            ui,
            "modpack_install_bar_card",
            true,
            p_col,
            Margin::same(12),
            |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Phiên bản cài đặt:").strong());
                    let current_ver_label = versions
                        .get(app.modpacks.chosen_version_idx)
                        .map(|v| {
                            let mc = v.game_versions.first().cloned().unwrap_or_default();
                            format!("{} ({}) [MC {mc}]", v.name, v.release_channel)
                        })
                        .unwrap_or_else(|| "Không có phiên bản".into());

                    egui::ComboBox::from_id_salt("modpack_version_picker")
                        .width(340.0)
                        .selected_text(current_ver_label)
                        .show_ui(ui, |ui| {
                            for (idx, v) in versions.iter().enumerate() {
                                let mc = v.game_versions.first().cloned().unwrap_or_default();
                                let lbl = format!("{} ({}) [MC {mc}]", v.name, v.release_channel);
                                ui.selectable_value(&mut app.modpacks.chosen_version_idx, idx, lbl);
                            }
                        });

                    if !versions.is_empty()
                        && primary_icon_button(
                            ui,
                            VectorIcon::Download,
                            "Cài đặt & Tạo Profile mới",
                        )
                        .clicked()
                    {
                        app.install_selected_modpack();
                    }
                });
            },
        );

        ui.add_space(8.0);
        ScrollArea::vertical()
            .id_salt("modpack_markdown_detail")
            .show(ui, |ui| {
                gaming_card_3d(
                    ui,
                    "modpack_md_body_card",
                    false,
                    p_col,
                    Margin::same(16),
                    |ui| {
                        ui.set_width(ui.available_width());
                        CommonMarkViewer::new().show(
                            ui,
                            &mut app.modpacks.md_cache,
                            &detail.body_markdown,
                        );
                    },
                );
            });
        return;
    }

    let hits = app.modpacks.hits.clone();
    let prov_col = provider_accent(app.modpacks.provider);

    ScrollArea::vertical()
        .id_salt("modpacks_grid_scroll")
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "Tìm thấy {} gói Modpack trên {}",
                    app.modpacks.total_hits.max(hits.len()),
                    app.modpacks.provider.display_name()
                ))
                .size(12.0)
                .weak(),
            );
            ui.add_space(6.0);

            for hit in hits {
                let hit_id = hit.project_id.clone();
                gaming_card_3d(
                    ui,
                    ("mp_hit_card", hit_id),
                    false,
                    prov_col,
                    Margin::symmetric(14, 10),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            paint_icon_3d(ui, 44.0, Icon3dKind::ModpackCrate, false);
                            ui.add_space(4.0);

                            let right_reserve = 152.0;
                            let text_w = (ui.available_width() - right_reserve).max(160.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(text_w, 46.0),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new(&hit.title).size(15.5).strong());
                                        ui.label(
                                            RichText::new(format!("bởi {}", hit.author))
                                                .size(12.0)
                                                .weak(),
                                        );
                                        badge(
                                            ui,
                                            &format!("{} lượt tải", format_downloads(hit.downloads)),
                                            ACCENT_AMBER,
                                        );
                                    });
                                    ui.label(RichText::new(&hit.summary).size(12.5));
                                    if !hit.categories.is_empty() {
                                        ui.horizontal_wrapped(|ui| {
                                            for cat in hit.categories.iter().take(5) {
                                                badge(ui, cat, ACCENT_BLUE);
                                            }
                                        });
                                    }
                                },
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if primary_icon_button(
                                        ui,
                                        VectorIcon::ArrowRight,
                                        "Xem & Cài đặt",
                                    )
                                    .clicked()
                                    {
                                        app.open_modpack_detail(hit);
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

fn format_downloads(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
