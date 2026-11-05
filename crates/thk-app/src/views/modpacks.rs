use crate::state::LauncherAppState;
use crate::theme::{ACCENT_AMBER, ACCENT_BLUE, ACCENT_EMERALD};
use crate::views::{badge, primary_button};
use egui::{CornerRadius, Frame, Margin, RichText, ScrollArea};
use egui_commonmark::CommonMarkViewer;
use thk_core::models::ProviderSource;
use thk_mods::SearchSort;

pub fn render_modpacks_view(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading(RichText::new(app.i18n.t("nav.modpacks")).size(20.0).strong());
        ui.add_space(12.0);
        for prov in [
            ProviderSource::Modrinth,
            ProviderSource::CurseForge,
            ProviderSource::Ftb,
            ProviderSource::Technic,
        ] {
            if ui
                .selectable_label(
                    app.modpacks.provider == prov,
                    RichText::new(prov.display_name()).strong(),
                )
                .clicked()
            {
                app.click_sfx();
                app.modpacks.provider = prov;
                app.modpacks.selected = None;
                app.search_modpacks();
            }
        }
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let q_resp = ui.add(
            egui::TextEdit::singleline(&mut app.modpacks.query)
                .hint_text("Tìm kiếm Modpack (VD: Better MC, All the Mods, Cobblemon, RLCraft...)")
                .desired_width(340.0),
        );
        if q_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            app.search_modpacks();
        }

        ui.add(
            egui::TextEdit::singleline(&mut app.modpacks.mc_version_filter)
                .hint_text("Phiên bản MC (VD: 1.21.1)")
                .desired_width(135.0),
        );

        egui::ComboBox::from_id_salt("modpack_sort_combo")
            .selected_text(match app.modpacks.sort {
                SearchSort::Downloads => "Lượt tải cao nhất",
                SearchSort::Relevance => "Độ liên quan",
                SearchSort::Updated => "Mới cập nhật",
                SearchSort::Newest => "Mới phát hành",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.modpacks.sort, SearchSort::Downloads, "Lượt tải cao nhất");
                ui.selectable_value(&mut app.modpacks.sort, SearchSort::Relevance, "Độ liên quan");
                ui.selectable_value(&mut app.modpacks.sort, SearchSort::Updated, "Mới cập nhật");
                ui.selectable_value(&mut app.modpacks.sort, SearchSort::Newest, "Mới phát hành");
            });

        if primary_button(ui, "🔍 Tìm kiếm").clicked() {
            app.search_modpacks();
        }
    });

    ui.separator();

    if let Some((detail, versions)) = app.modpacks.selected.clone() {
        ui.horizontal(|ui| {
            if ui.button("⬅ Quay lại danh sách").clicked() {
                app.modpacks.selected = None;
            }
            ui.heading(RichText::new(&detail.summary.title).strong());
            badge(ui, detail.summary.provider.display_name(), ACCENT_EMERALD);
            badge(
                ui,
                &format!("{} lượt tải", format_downloads(detail.summary.downloads)),
                ACCENT_BLUE,
            );
            if let Some(ref src_url) = detail.source_url {
                if ui.button("🌐 Mở trang gốc").clicked() {
                    let _ = open::that(src_url);
                }
            }
        });

        ui.add_space(6.0);
        Frame::group(ui.style())
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
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
                        && primary_button(ui, "⬇ Cài đặt & Tạo Profile mới").clicked()
                    {
                        app.install_selected_modpack();
                    }
                });
            });

        ui.add_space(8.0);
        ScrollArea::vertical()
            .id_salt("modpack_markdown_detail")
            .show(ui, |ui| {
                Frame::group(ui.style())
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        CommonMarkViewer::new().show(
                            ui,
                            &mut app.modpacks.md_cache,
                            &detail.body_markdown,
                        );
                    });
            });
        return;
    }

    let hits = app.modpacks.hits.clone();
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
                Frame::group(ui.style())
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&hit.title).size(15.5).strong());
                                    ui.label(
                                        RichText::new(format!("bởi {}", hit.author))
                                            .size(12.0)
                                            .weak(),
                                    );
                                    badge(
                                        ui,
                                        &format!("⬇ {}", format_downloads(hit.downloads)),
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
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if primary_button(ui, "Xem & Cài đặt ➡").clicked() {
                                        app.open_modpack_detail(hit);
                                    }
                                },
                            );
                        });
                    });
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
