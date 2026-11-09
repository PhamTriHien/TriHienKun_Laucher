pub mod accounts;
pub mod home;
pub mod mobile_game;
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
    accent_palette, readable_accent_ink, ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD,
    ACCENT_ORANGE, ACCENT_RED, ACCENT_VIOLET, BG_PANEL_DARK, BG_PANEL_LIGHT, BG_SKY_LIGHT,
    BG_VOID_DARK, TEXT_INK_LIGHT,
};
use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense,
    Stroke, Vec2,
};
use thk_core::models::Language;
use thk_network::{ConnectedNetworkSnapshot, DirectHostActiveInfo, MotdSpan, UpnpPortStatus};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SafeAreaInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl SafeAreaInsets {
    pub fn from_android_permissions(
        perms: &thk_core::AndroidPermissionStatus,
        pixels_per_point: f32,
    ) -> Self {
        let ppp = pixels_per_point.max(0.5);
        Self {
            left: perms.safe_inset_left_px as f32 / ppp,
            top: perms.safe_inset_top_px as f32 / ppp,
            right: perms.safe_inset_right_px as f32 / ppp,
            bottom: perms.safe_inset_bottom_px as f32 / ppp,
        }
    }
}

fn safe_area_temp_id() -> egui::Id {
    egui::Id::unique("thk_active_safe_area_insets")
}

pub fn current_safe_area_insets(ctx: &egui::Context) -> SafeAreaInsets {
    ctx.data(|d| d.get_temp::<SafeAreaInsets>(safe_area_temp_id()))
        .unwrap_or_default()
}

pub fn apply_safe_area_to_rect(rect: Rect, insets: SafeAreaInsets) -> Rect {
    let max_x = (rect.width() * 0.28).max(0.0);
    let max_y = (rect.height() * 0.28).max(0.0);
    let l = insets.left.clamp(0.0, max_x);
    let r = insets.right.clamp(0.0, max_x);
    let t = insets.top.clamp(0.0, max_y);
    let b = insets.bottom.clamp(0.0, max_y);
    let min_x = rect.left() + l;
    let min_y = rect.top() + t;
    let max_rx = (rect.right() - r).max(min_x + 32.0_f32.min(rect.width()));
    let max_ry = (rect.bottom() - b).max(min_y + 32.0_f32.min(rect.height()));
    Rect::from_min_max(pos2(min_x, min_y), pos2(max_rx, max_ry))
}

pub fn safe_content_rect(ctx: &egui::Context) -> Rect {
    apply_safe_area_to_rect(ctx.content_rect(), current_safe_area_insets(ctx))
}

fn margin_i8(v: f32) -> i8 {
    v.round().clamp(0.0, 120.0) as i8
}

pub fn is_compact_portrait(rect: Rect) -> bool {
    rect.width() < 680.0 || rect.height() > rect.width() * 1.04
}

pub(crate) fn locked_panel_body<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let rect = ui.max_rect();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
    let out = add_contents(&mut child);
    ui.advance_cursor_after_rect(rect);
    out
}

pub(crate) fn bounded_wrap_label(ui: &mut egui::Ui, rich: RichText, max_w: f32) -> egui::Response {
    let cap_w = max_w.max(36.0);
    let mut job = egui::text::LayoutJob::default();
    rich.clone().append_to(
        &mut job,
        ui.style(),
        egui::FontSelection::Default,
        egui::Align::LEFT,
    );
    let single_galley = ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
    if single_galley.size().x <= cap_w + 0.5 {
        return ui.add(egui::Label::new(rich).extend());
    }
    job.wrap.max_width = cap_w;
    let mut galley = ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
    if galley.size().x > cap_w + 1.0 {
        job.wrap.break_anywhere = true;
        galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    }
    ui.add(egui::Label::new(galley))
}

pub fn render_root_ui(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    render_state: Option<&egui_wgpu::RenderState>,
) {
    let insets = SafeAreaInsets::from_android_permissions(
        &app.android_permissions,
        ui.ctx().pixels_per_point(),
    );
    ui.ctx()
        .data_mut(|d| d.insert_temp(safe_area_temp_id(), insets));

    if app.mobile_game.screen_open && !app.mobile_game.hub_visible_over_game {
        if let Some(vp) = app.accounts.skin_viewport.as_mut() {
            vp.release_offscreen_target(render_state);
        }
        app.accounts.loaded_viewport_key.clear();
        mobile_game::render_mobile_game_screen(app, ui);
        modals::render_all_modals(app, ui.ctx());
        render_toast_overlay(app, ui.ctx());
        return;
    }

    if app.section != ActiveSection::Accounts
        && (app.settings.low_ram_mode || app.section != ActiveSection::Home)
    {
        if let Some(vp) = app.accounts.skin_viewport.as_mut() {
            vp.release_offscreen_target(render_state);
        }
        app.accounts.loaded_viewport_key.clear();
    }

    let compact_portrait = is_compact_portrait(ui.max_rect());
    if compact_portrait {
        render_portrait_top_bar(app, ui, insets);
        render_portrait_bottom_dock(app, ui, insets);
    } else {
        render_left_nav_rail(app, ui, insets);
    }
    let status_bar_visible = render_bottom_status_bar(app, ui, insets, compact_portrait);

    let bg_tex_id = app.custom_bg_tex.as_ref().map(|(_, tex)| tex.id());
    let dark_mode = app.settings.theme == thk_core::models::ThemeMode::Dark;
    let margin = if compact_portrait {
        Margin {
            left: margin_i8(10.0 + insets.left),
            right: margin_i8(10.0 + insets.right),
            top: 10,
            bottom: 10,
        }
    } else {
        let bottom_extra = if status_bar_visible {
            0.0
        } else {
            insets.bottom
        };
        Margin {
            left: 18,
            right: margin_i8(18.0 + insets.right),
            top: margin_i8(14.0 + insets.top),
            bottom: margin_i8(14.0 + bottom_extra),
        }
    };
    let base_bg = if dark_mode {
        BG_VOID_DARK
    } else {
        BG_SKY_LIGHT
    };
    let central_frame = if bg_tex_id.is_some() {
        let scrim = if dark_mode {
            Color32::from_rgba_unmultiplied(20, 29, 44, 212)
        } else {
            Color32::from_rgba_unmultiplied(235, 243, 252, 218)
        };
        Frame::central_panel(ui.style())
            .fill(scrim)
            .inner_margin(margin)
    } else {
        Frame::central_panel(ui.style())
            .fill(base_bg)
            .inner_margin(margin)
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

    if app.mobile_game.screen_open
        || (cfg!(target_os = "android") && !app.instance_supervisor.running_keys().is_empty())
    {
        mobile_game::render_mobile_floating_menu(app, ui.ctx());
    }

    modals::render_all_modals(app, ui.ctx());
    render_toast_overlay(app, ui.ctx());
}

pub(super) fn section_navigation_card(
    ui: &mut egui::Ui,
    section: ActiveSection,
    label: &str,
    icon: Icon3dKind,
    accent: Color32,
    selected: bool,
) -> egui::Response {
    let (_, response) = gaming_card_3d(
        ui,
        ("nav_sec_card", section as u8),
        selected,
        accent,
        Margin::symmetric(10, 6),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                paint_icon_3d(ui, 28.0, icon, selected);
                ui.add_space(4.0);
                let text_color = if selected && ui.visuals().dark_mode {
                    Color32::WHITE
                } else if selected {
                    TEXT_INK_LIGHT
                } else {
                    ui.visuals().text_color()
                };
                let rt = RichText::new(label).size(13.6).strong().color(text_color);
                if ui.available_width() < 120.0 {
                    ui.add(egui::Label::new(rt).truncate());
                } else {
                    ui.label(rt);
                }
            });
        },
    );
    let response = response.interact(Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_aria_current(if selected {
            egui::accesskit::AriaCurrent::Page
        } else {
            egui::accesskit::AriaCurrent::False
        });
    });
    response
}

