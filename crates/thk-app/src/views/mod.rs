pub mod accounts;
pub mod home;
pub mod modals;
pub mod modpacks;
pub mod servers;

use crate::state::{ActiveSection, LauncherAppState, ToastKind};
use crate::theme::{ACCENT_AMBER, ACCENT_BLUE, ACCENT_EMERALD, ACCENT_EMERALD_DIM, ACCENT_RED};
use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Vec2};
use thk_network::MotdSpan;

pub fn render_root_ui(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    render_state: Option<&egui_wgpu::RenderState>,
) {
    render_left_nav_rail(app, ui);
    render_bottom_status_bar(app, ui);

    egui::CentralPanel::default()
        .frame(
            Frame::central_panel(ui.style())
                .inner_margin(Margin::symmetric(18, 14)),
        )
        .show(ui, |ui| match app.section {
            ActiveSection::Home => home::render_home_view(app, ui),
            ActiveSection::Modpacks => modpacks::render_modpacks_view(app, ui),
            ActiveSection::Servers => servers::render_servers_view(app, ui),
            ActiveSection::Accounts => accounts::render_accounts_view(app, ui, render_state),
        });

    modals::render_all_modals(app, ui.ctx());
    render_toast_overlay(app, ui.ctx());
}

fn render_left_nav_rail(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    egui::Panel::left("thk_left_nav_rail")
        .resizable(false)
        .exact_size(216.0)
        .frame(
            Frame::side_top_panel(ui.style())
                .inner_margin(Margin::symmetric(12, 14)),
        )
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(8), ACCENT_EMERALD_DIM);
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "T",
                        egui::FontId::proportional(19.0),
                        Color32::WHITE,
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(
                            RichText::new("TriHienKun")
                                .strong()
                                .size(15.5)
                                .color(ACCENT_EMERALD),
                        );
                        ui.label(RichText::new("Laucher Pure Rust").size(11.0).weak());
                    });
                });

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);

                let nav_items = [
                    (ActiveSection::Home, "🏠", app.i18n.t("nav.home").to_string()),
                    (
                        ActiveSection::Modpacks,
                        "📦",
                        app.i18n.t("nav.modpacks").to_string(),
                    ),
                    (
                        ActiveSection::Servers,
                        "🖥",
                        app.i18n.t("nav.servers").to_string(),
                    ),
                    (
                        ActiveSection::Accounts,
                        "👤",
                        app.i18n.t("nav.accounts").to_string(),
                    ),
                ];

                for (sec, icon, label) in nav_items {
                    let is_active = app.section == sec;
                    let fill = if is_active {
                        Color32::from_rgba_unmultiplied(34, 197, 94, 42)
                    } else {
                        Color32::TRANSPARENT
                    };
                    let stroke = if is_active {
                        Stroke::new(1.2, ACCENT_EMERALD)
                    } else {
                        Stroke::NONE
                    };
                    let btn = egui::Button::new(
                        RichText::new(format!("{icon}  {label}"))
                            .size(13.5)
                            .strong(),
                    )
                    .fill(fill)
                    .stroke(stroke)
                    .min_size(Vec2::new(ui.available_width(), 36.0));

                    if ui.add(btn).clicked() {
                        app.set_section(sec);
                    }
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(RichText::new("CÔNG CỤ HỆ THỐNG").size(10.5).weak().strong());
                ui.add_space(4.0);

                let java_label = format!("☕  {}", app.i18n.t("nav.java"));
                if ui
                    .add_sized(
                        [ui.available_width(), 33.0],
                        egui::Button::new(RichText::new(java_label).size(13.0)),
                    )
                    .clicked()
                {
                    app.click_sfx();
                    app.open_java_manager();
                }

                let lan_label = format!("🌐  {}", app.i18n.t("nav.lan"));
                if ui
                    .add_sized(
                        [ui.available_width(), 33.0],
                        egui::Button::new(RichText::new(lan_label).size(13.0)),
                    )
                    .clicked()
                {
                    app.click_sfx();
                    app.show_lan_modal = true;
                }

                let settings_label = format!("⚙  {}", app.i18n.t("nav.settings"));
                if ui
                    .add_sized(
                        [ui.available_width(), 33.0],
                        egui::Button::new(RichText::new(settings_label).size(13.0)),
                    )
                    .clicked()
                {
                    app.click_sfx();
                    app.show_settings_modal = true;
                }

                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(format!("v{} • wgpu 30", env!("CARGO_PKG_VERSION")))
                            .size(11.0)
                            .weak(),
                    );
                    if let Some(acct) = app.selected_account() {
                        ui.label(
                            RichText::new(format!(
                                "Đang chọn: {} ({})",
                                acct.username,
                                acct.account_type.as_str()
                            ))
                            .size(11.5)
                            .color(ACCENT_EMERALD),
                        );
                    } else {
                        ui.label(RichText::new("Chưa chọn tài khoản").size(11.5).weak());
                    }
                });
            });
        });
}

fn render_bottom_status_bar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    if app.active_progress.is_none() && app.busy_message.is_none() {
        return;
    }

    egui::Panel::bottom("thk_bottom_progress_bar")
        .resizable(false)
        .exact_size(46.0)
        .frame(
            Frame::side_top_panel(ui.style())
                .inner_margin(Margin::symmetric(16, 8)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if let Some(ref prog) = app.active_progress {
                    let frac = (prog.percent / 100.0).clamp(0.0, 1.0);
                    let speed = format!("{}/s", format_bytes(prog.bytes_per_sec));
                    let summary = format!(
                        "[{}] {} ({}/{}) • {}",
                        prog.phase, prog.message, prog.completed_files, prog.total_files, speed
                    );
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .desired_width(280.0)
                            .show_percentage(),
                    );
                    ui.add_space(8.0);
                    ui.label(RichText::new(summary).size(12.0));
                } else if let Some(ref msg) = app.busy_message {
                    ui.spinner();
                    ui.label(RichText::new(msg).size(12.5));
                }
            });
        });
}

fn render_toast_overlay(app: &LauncherAppState, ctx: &egui::Context) {
    if app.toasts.is_empty() {
        return;
    }

    egui::Area::new(egui::Id::unique("thk_toasts_overlay"))
        .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-18.0, -56.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                for toast in &app.toasts {
                    let (border_col, prefix) = match toast.kind {
                        ToastKind::Success => (ACCENT_EMERALD, "✔"),
                        ToastKind::Error => (ACCENT_RED, "✖"),
                        ToastKind::Info => (ACCENT_BLUE, "ℹ"),
                    };
                    Frame::popup(ui.style())
                        .stroke(Stroke::new(1.5, border_col))
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("{prefix}  {}", toast.text))
                                    .size(12.5)
                                    .strong(),
                            );
                        });
                }
            });
        });
}

pub fn primary_button(ui: &mut egui::Ui, label: impl Into<String>) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(label.into())
                .color(Color32::WHITE)
                .strong(),
        )
        .fill(ACCENT_EMERALD_DIM)
        .stroke(Stroke::new(1.0, ACCENT_EMERALD)),
    )
}

pub fn danger_button(ui: &mut egui::Ui, label: impl Into<String>) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label.into()).color(Color32::WHITE).strong())
            .fill(Color32::from_rgb(153, 27, 27))
            .stroke(Stroke::new(1.0, ACCENT_RED)),
    )
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            36,
        ))
        .stroke(Stroke::new(1.0, color))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(Margin::symmetric(6, 2))
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
