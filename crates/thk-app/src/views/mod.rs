pub mod accounts;
pub mod home;
pub mod modals;
pub mod modpacks;
pub mod servers;

use crate::fx3d::{
    gaming_card_3d, paint_gaming_3d_backdrop, paint_glowing_progress_bar, paint_icon_3d,
    paint_vector_icon, section_enter_offset, tactile_button_3d, ButtonStyle3d, Icon3dKind,
    VectorIcon,
};
use crate::state::{ActiveSection, LauncherAppState, ToastKind};
use crate::theme::{
    accent_palette, ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_ORANGE,
    ACCENT_RED, ACCENT_VIOLET,
};
use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText,
    Sense, Stroke, Vec2,
};
use thk_network::MotdSpan;

pub fn render_root_ui(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    render_state: Option<&egui_wgpu::RenderState>,
) {
    render_left_nav_rail(app, ui);
    render_bottom_status_bar(app, ui);

    let bg_tex_id = app.custom_bg_tex.as_ref().map(|(_, tex)| tex.id());
    let dark_mode = app.settings.theme == thk_core::models::ThemeMode::Dark;
    let central_frame = if bg_tex_id.is_some() {
        let scrim = if dark_mode {
            Color32::from_rgba_unmultiplied(9, 11, 20, 208)
        } else {
            Color32::from_rgba_unmultiplied(241, 245, 251, 214)
        };
        Frame::central_panel(ui.style())
            .fill(scrim)
            .inner_margin(Margin::symmetric(18, 14))
    } else {
        Frame::central_panel(ui.style()).inner_margin(Margin::symmetric(18, 14))
    };

    let avail = ui.available_rect_before_wrap();
    if let Some(tex_id) = bg_tex_id {
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        ui.painter().image(tex_id, avail, uv, Color32::WHITE);
    }

    let sec_key = match app.section {
        ActiveSection::Home => 1,
        ActiveSection::Modpacks => 2,
        ActiveSection::Servers => 3,
        ActiveSection::Accounts => 4,
    };

    egui::CentralPanel::default()
        .frame(central_frame)
        .show(ui, |ui| {
            let panel_rect = ui.max_rect();
            paint_gaming_3d_backdrop(ui, panel_rect, dark_mode, bg_tex_id.is_some());

            let (_, y_offset) = section_enter_offset(ui.ctx(), sec_key);
            if y_offset > 0.2 {
                ui.add_space(y_offset);
            }

            match app.section {
                ActiveSection::Home => home::render_home_view(app, ui, render_state),
                ActiveSection::Modpacks => modpacks::render_modpacks_view(app, ui),
                ActiveSection::Servers => servers::render_servers_view(app, ui),
                ActiveSection::Accounts => accounts::render_accounts_view(app, ui, render_state),
            }
        });

    modals::render_all_modals(app, ui.ctx());
    render_toast_overlay(app, ui.ctx());
}