fn render_left_nav_rail(app: &mut LauncherAppState, ui: &mut egui::Ui, insets: SafeAreaInsets) {
    let primary_accent = accent_palette(app.settings.accent_preset).0;
    let avail_w = ui.available_width();
    let compact_landscape =
        avail_w < 900.0 || insets.left + insets.right > 0.0 || cfg!(target_os = "android");
    let base_min = if compact_landscape { 168.0 } else { 184.0 };
    let base_def = if compact_landscape { 200.0 } else { 236.0 };
    let min_nav_w = (base_min + insets.left).min((avail_w * 0.34).max(132.0));
    let max_nav_w = (avail_w - 360.0)
        .min(avail_w * 0.36 + insets.left)
        .min(380.0 + insets.left)
        .max(min_nav_w);
    let def_nav_w = (base_def + insets.left).clamp(min_nav_w, max_nav_w);
    let rail_margin = Margin {
        left: margin_i8(12.0 + insets.left),
        right: 12,
        top: margin_i8(14.0 + insets.top),
        bottom: margin_i8(14.0 + insets.bottom),
    };
    egui::Panel::left("thk_left_nav_rail")
        .resizable(!cfg!(target_os = "android"))
        .default_size(def_nav_w)
        .size_range(min_nav_w..=max_nav_w)
        .frame(Frame::side_top_panel(ui.style()).inner_margin(rail_margin))
        .show(ui, |ui| {
            locked_panel_body(ui, |ui| {
                let nav_scroll_w = ui.available_width();
                egui::ScrollArea::vertical()
                    .id_salt("navigation_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(nav_scroll_w);
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
                                    let right_w = ui.available_width().max(40.0);
                                    ui.vertical(|ui| {
                                        ui.set_max_width(right_w);
                                        let dark = ui.visuals().dark_mode;
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        ui.label(
                                            RichText::new("TriHienKun")
                                                .strong()
                                                .size(16.0)
                                                .color(readable_accent_ink(primary_accent, dark)),
                                        );
                                        ui.label(
                                            RichText::new("Laucher")
                                                .size(11.4)
                                                .strong()
                                                .color(readable_accent_ink(ACCENT_CYAN, dark)),
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
                            let card_resp = section_navigation_card(
                                ui, sec, &label, icon_kind, accent, is_active,
                            );
                            if card_resp.clicked() {
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
                                        let rt = RichText::new(&label).size(12.8).strong();
                                        if ui.available_width() < 116.0 {
                                            ui.add(egui::Label::new(rt).truncate());
                                        } else {
                                            bounded_wrap_label(
                                                ui,
                                                rt,
                                                ui.available_width().max(44.0),
                                            );
                                        }
                                    });
                                },
                            );
                            let resp = resp.interact(Sense::click());
                            resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    ui.is_enabled(),
                                    &label,
                                )
                            });
                            if resp.clicked() {
                                app.click_sfx();
                                match tool_id {
                                    1 => app.open_java_manager(),
                                    2 => app.open_lan_share_modal(),
                                    _ => {
                                        app.show_settings_modal = true;
                                        app.refresh_cache_size();
                                        app.refresh_recovery_artifacts(false);
                                    }
                                }
                            }
                            ui.add_space(2.0);
                        }

                        #[cfg(not(target_os = "android"))]
                        {
                            let mw_title =
                                app.i18n.tr("Cửa sổ Đa nhiệm", "Multi-Window").to_string();
                            let (_, mw_resp) = gaming_card_3d(
                                ui,
                                "nav_multiwindow_card",
                                app.detached.secondary_workspace_open,
                                ACCENT_BLUE,
                                Margin::symmetric(10, 5),
                                |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        paint_icon_3d(
                                            ui,
                                            25.0,
                                            Icon3dKind::MultiWindow,
                                            app.detached.secondary_workspace_open,
                                        );
                                        ui.add_space(4.0);
                                        let rt = RichText::new(&mw_title).size(12.8).strong();
                                        if ui.available_width() < 116.0 {
                                            ui.add(egui::Label::new(rt).truncate());
                                        } else {
                                            bounded_wrap_label(
                                                ui,
                                                rt,
                                                ui.available_width().max(44.0),
                                            );
                                        }
                                    });
                                },
                            );
                            let mw_resp = mw_resp.interact(Sense::click());
                            mw_resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    ui.is_enabled(),
                                    &mw_title,
                                )
                            });
                            if mw_resp.clicked() {
                                app.click_sfx();
                                app.detached.secondary_workspace_open =
                                    !app.detached.secondary_workspace_open;
                            }
                            ui.add_space(2.0);
                        }

                        #[cfg(target_os = "android")]
                        {
                            let rot_title =
                                app.i18n.tr("Xoay màn hình", "Rotate Screen").to_string();
                            let mode_lbl = app
                                .mobile_orientation_label(app.settings.mobile_orientation)
                                .to_string();
                            let (_, rot_resp) = gaming_card_3d(
                                ui,
                                "nav_mobile_rotate_card",
                                false,
                                ACCENT_EMERALD,
                                Margin::symmetric(10, 5),
                                |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        paint_icon_3d(ui, 24.0, Icon3dKind::ScreenRotate, false);
                                        ui.add_space(4.0);
                                        let right_w = ui.available_width().max(44.0);
                                        ui.vertical(|ui| {
                                            ui.set_max_width(right_w);
                                            bounded_wrap_label(
                                                ui,
                                                RichText::new(&rot_title).size(12.4).strong(),
                                                right_w,
                                            );
                                            badge(ui, &mode_lbl, ACCENT_EMERALD);
                                        });
                                    });
                                },
                            );
                            let rot_resp = rot_resp.interact(Sense::click());
                            rot_resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    ui.is_enabled(),
                                    &rot_title,
                                )
                            });
                            if rot_resp.clicked() {
                                app.click_sfx();
                                app.cycle_mobile_orientation();
                            }
                            ui.add_space(2.0);

                            let pad_title = app
                                .i18n
                                .tr("Màn hình Game & Phím", "Game Screen & Controls")
                                .to_string();
                            let (_, pad_resp) = gaming_card_3d(
                                ui,
                                "nav_mobile_game_screen_card",
                                app.mobile_game.screen_open,
                                ACCENT_CYAN,
                                Margin::symmetric(10, 5),
                                |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        paint_icon_3d(
                                            ui,
                                            24.0,
                                            Icon3dKind::MobileGamepad,
                                            app.mobile_game.screen_open,
                                        );
                                        ui.add_space(4.0);
                                        let right_w = ui.available_width().max(44.0);
                                        ui.vertical(|ui| {
                                            ui.set_max_width(right_w);
                                            bounded_wrap_label(
                                                ui,
                                                RichText::new(&pad_title).size(12.4).strong(),
                                                right_w,
                                            );
                                        });
                                    });
                                },
                            );
                            let pad_resp = pad_resp.interact(Sense::click());
                            pad_resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    ui.is_enabled(),
                                    &pad_title,
                                )
                            });
                            if pad_resp.clicked() {
                                app.click_sfx();
                                app.open_mobile_game_screen(false);
                            }
                            ui.add_space(2.0);
                        }

                        ui.add_space(14.0);
                        ui.vertical(|ui| {
                            ui.set_max_width(ui.available_width());
                            ui.label(
                                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
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
                                    let hud_w = ui.available_width();
                                    ui.set_width(hud_w);
                                    if let Some(acct) = sel_acct {
                                        let dark = ui.visuals().dark_mode;
                                        ui.horizontal_wrapped(|ui| {
                                            bounded_wrap_label(
                                                ui,
                                                RichText::new(&acct.username)
                                                    .size(13.0)
                                                    .strong()
                                                    .color(readable_accent_ink(
                                                        ACCENT_EMERALD,
                                                        dark,
                                                    )),
                                                (hud_w - 4.0).max(40.0),
                                            );
                                            badge(ui, acct.account_type.as_str(), ACCENT_BLUE);
                                        });
                                    } else {
                                        bounded_wrap_label(
                                            ui,
                                            RichText::new(
                                                app.i18n.tr(
                                                    "Chưa chọn tài khoản",
                                                    "No account selected",
                                                ),
                                            )
                                            .size(11.8)
                                            .weak(),
                                            hud_w.max(40.0),
                                        );
                                    }
                                },
                            );
                        });
                    });
            });
        });
}

fn portrait_header_tool_btn(
    ui: &mut egui::Ui,
    id_salt: &str,
    icon: Icon3dKind,
    active: bool,
    accent: Color32,
    tooltip: &str,
    compact: bool,
) -> bool {
    let pad = if compact {
        Margin::symmetric(3, 2)
    } else {
        Margin::symmetric(4, 3)
    };
    let icon_sz = if compact { 18.0 } else { 22.0 };
    let (_, resp) = gaming_card_3d(ui, id_salt, active, accent, pad, |ui| {
        paint_icon_3d(ui, icon_sz, icon, active);
    });
    let resp = resp.interact(Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), tooltip));
    resp.on_hover_text(tooltip).clicked()
}

fn render_portrait_top_bar(app: &mut LauncherAppState, ui: &mut egui::Ui, insets: SafeAreaInsets) {
    let primary_accent = accent_palette(app.settings.accent_preset).0;
    let dark = ui.visuals().dark_mode;
    let panel_fill = if dark { BG_PANEL_DARK } else { BG_PANEL_LIGHT };
    let bar_margin = Margin {
        left: margin_i8(10.0 + insets.left),
        right: margin_i8(10.0 + insets.right),
        top: margin_i8(6.0 + insets.top),
        bottom: 6,
    };

    egui::Panel::top("thk_portrait_top_bar")
        .resizable(false)
        .exact_size(48.0 + insets.top)
        .frame(
            Frame::side_top_panel(ui.style())
                .fill(panel_fill)
                .inner_margin(bar_margin),
        )
        .show(ui, |ui| {
            let ultra_compact = ui.available_width() < 290.0;
            ui.horizontal_centered(|ui| {
                paint_icon_3d(
                    ui,
                    if ultra_compact { 22.0 } else { 28.0 },
                    Icon3dKind::BrandVoxelCore,
                    true,
                );
                if !ultra_compact {
                    ui.label(
                        RichText::new("TriHienKun")
                            .size(14.5)
                            .strong()
                            .color(readable_accent_ink(primary_accent, dark)),
                    );
                }
                if ui.available_width() > 210.0 {
                    if let Some(acct) = app.selected_account() {
                        let short_u = modpacks::truncate_chars(&acct.username, 10);
                        badge(ui, &short_u, ACCENT_EMERALD);
                    }
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = if ultra_compact { 3.0 } else { 5.0 };

                    let set_lbl = app.i18n.t("nav.settings").to_string();
                    if portrait_header_tool_btn(
                        ui,
                        "pt_tool_settings",
                        Icon3dKind::SettingsGyro,
                        app.show_settings_modal,
                        ACCENT_VIOLET,
                        &set_lbl,
                        ultra_compact,
                    ) {
                        app.click_sfx();
                        app.show_settings_modal = true;
                        app.refresh_cache_size();
                        app.refresh_recovery_artifacts(false);
                    }

                    #[cfg(target_os = "android")]
                    {
                        let rot_lbl = format!(
                            "{}: {}",
                            app.i18n.tr("Xoay màn hình", "Rotate"),
                            app.mobile_orientation_label(app.settings.mobile_orientation)
                        );
                        if portrait_header_tool_btn(
                            ui,
                            "pt_tool_rotate",
                            Icon3dKind::ScreenRotate,
                            false,
                            ACCENT_EMERALD,
                            &rot_lbl,
                            ultra_compact,
                        ) {
                            app.click_sfx();
                            app.cycle_mobile_orientation();
                        }
                    }

                    #[cfg(not(target_os = "android"))]
                    {
                        let mw_lbl = app.i18n.tr("Cửa sổ Đa nhiệm", "Multi-Window").to_string();
                        if portrait_header_tool_btn(
                            ui,
                            "pt_tool_multiwin",
                            Icon3dKind::MultiWindow,
                            app.detached.secondary_workspace_open,
                            ACCENT_BLUE,
                            &mw_lbl,
                            ultra_compact,
                        ) {
                            app.click_sfx();
                            app.detached.secondary_workspace_open =
                                !app.detached.secondary_workspace_open;
                        }
                    }

                    let lan_lbl = app.i18n.t("nav.lan").to_string();
                    if portrait_header_tool_btn(
                        ui,
                        "pt_tool_lan",
                        Icon3dKind::LanRadar,
                        app.show_lan_modal,
                        ACCENT_CYAN,
                        &lan_lbl,
                        ultra_compact,
                    ) {
                        app.click_sfx();
                        app.open_lan_share_modal();
                    }

                    let java_lbl = app.i18n.t("nav.java").to_string();
                    if portrait_header_tool_btn(
                        ui,
                        "pt_tool_java",
                        Icon3dKind::JavaReactor,
                        app.show_java_modal,
                        ACCENT_ORANGE,
                        &java_lbl,
                        ultra_compact,
                    ) {
                        app.click_sfx();
                        app.open_java_manager();
                    }
                });
            });
        });
}