fn render_left_nav_rail(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let primary_accent = accent_palette(app.settings.accent_preset).0;
    egui::Panel::left("thk_left_nav_rail")
        .resizable(false)
        .exact_size(236.0)
        .frame(
            Frame::side_top_panel(ui.style())
                .inner_margin(Margin::symmetric(12, 14)),
        )
        .show(ui, |ui| {
            ui.vertical(|ui| {
                gaming_card_3d(
                    ui,
                    "nav_brand_showcase",
                    false,
                    primary_accent,
                    Margin::symmetric(10, 8),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            paint_icon_3d(ui, 40.0, Icon3dKind::BrandVoxelCore, true);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(
                                    RichText::new("TriHienKun")
                                        .strong()
                                        .size(16.0)
                                        .color(primary_accent),
                                );
                                ui.label(
                                    RichText::new("Laucher • 3D wgpu")
                                        .size(11.2)
                                        .color(ACCENT_CYAN),
                                );
                            });
                        });
                    },
                );

                ui.add_space(10.0);

                let nav_items = [
                    (
                        ActiveSection::Home,
                        Icon3dKind::HomePortal,
                        primary_accent,
                        app.i18n.t("nav.home").to_string(),
                    ),
                    (
                        ActiveSection::Modpacks,
                        Icon3dKind::ModpackCrate,
                        ACCENT_AMBER,
                        app.i18n.t("nav.modpacks").to_string(),
                    ),
                    (
                        ActiveSection::Servers,
                        Icon3dKind::ServerBladeTower,
                        ACCENT_CYAN,
                        app.i18n.t("nav.servers").to_string(),
                    ),
                    (
                        ActiveSection::Accounts,
                        Icon3dKind::AccountCharacter,
                        ACCENT_VIOLET,
                        app.i18n.t("nav.accounts").to_string(),
                    ),
                ];

                for (sec, icon_kind, accent, label) in nav_items {
                    let is_active = app.section == sec;
                    let (_, card_resp) = gaming_card_3d(
                        ui,
                        ("nav_sec_card", sec as u8),
                        is_active,
                        accent,
                        Margin::symmetric(10, 6),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                paint_icon_3d(ui, 28.0, icon_kind, is_active);
                                ui.add_space(4.0);
                                let text_col = if is_active {
                                    Color32::WHITE
                                } else {
                                    ui.visuals().text_color()
                                };
                                ui.label(
                                    RichText::new(label)
                                        .size(13.6)
                                        .strong()
                                        .color(text_col),
                                );
                            });
                        },
                    );
                    if card_resp.interact(Sense::click()).clicked() {
                        app.set_section(sec);
                    }
                    ui.add_space(2.0);
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);
                ui.label(
                    RichText::new(app.i18n.tr("Công cụ hệ thống", "System tools"))
                        .size(11.5)
                        .weak()
                        .strong(),
                );
                ui.add_space(4.0);

                let tools: [(u8, Icon3dKind, Color32, String); 3] = [
                    (
                        1,
                        Icon3dKind::JavaReactor,
                        ACCENT_ORANGE,
                        app.i18n.t("nav.java").to_string(),
                    ),
                    (
                        2,
                        Icon3dKind::LanRadar,
                        ACCENT_CYAN,
                        app.i18n.t("nav.lan").to_string(),
                    ),
                    (
                        3,
                        Icon3dKind::SettingsGyro,
                        ACCENT_VIOLET,
                        app.i18n.t("nav.settings").to_string(),
                    ),
                ];

                for (tool_id, icon_kind, accent, label) in tools {
                    let is_open = match tool_id {
                        1 => app.show_java_modal,
                        2 => app.show_lan_modal,
                        _ => app.show_settings_modal,
                    };
                    let (_, resp) = gaming_card_3d(
                        ui,
                        ("nav_tool_card", tool_id),
                        is_open,
                        accent,
                        Margin::symmetric(10, 5),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                paint_icon_3d(ui, 25.0, icon_kind, is_open);
                                ui.add_space(4.0);
                                ui.label(RichText::new(label).size(12.8).strong());
                            });
                        },
                    );
                    if resp.interact(Sense::click()).clicked() {
                        app.click_sfx();
                        match tool_id {
                            1 => app.open_java_manager(),
                            2 => app.show_lan_modal = true,
                            _ => {
                                app.show_settings_modal = true;
                                app.refresh_cache_size();
                            }
                        }
                    }
                    ui.add_space(2.0);
                }

                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(format!("v{} • Pure Rust + wgpu 30", env!("CARGO_PKG_VERSION")))
                            .size(10.8)
                            .weak(),
                    );
                    ui.add_space(4.0);
                    let sel_acct = app.selected_account().cloned();
                    gaming_card_3d(
                        ui,
                        "nav_active_player_hud",
                        sel_acct.is_some(),
                        ACCENT_EMERALD,
                        Margin::symmetric(10, 7),
                        |ui| {
                            ui.set_width(ui.available_width());
                            if let Some(acct) = sel_acct {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(&acct.username)
                                            .size(13.0)
                                            .strong()
                                            .color(ACCENT_EMERALD),
                                    );
                                    badge(ui, acct.account_type.as_str(), ACCENT_BLUE);
                                });
                            } else {
                                ui.label(
                                    RichText::new(
                                        app.i18n.tr("Chưa chọn tài khoản", "No account selected"),
                                    )
                                    .size(11.8)
                                    .weak(),
                                );
                            }
                        },
                    );
                });
            });
        });
}

fn render_bottom_status_bar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let minimized_mp_title = app
        .modpacks
        .install_modal
        .as_ref()
        .filter(|m| m.installing && m.minimized)
        .map(|m| m.detail.summary.title.clone());
    if app.active_progress.is_none() && app.busy_message.is_none() && minimized_mp_title.is_none() {
        return;
    }

    egui::Panel::bottom("thk_bottom_progress_bar")
        .resizable(false)
        .exact_size(48.0)
        .frame(
            Frame::side_top_panel(ui.style())
                .inner_margin(Margin::symmetric(16, 8)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if let Some(ref prog) = app.active_progress {
                    let frac = (prog.percent / 100.0).clamp(0.0, 1.0);
                    let speed = format!("{}/s", format_bytes(prog.bytes_per_sec));
                    paint_glowing_progress_bar(ui, frac, 260.0, 18.0);
                    ui.add_space(8.0);
                    badge(ui, &prog.phase, ACCENT_CYAN);
                    ui.label(
                        RichText::new(format!(
                            "{} ({}/{})",
                            prog.message, prog.completed_files, prog.total_files
                        ))
                        .size(12.2)
                        .strong(),
                    );
                    badge(ui, &speed, ACCENT_EMERALD);
                } else if let Some(ref msg) = app.busy_message {
                    ui.spinner();
                    ui.label(RichText::new(msg).size(12.5).strong());
                } else if let Some(ref mp_title) = minimized_mp_title {
                    ui.spinner();
                    ui.label(
                        RichText::new(format!(
                            "{}: {mp_title}",
                            app.i18n.tr("Đang chuẩn bị cài đặt", "Preparing installation")
                        ))
                        .size(12.4)
                        .strong(),
                    );
                }

                if minimized_mp_title.is_some() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if danger_icon_button(
                            ui,
                            VectorIcon::Close,
                            app.i18n.tr("Hủy", "Cancel"),
                        )
                        .clicked()
                        {
                            app.click_sfx();
                            app.cancel_modpack_modal_install();
                        }
                        if secondary_icon_button(
                            ui,
                            VectorIcon::Download,
                            app.i18n.tr("Mở cửa sổ cài đặt", "Show Installer"),
                        )
                        .clicked()
                        {
                            app.click_sfx();
                            if let Some(m) = app.modpacks.install_modal.as_mut() {
                                m.minimized = false;
                            }
                        }
                    });
                }
            });
        });
}