fn render_portrait_bottom_dock(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    insets: SafeAreaInsets,
) {
    let primary_accent = accent_palette(app.settings.accent_preset).0;
    let dark = ui.visuals().dark_mode;
    let panel_fill = if dark { BG_PANEL_DARK } else { BG_PANEL_LIGHT };
    let corner_x = if insets.bottom > 0.0 {
        (insets.bottom * 0.22).min(5.0)
    } else {
        0.0
    };
    let dock_margin = Margin {
        left: margin_i8(8.0 + insets.left + corner_x),
        right: margin_i8(8.0 + insets.right + corner_x),
        top: 5,
        bottom: margin_i8(5.0 + insets.bottom),
    };

    egui::Panel::bottom("thk_portrait_bottom_dock")
        .resizable(false)
        .exact_size(60.0 + insets.bottom)
        .frame(
            Frame::side_top_panel(ui.style())
                .fill(panel_fill)
                .inner_margin(dock_margin),
        )
        .show(ui, |ui| {
            let items = [
                (
                    ActiveSection::Home,
                    Icon3dKind::HomePortal,
                    primary_accent,
                    app.i18n.tr("Trang chủ", "Home").to_string(),
                    app.i18n.t("nav.home").to_string(),
                ),
                (
                    ActiveSection::Modpacks,
                    Icon3dKind::ModpackCrate,
                    ACCENT_AMBER,
                    app.i18n.tr("Modpack", "Packs").to_string(),
                    app.i18n.t("nav.modpacks").to_string(),
                ),
                (
                    ActiveSection::Servers,
                    Icon3dKind::ServerBladeTower,
                    ACCENT_CYAN,
                    app.i18n.tr("Máy chủ", "Servers").to_string(),
                    app.i18n.t("nav.servers").to_string(),
                ),
                (
                    ActiveSection::Accounts,
                    Icon3dKind::AccountCharacter,
                    ACCENT_VIOLET,
                    app.i18n.tr("Tài khoản", "Accounts").to_string(),
                    app.i18n.t("nav.accounts").to_string(),
                ),
            ];

            let col_count = if cfg!(target_os = "android") { 5 } else { 4 };
            ui.columns(col_count, |cols| {
                for (idx, (sec, icon_kind, accent, short_lbl, a11y_lbl)) in
                    items.into_iter().enumerate()
                {
                    let is_active = app.section == sec;
                    let col_ui = &mut cols[idx];
                    let col_w = col_ui.available_width();
                    let card_pad = if col_w < 46.0 {
                        Margin::symmetric(2, 5)
                    } else if col_w < 70.0 {
                        Margin::symmetric(3, 3)
                    } else {
                        Margin::symmetric(4, 3)
                    };
                    let (_, resp) = gaming_card_3d(
                        col_ui,
                        ("pt_dock_sec", sec as u8),
                        is_active,
                        accent,
                        card_pad,
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.vertical_centered(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                let fg = if is_active && ui.visuals().dark_mode {
                                    Color32::WHITE
                                } else if is_active {
                                    TEXT_INK_LIGHT
                                } else {
                                    ui.visuals().text_color()
                                };
                                if col_w < 46.0 {
                                    paint_icon_3d(ui, 20.0, icon_kind, is_active);
                                } else if col_w < 70.0 {
                                    paint_icon_3d(ui, 19.0, icon_kind, is_active);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&short_lbl).size(9.4).strong().color(fg),
                                        )
                                        .truncate(),
                                    );
                                } else {
                                    paint_icon_3d(ui, 22.0, icon_kind, is_active);
                                    ui.label(
                                        RichText::new(&short_lbl).size(10.8).strong().color(fg),
                                    );
                                }
                            });
                        },
                    );
                    let resp = resp.interact(Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            col_ui.is_enabled(),
                            &a11y_lbl,
                        )
                    });
                    col_ui.ctx().accesskit_node_builder(resp.id, |node| {
                        node.set_aria_current(if is_active {
                            egui::accesskit::AriaCurrent::Page
                        } else {
                            egui::accesskit::AriaCurrent::False
                        });
                    });
                    if resp.clicked() {
                        app.set_section(sec);
                    }
                }

                #[cfg(target_os = "android")]
                {
                    let col_ui = &mut cols[4];
                    let col_w = col_ui.available_width();
                    let card_pad = if col_w < 46.0 {
                        Margin::symmetric(2, 5)
                    } else if col_w < 70.0 {
                        Margin::symmetric(3, 3)
                    } else {
                        Margin::symmetric(4, 3)
                    };
                    let pad_lbl = app.i18n.tr("Chơi Game", "Game").to_string();
                    let pad_a11y = app
                        .i18n
                        .tr("Màn hình Game & Phím", "Game Screen & Controls")
                        .to_string();
                    let (_, resp) = gaming_card_3d(
                        col_ui,
                        "pt_dock_mobile_game",
                        app.mobile_game.screen_open,
                        ACCENT_EMERALD,
                        card_pad,
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.vertical_centered(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                if col_w < 46.0 {
                                    paint_icon_3d(
                                        ui,
                                        20.0,
                                        Icon3dKind::MobileGamepad,
                                        app.mobile_game.screen_open,
                                    );
                                } else if col_w < 70.0 {
                                    paint_icon_3d(
                                        ui,
                                        19.0,
                                        Icon3dKind::MobileGamepad,
                                        app.mobile_game.screen_open,
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&pad_lbl).size(9.4).strong(),
                                        )
                                        .truncate(),
                                    );
                                } else {
                                    paint_icon_3d(
                                        ui,
                                        22.0,
                                        Icon3dKind::MobileGamepad,
                                        app.mobile_game.screen_open,
                                    );
                                    ui.label(RichText::new(&pad_lbl).size(10.8).strong());
                                }
                            });
                        },
                    );
                    let resp = resp.interact(Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            col_ui.is_enabled(),
                            &pad_a11y,
                        )
                    });
                    if resp.clicked() {
                        app.click_sfx();
                        app.open_mobile_game_screen(false);
                    }
                }
            });
        });
}

#[cfg(not(target_os = "android"))]
pub fn render_detached_viewports(
    app: &mut LauncherAppState,
    ctx: &egui::Context,
    render_state: Option<&egui_wgpu::RenderState>,
) {
    let dark_mode = app.settings.theme == thk_core::models::ThemeMode::Dark;
    let base_bg = if dark_mode {
        BG_VOID_DARK
    } else {
        BG_SKY_LIGHT
    };

    if app.detached.secondary_workspace_open {
        let vp_id = egui::ViewportId::from_hash_of("thk_secondary_workspace");
        let title = app
            .i18n
            .tr(
                "TriHienKun Laucher — Cửa sổ Đa nhiệm",
                "TriHienKun Laucher — Secondary Workspace",
            )
            .to_string();
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([980.0, 680.0])
            .with_min_inner_size([400.0, 480.0]);

        ctx.show_viewport_immediate(vp_id, builder, |ui, _class| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                app.detached.secondary_workspace_open = false;
                return;
            }

            let panel_fill = if dark_mode {
                BG_PANEL_DARK
            } else {
                BG_PANEL_LIGHT
            };
            egui::Panel::top("thk_sec_vp_topbar")
                .resizable(false)
                .frame(
                    Frame::side_top_panel(ui.style())
                        .fill(panel_fill)
                        .inner_margin(Margin::symmetric(12, 7)),
                )
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        paint_icon_3d(ui, 26.0, Icon3dKind::MultiWindow, true);
                        let tabs = [
                            (
                                ActiveSection::Home,
                                VectorIcon::Play,
                                app.i18n.t("nav.home").to_string(),
                            ),
                            (
                                ActiveSection::Modpacks,
                                VectorIcon::Cube,
                                app.i18n.t("nav.modpacks").to_string(),
                            ),
                            (
                                ActiveSection::Servers,
                                VectorIcon::Terminal,
                                app.i18n.t("nav.servers").to_string(),
                            ),
                            (
                                ActiveSection::Accounts,
                                VectorIcon::Shield,
                                app.i18n.t("nav.accounts").to_string(),
                            ),
                        ];
                        if let Some(next_sec) = crate::fx3d::sliding_pill_tabs(
                            ui,
                            "sec_vp_nav_tabs",
                            &tabs,
                            app.detached.secondary_section,
                        ) {
                            app.click_sfx();
                            app.detached.secondary_section = next_sec;
                        }
                    });
                });

            let compact = is_compact_portrait(ui.max_rect());
            let margin = if compact {
                Margin::symmetric(10, 10)
            } else {
                Margin::symmetric(16, 12)
            };
            let sec_rs = if matches!(app.section, ActiveSection::Home | ActiveSection::Accounts) {
                None
            } else {
                render_state
            };

            egui::CentralPanel::default()
                .frame(
                    Frame::central_panel(ui.style())
                        .fill(base_bg)
                        .inner_margin(margin),
                )
                .show(ui, |ui| {
                    paint_gaming_3d_backdrop(ui, ui.max_rect(), dark_mode, false);
                    ui.push_id("thk_secondary_vp_scope", |ui| {
                        match app.detached.secondary_section {
                            ActiveSection::Home => home::render_home_view(app, ui, sec_rs),
                            ActiveSection::Modpacks => modpacks::render_modpacks_view(app, ui),
                            ActiveSection::Servers => servers::render_servers_view(app, ui),
                            ActiveSection::Accounts => {
                                accounts::render_accounts_view(app, ui, sec_rs)
                            }
                        }
                    });
                });
        });
    }

    if app.detached.game_logs_window_open {
        let vp_id = egui::ViewportId::from_hash_of("thk_detached_game_logs");
        let title = app
            .i18n
            .tr(
                "TriHienKun Laucher — Nhật ký Phiên Game",
                "TriHienKun Laucher — Game Session Logs",
            )
            .to_string();
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([820.0, 540.0])
            .with_min_inner_size([380.0, 320.0]);

        ctx.show_viewport_immediate(vp_id, builder, |ui, _class| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                app.detached.game_logs_window_open = false;
                return;
            }
            egui::CentralPanel::default()
                .frame(
                    Frame::central_panel(ui.style())
                        .fill(base_bg)
                        .inner_margin(Margin::symmetric(14, 12)),
                )
                .show(ui, |ui| {
                    paint_gaming_3d_backdrop(ui, ui.max_rect(), dark_mode, false);
                    ui.push_id("thk_detached_logs_scope", |ui| {
                        home::render_detached_game_logs_panel(app, ui);
                    });
                });
        });
    }

    if app.detached.server_console_window_open {
        let vp_id = egui::ViewportId::from_hash_of("thk_detached_server_console");
        let title = app
            .i18n
            .tr(
                "TriHienKun Laucher — Bảng điều khiển Máy chủ",
                "TriHienKun Laucher — Server Console",
            )
            .to_string();
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([880.0, 600.0])
            .with_min_inner_size([400.0, 360.0]);

        ctx.show_viewport_immediate(vp_id, builder, |ui, _class| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                app.detached.server_console_window_open = false;
                return;
            }
            egui::CentralPanel::default()
                .frame(
                    Frame::central_panel(ui.style())
                        .fill(base_bg)
                        .inner_margin(Margin::symmetric(14, 12)),
                )
                .show(ui, |ui| {
                    paint_gaming_3d_backdrop(ui, ui.max_rect(), dark_mode, false);
                    ui.push_id("thk_detached_srv_console_scope", |ui| {
                        servers::render_detached_server_console_panel(app, ui);
                    });
                });
        });
    }
}