fn render_toast_overlay(app: &LauncherAppState, ctx: &egui::Context) {
    if app.toasts.is_empty() {
        return;
    }

    egui::Area::new(egui::Id::unique("thk_toasts_overlay"))
        .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-18.0, -58.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                for toast in &app.toasts {
                    let (border_col, v_icon) = match toast.kind {
                        ToastKind::Success => (ACCENT_EMERALD, VectorIcon::Sparkles),
                        ToastKind::Error => (ACCENT_RED, VectorIcon::Shield),
                        ToastKind::Info => (ACCENT_CYAN, VectorIcon::Globe),
                    };
                    Frame::popup(ui.style())
                        .stroke(Stroke::new(1.5, border_col))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(12, 9))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (ic_rect, _) =
                                    ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                                paint_vector_icon(ui.painter(), ic_rect, v_icon, border_col);
                                ui.label(RichText::new(&toast.text).size(12.5).strong());
                            });
                        });
                }
            });
        });
}

pub fn primary_button(ui: &mut egui::Ui, label: impl Into<String>) -> egui::Response {
    tactile_button_3d(
        ui,
        None,
        label,
        ButtonStyle3d::PrimaryEmerald,
        Vec2::ZERO,
    )
}

pub fn primary_icon_button(
    ui: &mut egui::Ui,
    icon: VectorIcon,
    label: impl Into<String>,
) -> egui::Response {
    tactile_button_3d(
        ui,
        Some(icon),
        label,
        ButtonStyle3d::PrimaryEmerald,
        Vec2::ZERO,
    )
}

pub fn accent_icon_button(
    ui: &mut egui::Ui,
    icon: VectorIcon,
    label: impl Into<String>,
) -> egui::Response {
    tactile_button_3d(
        ui,
        Some(icon),
        label,
        ButtonStyle3d::AccentViolet,
        Vec2::ZERO,
    )
}

pub fn secondary_button(ui: &mut egui::Ui, label: impl Into<String>) -> egui::Response {
    tactile_button_3d(
        ui,
        None,
        label,
        ButtonStyle3d::SecondaryGlass,
        Vec2::ZERO,
    )
}

pub fn secondary_icon_button(
    ui: &mut egui::Ui,
    icon: VectorIcon,
    label: impl Into<String>,
) -> egui::Response {
    tactile_button_3d(
        ui,
        Some(icon),
        label,
        ButtonStyle3d::SecondaryGlass,
        Vec2::ZERO,
    )
}

pub fn danger_button(ui: &mut egui::Ui, label: impl Into<String>) -> egui::Response {
    tactile_button_3d(ui, None, label, ButtonStyle3d::DangerRose, Vec2::ZERO)
}

pub fn danger_icon_button(
    ui: &mut egui::Ui,
    icon: VectorIcon,
    label: impl Into<String>,
) -> egui::Response {
    tactile_button_3d(
        ui,
        Some(icon),
        label,
        ButtonStyle3d::DangerRose,
        Vec2::ZERO,
    )
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            34,
        ))
        .stroke(Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 190),
        ))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(color).strong());
        });
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

pub fn format_duration_hms(seconds: u64) -> String {
    let hrs = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hrs > 0 {
        format!("{hrs}h {mins:02}m")
    } else if mins > 0 {
        format!("{mins}m {secs:02}s")
    } else {
        format!("{secs}s")
    }
}

pub fn render_motd_spans(ui: &mut egui::Ui, spans: &[MotdSpan]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for sp in spans {
            let mut rt = RichText::new(&sp.text).size(12.5);
            if sp.bold {
                rt = rt.strong();
            }
            if sp.italic {
                rt = rt.italics();
            }
            if let Some(ref hex) = sp.color_hex {
                if let Some(col) = parse_hex_color(hex) {
                    rt = rt.color(col);
                }
            } else {
                rt = rt.color(ACCENT_AMBER);
            }
            ui.label(rt);
        }
    });
}

fn parse_hex_color(hex: &str) -> Option<Color32> {
    let clean = hex.trim_start_matches('#');
    if clean.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
    let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
    let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}