fn render_bottom_status_bar(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    insets: SafeAreaInsets,
    compact_portrait: bool,
) -> bool {
    let minimized_mp_title = app
        .modpacks
        .install_modal
        .as_ref()
        .filter(|m| m.installing && m.minimized)
        .map(|m| m.detail.summary.title.clone());
    let minimized_disk_title = app
        .disk_pack_modal
        .as_ref()
        .filter(|m| m.installing && m.minimized)
        .map(|m| m.profile_name.clone());
    if app.active_progress.is_none()
        && app.busy_message.is_none()
        && minimized_mp_title.is_none()
        && minimized_disk_title.is_none()
    {
        return false;
    }

    let extra_tasks = app.active_tasks.len().saturating_sub(1);
    let can_cancel = app.can_cancel_primary_task()
        || minimized_mp_title.is_some()
        || minimized_disk_title.is_some();
    let bottom_pad = if compact_portrait { 0.0 } else { insets.bottom };
    let left_pad = if compact_portrait { insets.left } else { 0.0 };
    let bar_margin = Margin {
        left: margin_i8(16.0 + left_pad),
        right: margin_i8(16.0 + insets.right),
        top: 8,
        bottom: margin_i8(8.0 + bottom_pad),
    };

    egui::Panel::bottom("thk_bottom_progress_bar")
        .resizable(false)
        .size_range((48.0 + bottom_pad)..=220.0)
        .frame(Frame::side_top_panel(ui.style()).inner_margin(bar_margin))
        .show(ui, |ui| {
            ui.set_min_height(32.0);
            let full_w = ui.available_width();
            ui.set_max_width(full_w);
            ui.horizontal_wrapped(|ui| {
                if let Some(ref prog) = app.active_progress {
                    let frac = (prog.percent / 100.0).clamp(0.0, 1.0);
                    let speed = format!("{}/s", format_bytes(prog.bytes_per_sec));
                    let bar_w = (full_w * 0.28).clamp(88.0, 240.0);
                    paint_glowing_progress_bar(ui, frac, bar_w, 18.0);
                    ui.add_space(6.0);
                    badge(ui, &prog.phase, ACCENT_CYAN);
                    if extra_tasks > 0 {
                        badge(ui, &format!("+{extra_tasks}"), ACCENT_AMBER);
                    }
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
                } else if let Some(mp_title) = minimized_mp_title
                    .as_ref()
                    .or(minimized_disk_title.as_ref())
                {
                    ui.spinner();
                    ui.label(
                        RichText::new(format!(
                            "{}: {mp_title}",
                            app.i18n
                                .tr("Đang chuẩn bị cài đặt", "Preparing installation")
                        ))
                        .size(12.4)
                        .strong(),
                    );
                }

                if can_cancel || minimized_mp_title.is_some() || minimized_disk_title.is_some() {
                    let has_reopen = minimized_mp_title.is_some() || minimized_disk_title.is_some();
                    let min_right_w =
                        (if has_reopen { 268.0 } else { 108.0 }) * app.settings.font_scale.max(1.0);
                    let render_actions = |app: &mut LauncherAppState, ui: &mut egui::Ui| {
                        if minimized_mp_title.is_some()
                            && secondary_icon_button(
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
                        } else if minimized_disk_title.is_some()
                            && secondary_icon_button(
                                ui,
                                VectorIcon::Download,
                                app.i18n.tr("Mở cửa sổ nhập gói", "Show Importer"),
                            )
                            .clicked()
                        {
                            app.click_sfx();
                            if let Some(m) = app.disk_pack_modal.as_mut() {
                                m.minimized = false;
                            }
                        }
                        if can_cancel
                            && danger_icon_button(
                                ui,
                                VectorIcon::Close,
                                app.i18n.tr("Hủy", "Cancel"),
                            )
                            .clicked()
                        {
                            app.click_sfx();
                            if app.can_cancel_primary_task() {
                                app.cancel_primary_active_task();
                            } else if minimized_mp_title.is_some() {
                                app.cancel_modpack_modal_install();
                            } else if minimized_disk_title.is_some() {
                                app.cancel_disk_pack_import();
                            }
                        }
                    };

                    if ui.available_width() >= min_right_w {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if can_cancel
                                && danger_icon_button(
                                    ui,
                                    VectorIcon::Close,
                                    app.i18n.tr("Hủy", "Cancel"),
                                )
                                .clicked()
                            {
                                app.click_sfx();
                                if app.can_cancel_primary_task() {
                                    app.cancel_primary_active_task();
                                } else if minimized_mp_title.is_some() {
                                    app.cancel_modpack_modal_install();
                                } else if minimized_disk_title.is_some() {
                                    app.cancel_disk_pack_import();
                                }
                            }
                            if minimized_mp_title.is_some()
                                && secondary_icon_button(
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
                            } else if minimized_disk_title.is_some()
                                && secondary_icon_button(
                                    ui,
                                    VectorIcon::Download,
                                    app.i18n.tr("Mở cửa sổ nhập gói", "Show Importer"),
                                )
                                .clicked()
                            {
                                app.click_sfx();
                                if let Some(m) = app.disk_pack_modal.as_mut() {
                                    m.minimized = false;
                                }
                            }
                        });
                    } else {
                        render_actions(app, ui);
                    }
                }
            });
        });
    true
}

fn render_toast_overlay(app: &LauncherAppState, ctx: &egui::Context) {
    if app.toasts.is_empty() {
        return;
    }
    let insets = current_safe_area_insets(ctx);
    let max_toast_w = (safe_content_rect(ctx).width() - 28.0).max(120.0);

    egui::Area::new(egui::Id::unique("thk_toasts_overlay"))
        .anchor(
            Align2::RIGHT_BOTTOM,
            Vec2::new(-(18.0 + insets.right), -(58.0 + insets.bottom)),
        )
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_max_width(max_toast_w);
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
                            ui.set_max_width((max_toast_w - 24.0).max(96.0));
                            ui.horizontal(|ui| {
                                let (ic_rect, _) =
                                    ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                                paint_vector_icon(ui.painter(), ic_rect, v_icon, border_col);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&toast.text).size(12.5).strong(),
                                    )
                                    .wrap(),
                                );
                            });
                        });
                }
            });
        });
}

pub fn primary_button(ui: &mut egui::Ui, label: impl Into<String>) -> egui::Response {
    tactile_button_3d(ui, None, label, ButtonStyle3d::PrimaryEmerald, Vec2::ZERO)
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
    tactile_button_3d(ui, None, label, ButtonStyle3d::SecondaryGlass, Vec2::ZERO)
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
    tactile_button_3d(ui, Some(icon), label, ButtonStyle3d::DangerRose, Vec2::ZERO)
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    let dark = ui.visuals().dark_mode;
    let fg = readable_accent_ink(color, dark);
    let label = RichText::new(text).size(11.0).color(fg).strong();
    let fill_alpha = if dark { 42 } else { 28 };
    let border_alpha = if dark { 195 } else { 175 };
    let frame = Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            fill_alpha,
        ))
        .stroke(Stroke::new(
            1.1,
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), border_alpha),
        ))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(7, 2));
    let frame_margin = frame.total_margin().sum();
    let mut job = egui::text::LayoutJob::default();
    label.clone().append_to(
        &mut job,
        ui.style(),
        egui::FontSelection::Default,
        Align::LEFT,
    );
    let unwrapped_size = ui.fonts_mut(|fonts| fonts.layout_job(job.clone())).size();
    let max_outer_w = ui.max_rect().width();
    let wrap_needed = unwrapped_size.x + frame_margin.x > max_outer_w + 2.0;
    let inner_cap_w = (max_outer_w - frame_margin.x - 1.0).max(32.0);
    let (galley_opt, text_size) = if wrap_needed {
        job.wrap.max_width = inner_cap_w;
        let mut galley = ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
        if galley.size().x > inner_cap_w + 1.0 {
            job.wrap.break_anywhere = true;
            galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        }
        let sz = galley.size();
        (Some(galley), sz)
    } else {
        (None, unwrapped_size)
    };
    ui.allocate_ui_with_layout(
        text_size + frame_margin,
        Layout::left_to_right(Align::Center),
        |ui| {
            frame.show(ui, |ui| {
                if let Some(galley) = galley_opt {
                    ui.set_max_width(inner_cap_w);
                    ui.add(egui::Label::new(galley));
                } else {
                    ui.add(egui::Label::new(label).extend());
                }
            });
        },
    );
}

pub(crate) fn wrapped_checkbox<'a>(
    ui: &mut egui::Ui,
    checked: &mut bool,
    text: impl Into<egui::Atom<'a>>,
) -> egui::Response {
    use egui::AtomExt as _;
    ui.checkbox(checked, text.atom_shrink(true))
}

pub fn java_runtime_mode_selector(
    ui: &mut egui::Ui,
    i18n: &thk_core::I18n,
    mode: &mut thk_core::models::JavaRuntimeMode,
    target_mc_version: Option<&str>,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(i18n.tr("Chế độ Java:", "Java Mode:")).strong());
        changed |= ui
            .selectable_value(
                mode,
                thk_core::models::JavaRuntimeMode::OptimalGraalVm,
                i18n.tr("Tối ưu (GraalVM)", "Optimal (GraalVM)"),
            )
            .changed();
        changed |= ui
            .selectable_value(
                mode,
                thk_core::models::JavaRuntimeMode::MojangOfficial,
                i18n.tr("Tiêu chuẩn (Mojang)", "Standard (Mojang)"),
            )
            .changed();
        if let Some(mc_ver) = target_mc_version.map(str::trim).filter(|s| !s.is_empty()) {
            let prefer_optimal = *mode == thk_core::models::JavaRuntimeMode::OptimalGraalVm;
            let rt_lbl = thk_java::recommended_runtime_label_for_mc(mc_ver, prefer_optimal);
            let color = if rt_lbl.starts_with("GraalVM") {
                ACCENT_ORANGE
            } else {
                ACCENT_EMERALD
            };
            badge(ui, &rt_lbl, color);
        }
    });
    changed
}

pub fn font_scale_selector(
    ui: &mut egui::Ui,
    i18n: &thk_core::I18n,
    font_scale: &mut f32,
    drag_dirty: &mut bool,
) -> bool {
    let min_pct = thk_core::models::MIN_FONT_SCALE * 100.0;
    let max_pct = thk_core::models::MAX_FONT_SCALE * 100.0;
    let clamped = thk_core::models::sanitize_font_scale(*font_scale);
    let mut pct = (clamped * 1000.0).round() / 10.0;
    let mut commit = false;

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(i18n.tr("Cỡ chữ:", "Font Scale:")).strong());
        let adaptive_slider_w = (ui.max_rect().width() - 128.0).clamp(52.0, 130.0);
        if ui.spacing().slider_width > adaptive_slider_w {
            ui.spacing_mut().slider_width = adaptive_slider_w;
        }
        let slider_resp = ui.add(egui::Slider::new(&mut pct, min_pct..=max_pct).show_value(false));
        let drag_val_resp = ui.add(
            egui::DragValue::new(&mut pct)
                .range(min_pct..=max_pct)
                .speed(0.5)
                .max_decimals(1)
                .suffix("%"),
        );

        let val_changed = slider_resp.changed() || drag_val_resp.changed();
        let is_dragging = slider_resp.dragged() || drag_val_resp.dragged();

        if val_changed {
            *font_scale = thk_core::models::sanitize_font_scale(pct / 100.0);
            if is_dragging {
                *drag_dirty = true;
            } else {
                *drag_dirty = false;
                commit = true;
            }
        }
        if slider_resp.drag_stopped() || drag_val_resp.drag_stopped() {
            *drag_dirty = false;
            commit = true;
        }
        for preset_pct in [75.0f32, 100.0, 125.0, 150.0] {
            let selected = (pct - preset_pct).abs() < 0.2;
            if ui
                .selectable_label(selected, format!("{preset_pct:.0}%"))
                .clicked()
            {
                *font_scale = thk_core::models::sanitize_font_scale(preset_pct / 100.0);
                *drag_dirty = false;
                commit = true;
            }
        }
    });
    commit
}

pub fn centered_window_rect(ctx: &egui::Context, preferred_size: Vec2) -> Rect {
    let screen = safe_content_rect(ctx);
    let pad = if screen.width() < 340.0 || screen.height() < 260.0 {
        vec2(8.0, 10.0)
    } else if screen.width() < 520.0 {
        vec2(16.0, 24.0)
    } else {
        vec2(32.0, 32.0)
    };
    let min_floor = vec2(280.0, 200.0).min((screen.size() - pad).max(vec2(110.0, 90.0)));
    let max_size = (screen.size() - pad).max(min_floor).min(screen.size());
    let size = preferred_size.min(max_size);
    let edge = if screen.width() < 340.0 || screen.height() < 260.0 {
        4.0
    } else if screen.width() < 520.0 {
        8.0
    } else {
        12.0
    };
    let max_x = (screen.right() - size.x - edge).max(screen.left());
    let max_y = (screen.bottom() - size.y - edge).max(screen.top());
    let min_x = (screen.center().x - size.x * 0.5).clamp(
        screen.left() + edge.min((max_x - screen.left()).max(0.0)),
        max_x,
    );
    let min_y = (screen.center().y - size.y * 0.5).clamp(
        screen.top() + edge.min((max_y - screen.top()).max(0.0)),
        max_y,
    );
    Rect::from_min_size(pos2(min_x, min_y), size)
}

pub fn format_project_version_option(
    v: &thk_mods::ProjectVersion,
    include_server_pack_tag: bool,
) -> String {
    let raw_ver = v.canonical_version_label();
    let ver_head = if raw_ver.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("v{}", modpacks::truncate_chars(&raw_ver, 26))
    } else {
        modpacks::truncate_chars(&raw_ver, 28)
    };
    let channel = match v.release_channel.to_ascii_lowercase().as_str() {
        "release" => "Release",
        "beta" => "Beta",
        "alpha" => "Alpha",
        _ => v.release_channel.as_str(),
    };
    let mc = v.game_versions.first().map(String::as_str).unwrap_or("—");
    let ldr = v.loaders.first().map(String::as_str).unwrap_or("");
    let srv_suffix = if include_server_pack_tag && v.server_pack_file_id.is_some() {
        " • Server Pack"
    } else {
        ""
    };
    if ldr.is_empty() {
        format!("{ver_head} [{channel}] • MC {mc}{srv_suffix}")
    } else {
        format!("{ver_head} [{channel}] • {ldr} • MC {mc}{srv_suffix}")
    }
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

pub(crate) fn network_interface_selector_ui(
    ui: &mut egui::Ui,
    id_salt: &str,
    i18n: &thk_core::I18n,
    snapshot: &ConnectedNetworkSnapshot,
    selected_ip: &mut Option<std::net::Ipv4Addr>,
) -> bool {
    let mut changed = false;
    let nic_lbl = ui.label(RichText::new(i18n.tr("Mạng kết nối:", "Network:")).strong());
    let all_label = i18n
        .tr("Tất cả mạng đang kết nối", "All connected networks")
        .to_string();
    let selected_text = match *selected_ip {
        None => all_label.clone(),
        Some(ip) => snapshot
            .interfaces
            .iter()
            .find(|entry| entry.ipv4 == ip)
            .map(|entry| format!("{} ({})", entry.name, entry.ipv4))
            .unwrap_or_else(|| ip.to_string()),
    };

    let combo_w = 240.0_f32.min((ui.max_rect().width() - 16.0).max(64.0));
    let combo_resp = egui::ComboBox::from_id_salt(id_salt)
        .width(combo_w)
        .truncate()
        .selected_text(selected_text)
        .show_ui(ui, |ui| {
            if ui.selectable_value(selected_ip, None, &all_label).changed() {
                changed = true;
            }
            for entry in &snapshot.interfaces {
                let item_label = if entry.is_primary {
                    format!(
                        "{} ({}) • {}",
                        entry.name,
                        entry.ipv4,
                        i18n.tr("Chính", "Primary")
                    )
                } else {
                    format!("{} ({})", entry.name, entry.ipv4)
                };
                if ui
                    .selectable_value(selected_ip, Some(entry.ipv4), item_label)
                    .changed()
                {
                    changed = true;
                }
            }
        });
    combo_resp.response.labelled_by(nic_lbl.id);
    changed
}

pub(crate) fn render_direct_host_endpoints_ui(
    ui: &mut egui::Ui,
    i18n: &thk_core::I18n,
    info: &DirectHostActiveInfo,
    public_ping_ms: Option<u32>,
    ping_color: Option<Color32>,
) -> Option<String> {
    let mut copied: Option<String> = None;

    ui.horizontal_wrapped(|ui| {
        badge(
            ui,
            &format!(
                "{}: {}",
                i18n.tr("Người chơi kết nối qua cổng mở", "Connected via port"),
                info.active_clients
            ),
            ACCENT_CYAN,
        );
        if info.announcing_lan {
            badge(
                ui,
                i18n.tr("Đang phát sóng LAN nội bộ", "LAN discovery active"),
                ACCENT_BLUE,
            );
        }
        if let Some(ms) = public_ping_ms {
            badge(
                ui,
                &format!("{}: {ms} ms", i18n.tr("Ping cổng mở", "Hosted Port Ping")),
                ping_color.unwrap_or(ACCENT_EMERALD),
            );
        }
    });

    for (idx, (nic_name, endpoint)) in info.lan_endpoints.iter().enumerate() {
        ui.push_id(("direct_lan_ep_row", idx), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} ({nic_name}):",
                        i18n.tr("Cùng mạng LAN / VPN", "LAN / VPN")
                    ))
                    .strong(),
                );
                badge(ui, endpoint, ACCENT_BLUE);
                let btn_lbl = format!("{} {nic_name}", i18n.tr("Sao chép", "Copy"));
                if secondary_icon_button(ui, VectorIcon::Copy, &btn_lbl).clicked() {
                    copied = Some(endpoint.clone());
                }
            });
        });
    }

    if let Some(ref v6_endpoint) = info.ipv6_endpoint {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(i18n.tr("Chơi Online trực tiếp (IPv6):", "Direct Online (IPv6):"))
                    .strong(),
            );
            badge(ui, v6_endpoint, ACCENT_EMERALD);
            if primary_icon_button(
                ui,
                VectorIcon::Copy,
                i18n.tr("Sao chép Online IPv6", "Copy Online IPv6"),
            )
            .clicked()
            {
                copied = Some(v6_endpoint.clone());
            }
        });
    }

    match &info.upnp_status {
        UpnpPortStatus::Mapped {
            external_ip,
            external_port,
            gateway_label,
        } => {
            let v4_endpoint = format!("{external_ip}:{external_port}");
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(i18n.tr(
                        "Chơi Online IPv4 (Đã mở trên Modem):",
                        "Online IPv4 (Router port open):",
                    ))
                    .strong(),
                );
                badge(ui, &v4_endpoint, ACCENT_EMERALD);
                badge(ui, gateway_label, ACCENT_CYAN);
                if primary_icon_button(
                    ui,
                    VectorIcon::Copy,
                    i18n.tr("Sao chép Online IPv4", "Copy Online IPv4"),
                )
                .clicked()
                {
                    copied = Some(v4_endpoint);
                }
            });
        }
        UpnpPortStatus::Mapping => {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(i18n.tr(
                    "Đang mở cổng tự động trên Modem...",
                    "Requesting automatic port mapping on router...",
                ));
            });
        }
        UpnpPortStatus::CarrierGradeNat { wan_ip, .. } => {
            let note = if i18n.language() == Language::Vi {
                format!("Mạng nhà mạng CGNAT ({wan_ip}) — kết nối khác mạng qua IPv6 hoặc VPN.")
            } else {
                format!("ISP CGNAT ({wan_ip}) — connect across networks via IPv6 or VPN.")
            };
            ui.label(RichText::new(note).size(11.5).color(ACCENT_AMBER));
        }
        UpnpPortStatus::Unavailable(_) => {
            if let Some(ref pub_ep) = info.public_ipv4_endpoint {
                ui.horizontal_wrapped(|ui| {
                    ui.label(i18n.tr("IPv4 Internet:", "Public IPv4:"));
                    badge(ui, pub_ep, ACCENT_AMBER);
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Copy,
                        i18n.tr("Sao chép Online IPv4", "Copy Online IPv4"),
                    )
                    .clicked()
                    {
                        copied = Some(pub_ep.clone());
                    }
                });
            }
            ui.label(
                RichText::new(i18n.tr("UPnP đang tắt trên Modem.", "Router UPnP is disabled."))
                    .size(11.5)
                    .weak(),
            );
        }
        UpnpPortStatus::Disabled => {
            if let Some(ref pub_ep) = info.public_ipv4_endpoint {
                ui.horizontal_wrapped(|ui| {
                    ui.label(i18n.tr("IPv4 Internet:", "Public IPv4:"));
                    badge(ui, pub_ep, ACCENT_BLUE);
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Copy,
                        i18n.tr("Sao chép Online IPv4", "Copy Online IPv4"),
                    )
                    .clicked()
                    {
                        copied = Some(pub_ep.clone());
                    }
                });
            }
        }
    }

    copied
}
