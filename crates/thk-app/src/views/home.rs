use crate::fx3d::{
    gaming_card_3d, paint_3d_isometric_bars, paint_icon_3d, sliding_pill_tabs, tactile_button_3d,
    ButtonStyle3d, Icon3dKind, VectorIcon,
};
use crate::state::{HomeTab, LauncherAppState, ToastKind};
use crate::theme::{
    readable_accent_ink, ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_ORANGE,
    ACCENT_RED, ACCENT_VIOLET, BG_SLOT_LIGHT,
};
use crate::views::{
    accent_icon_button, badge, bounded_wrap_label, danger_icon_button, format_bytes,
    format_duration_hms, java_runtime_mode_selector, primary_icon_button, render_motd_spans,
    secondary_button, secondary_icon_button,
};
use egui::{
    pos2, vec2, Color32, CornerRadius, Frame, Margin, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, Vec2,
};
use std::collections::HashSet;
use thk_core::models::{ContentKind, JavaRuntimeMode, Language, LoaderType, ProviderSource};
use thk_launcher::InstanceKey;
use thk_mods::ContentUpdateCandidate;

#[cfg(all(test, windows))]
#[path = "home_tests.rs"]
mod tests;

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
        let slot_bg = if ui.visuals().dark_mode {
            Color32::from_rgb(20, 29, 44)
        } else {
            BG_SLOT_LIGHT
        };
        ui.painter().rect_filled(rect, radius, slot_bg);

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
    let compact = crate::views::is_compact_portrait(ui.max_rect());
    if !compact {
        let avail_w = ui.available_width();
        let min_side_w = 176.0_f32.min((avail_w * 0.38).max(140.0));
        let max_side_w = (avail_w - 260.0).min(avail_w * 0.55).max(min_side_w);
        let def_side_w = if avail_w < 640.0 {
            214.0_f32
        } else {
            266.0_f32
        }
        .clamp(min_side_w, max_side_w);
        let sidebar_id = if ui.ctx().viewport_id() == egui::ViewportId::ROOT {
            egui::Id::unique("thk_home_profiles_sidebar")
        } else {
            ui.scope_id().with("thk_home_profiles_sidebar")
        };
        egui::Panel::left(sidebar_id)
            .resizable(!cfg!(target_os = "android"))
            .default_size(def_side_w)
            .size_range(min_side_w..=max_side_w)
            .frame(Frame::NONE.inner_margin(Margin {
                left: 0,
                right: 12,
                top: 0,
                bottom: 0,
            }))
            .show(ui, |ui| {
                crate::views::locked_panel_body(ui, |ui| {
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
                    let profiles_scroll_w = ui.available_width();

                    ScrollArea::vertical()
                        .id_salt("home_profiles_scroll")
                        .show(ui, |ui| {
                            ui.set_max_width(profiles_scroll_w);
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
                                                ui.set_width(ui.available_width());
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(&prof.name)
                                                            .strong()
                                                            .size(13.5),
                                                    )
                                                    .wrap(),
                                                );
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
            });
    }

    let viewport_h = ui.available_height();
    let scroll_w = ui.available_width();
    ScrollArea::vertical()
        .id_salt("home_workspace_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(scroll_w);
            if compact {
                render_compact_profile_bar(app, ui);
                ui.add_space(6.0);
            }

            let Some(profile) = app.selected_profile().cloned() else {
                ui.vertical_centered(|ui| {
                    ui.add_space(56.0);
                    paint_icon_3d(ui, 76.0, Icon3dKind::BrandVoxelCore, true);
                    ui.add_space(10.0);
                    ui.heading(RichText::new(app.i18n.t("app.name")).size(22.0).strong());
                    ui.label(RichText::new(app.i18n.t("home.no_profiles")).weak());
                    ui.add_space(14.0);
                    if primary_icon_button(ui, VectorIcon::Plus, app.i18n.t("home.new_profile"))
                        .clicked()
                    {
                        app.show_new_profile_modal = true;
                    }
                });
                return;
            };

            render_hero_launch_banner(app, ui, &profile, render_state);
            ui.add_space(if compact { 8.0 } else { 10.0 });
            render_profile_tabs_bar(app, ui);
            ui.add_space(8.0);
            render_active_home_tab(app, ui, &profile, false, viewport_h);
        });
}

fn render_compact_profile_bar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let profiles_snapshot = app.home.profiles.clone();
    let selected_id = app.home.selected_profile_id.clone();
    let current_txt = profiles_snapshot
        .iter()
        .find(|p| selected_id.as_deref() == Some(p.id.as_str()))
        .map(|p| {
            format!(
                "{} ({} • {})",
                p.name,
                p.game_version,
                p.loader.display_name()
            )
        })
        .unwrap_or_else(|| app.i18n.t("home.no_profiles").to_string());

    gaming_card_3d(
        ui,
        "home_compact_profile_switcher",
        false,
        ACCENT_EMERALD,
        Margin::symmetric(10, 7),
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let cap_w = (ui.max_rect().width() - 12.0).max(84.0);
                let combo_w = (ui.available_width() - 120.0)
                    .clamp(140.0_f32.min(cap_w), 320.0)
                    .min(cap_w);
                let mut picked_id: Option<String> = None;
                let a11y_prof_lbl = app.i18n.t("home.profiles").to_string();
                let combo_resp = egui::ComboBox::from_id_salt("home_compact_profile_combo")
                    .width(combo_w)
                    .truncate()
                    .selected_text(&current_txt)
                    .show_ui(ui, |ui| {
                        for prof in &profiles_snapshot {
                            let is_sel = selected_id.as_deref() == Some(prof.id.as_str());
                            let lbl = format!(
                                "{} ({} • {})",
                                prof.name,
                                prof.game_version,
                                prof.loader.display_name()
                            );
                            if ui.selectable_label(is_sel, lbl).clicked() {
                                picked_id = Some(prof.id.clone());
                            }
                        }
                    });
                ui.ctx()
                    .accesskit_node_builder(combo_resp.response.id, |node| {
                        node.set_label(a11y_prof_lbl);
                    });
                if let Some(pid) = picked_id {
                    app.click_sfx();
                    app.select_profile(pid);
                }

                if primary_icon_button(ui, VectorIcon::Plus, app.i18n.tr("Tạo", "New")).clicked()
                {
                    app.click_sfx();
                    let mc = app.home.new_draft.game_version.clone();
                    let ldr = app.home.new_draft.loader;
                    app.ensure_loader_versions(&mc, ldr);
                    app.show_new_profile_modal = true;
                }

                if secondary_icon_button(ui, VectorIcon::Download, app.i18n.tr("Nhập", "Import"))
                    .clicked()
                {
                    app.click_sfx();
                    app.import_modpack_from_disk();
                }
            });
        },
    );
}

fn maybe_home_scroll(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    standalone_scroll: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let w = ui.available_width();
    if standalone_scroll {
        ScrollArea::vertical().id_salt(id_salt).show(ui, |ui| {
            ui.set_max_width(w);
            add_contents(ui);
        });
    } else {
        ui.set_max_width(w);
        add_contents(ui);
    }
}

fn render_active_home_tab(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
    standalone_scroll: bool,
    viewport_h: f32,
) {
    match app.home.active_tab {
        HomeTab::General => render_tab_general_with_scroll(app, ui, profile, standalone_scroll),
        HomeTab::Mods => render_tab_content(app, ui, profile, ContentKind::Mod, standalone_scroll),
        HomeTab::Shaders => {
            render_tab_content(app, ui, profile, ContentKind::Shader, standalone_scroll)
        }
        HomeTab::ResourcePacks => render_tab_content(
            app,
            ui,
            profile,
            ContentKind::ResourcePack,
            standalone_scroll,
        ),
        HomeTab::Worlds => render_tab_worlds(app, ui, profile, standalone_scroll),
        HomeTab::Files => render_tab_files(app, ui, profile, standalone_scroll),
        HomeTab::Bookmarks => render_tab_bookmarks(app, ui, standalone_scroll),
        HomeTab::Logs => render_tab_logs(
            app,
            ui,
            profile,
            if standalone_scroll {
                None
            } else {
                Some(viewport_h)
            },
        ),
        HomeTab::Analytics => render_tab_analytics(app, ui, profile, standalone_scroll),
    }
}

fn render_hero_launch_banner(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
    render_state: Option<&egui_wgpu::RenderState>,
) {
    let low_ram = app.settings.low_ram_mode;
    if !low_ram {
        app.sync_3d_viewport_textures(render_state);
    }
    let hero_icon_tex = profile
        .icon_url
        .as_deref()
        .and_then(|u| app.request_modpack_texture(u, 96));

    let active_key = app
        .selected_account()
        .map(|a| InstanceKey::new(profile.id.clone(), a.id.clone()));
    let is_running = active_key
        .as_ref()
        .is_some_and(|k| app.instance_supervisor.is_running(k));
    let live_instances: Vec<_> = app
        .instance_supervisor
        .running_snapshots()
        .into_iter()
        .filter(|s| s.key.profile_id == profile.id)
        .map(|s| {
            let username = app
                .accounts
                .accounts
                .iter()
                .find(|a| a.id == s.key.account_id)
                .map(|a| a.username.as_str())
                .unwrap_or("Player");
            let pid = s.pid.map(|p| format!("PID {p}")).unwrap_or_default();
            format!(
                "LIVE: {username} ({pid} • {})",
                format_duration_hms(s.uptime_secs)
            )
        })
        .collect();
    let actions = profile_hero_ui(
        ui,
        &ProfileHeroUi {
            profile,
            i18n: &app.i18n,
            icon: hero_icon_tex.as_ref(),
            accounts: &app.accounts.accounts,
            selected_account_id: app.accounts.selected_account_id.as_deref(),
            is_running,
            live_instances: &live_instances,
        },
        |ui| {
            if !low_ram {
                if let (Some(rs), Some(vp)) = (render_state, app.accounts.skin_viewport.as_mut()) {
                    vp.show(ui, rs, vec2(142.0, 154.0));
                    ui.add_space(8.0);
                }
            }
        },
    );
    if let Some(id) = actions.account_id {
        app.select_account(id);
    }
    if actions.open_folder {
        let _ = std::fs::create_dir_all(&profile.instance_path);
        app.open_local_folder_or_toast(&profile.instance_path);
    }
    if actions.launch_or_stop {
        if is_running {
            if let Some(key) = active_key {
                app.instance_supervisor.stop_instance(&key);
            }
        } else {
            app.launch_selected_profile(None);
        }
    }
}

pub(super) struct ProfileHeroUi<'a> {
    pub profile: &'a thk_core::models::GameProfile,
    pub i18n: &'a thk_core::I18n,
    pub icon: Option<&'a egui::TextureHandle>,
    pub accounts: &'a [thk_core::models::Account],
    pub selected_account_id: Option<&'a str>,
    pub is_running: bool,
    pub live_instances: &'a [String],
}

#[derive(Default)]
pub(super) struct ProfileHeroActions {
    pub account_id: Option<String>,
    pub open_folder: bool,
    pub launch_or_stop: bool,
}

pub(super) fn profile_hero_ui(
    ui: &mut egui::Ui,
    hero: &ProfileHeroUi<'_>,
    preview: impl FnOnce(&mut egui::Ui),
) -> ProfileHeroActions {
    let profile = hero.profile;
    let hero_accent = loader_accent_color(profile.loader);
    let mut actions = ProfileHeroActions::default();

    gaming_card_3d(
        ui,
        ("home_hero_arena", &profile.id),
        true,
        hero_accent,
        Margin::symmetric(16, 12),
        |ui| {
            ui.set_width(ui.available_width());
            let stack_vertical = ui.available_width() < 380.0;
            let render_hero_details = |ui: &mut egui::Ui,
                                       actions: &mut ProfileHeroActions,
                                       stack: bool| {
                ui.horizontal_wrapped(|ui| {
                    paint_square_thumbnail(
                        ui,
                        hero.icon,
                        38.0,
                        hero_accent,
                        Icon3dKind::LaunchPrism,
                        true,
                    );
                    ui.heading(
                        RichText::new(&profile.name)
                            .size(if stack { 19.0 } else { 22.0 })
                            .strong(),
                    );
                });

                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    profile_badge_ui(ui, &format!("MC {}", profile.game_version), ACCENT_BLUE);
                    let ldr_txt = match profile.loader_version {
                        Some(ref lv) => format!("{} {lv}", profile.loader.display_name()),
                        None => profile.loader.display_name().to_string(),
                    };
                    profile_badge_ui(ui, &ldr_txt, hero_accent);
                    profile_badge_ui(ui, &format!("{} MB RAM", profile.ram_mb), ACCENT_AMBER);
                    if profile.fullscreen {
                        profile_badge_ui(
                            ui,
                            hero.i18n.tr("Toàn màn hình", "Fullscreen"),
                            ACCENT_VIOLET,
                        );
                    } else if let (Some(w), Some(h)) = (profile.window_width, profile.window_height)
                    {
                        profile_badge_ui(ui, &format!("{w}×{h}"), ACCENT_VIOLET);
                    }
                    if profile.total_play_seconds > 0 {
                        profile_badge_ui(
                            ui,
                            &format_duration_hms(profile.total_play_seconds),
                            ACCENT_CYAN,
                        );
                    }
                });

                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    let current_name = hero
                        .accounts
                        .iter()
                        .find(|a| Some(a.id.as_str()) == hero.selected_account_id)
                        .map(|a| format!("{} ({})", a.username, a.account_type.as_str()))
                        .unwrap_or_else(|| {
                            hero.i18n
                                .tr("Chọn tài khoản chơi", "Select Account")
                                .to_string()
                        });

                    let cap_w = (ui.max_rect().width() - 10.0).max(80.0);
                    let combo_w = if stack {
                        (ui.available_width() - 10.0)
                            .clamp(140.0_f32.min(cap_w), 220.0)
                            .min(cap_w)
                    } else {
                        190.0_f32.min(cap_w)
                    };
                    egui::ComboBox::from_id_salt("hero_account_combo")
                        .width(combo_w)
                        .truncate()
                        .selected_text(&current_name)
                        .show_ui(ui, |ui| {
                            for a in hero.accounts {
                                let label = format!("{} ({})", a.username, a.account_type.as_str());
                                if ui
                                    .selectable_label(
                                        hero.selected_account_id == Some(a.id.as_str()),
                                        label,
                                    )
                                    .clicked()
                                {
                                    actions.account_id = Some(a.id.clone());
                                }
                            }
                        })
                        .response
                        .widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::ComboBox,
                                ui.is_enabled(),
                                &current_name,
                            )
                        });

                    if secondary_icon_button(
                        ui,
                        VectorIcon::Folder,
                        hero.i18n.t("home.open_folder"),
                    )
                    .clicked()
                    {
                        actions.open_folder = true;
                    }
                });

                ui.add_space(10.0);
                let avail_launch_w = ui.available_width().max(80.0);
                let launch_w = if stack {
                    ui.available_width()
                        .clamp(160.0_f32.min(avail_launch_w), 280.0)
                        .min(avail_launch_w)
                } else {
                    194.0_f32.min(avail_launch_w)
                };
                actions.launch_or_stop = tactile_button_3d(
                    ui,
                    Some(if hero.is_running {
                        VectorIcon::Stop
                    } else {
                        VectorIcon::Play
                    }),
                    hero.i18n.t(if hero.is_running {
                        "home.stop"
                    } else {
                        "home.play"
                    }),
                    if hero.is_running {
                        ButtonStyle3d::HeroStop
                    } else {
                        ButtonStyle3d::HeroLaunch
                    },
                    vec2(launch_w, 50.0),
                )
                .clicked();

                if !hero.live_instances.is_empty() {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        for label in hero.live_instances {
                            badge(ui, label, ACCENT_EMERALD);
                        }
                    });
                }
            };

            if stack_vertical {
                ui.vertical_centered(|ui| {
                    preview(ui);
                });
                ui.add_space(4.0);
                render_hero_details(ui, &mut actions, true);
            } else {
                ui.horizontal(|ui| {
                    preview(ui);
                    let mid_w = ui.available_width();
                    ui.allocate_ui_with_layout(
                        vec2(mid_w, 150.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            render_hero_details(ui, &mut actions, false);
                        },
                    );
                });
            }
        },
    );
    actions
}

fn profile_badge_ui(ui: &mut egui::Ui, text: &str, color: Color32) {
    let max_line_w = (ui.max_rect().width() - 16.0).max(40.0);
    let galley = egui::WidgetText::from(RichText::new(text).size(11.0).strong()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    let text_size = if galley.size().x > max_line_w {
        egui::WidgetText::from(RichText::new(text).size(11.0).strong())
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Wrap),
                max_line_w,
                egui::TextStyle::Body,
            )
            .size()
    } else {
        galley.size()
    };
    // Allocate the complete frame before drawing so wrapped rows can move it intact.
    ui.allocate_ui_with_layout(
        text_size + vec2(16.0, 6.0),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| badge(ui, text, color),
    );
}

fn render_profile_tabs_bar(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    if let Some(next_tab) = profile_tabs_ui(ui, &app.i18n, app.home.active_tab) {
        app.click_sfx();
        app.home.active_tab = next_tab;
        app.home.show_online_browser = false;
        app.refresh_selected_profile_tabs();
        if next_tab == HomeTab::Bookmarks {
            app.ping_all_bookmarks();
        }
    }
}

pub(super) fn profile_tabs_ui(
    ui: &mut egui::Ui,
    i18n: &thk_core::I18n,
    current: HomeTab,
) -> Option<HomeTab> {
    let tabs = [
        (
            HomeTab::General,
            VectorIcon::Sliders,
            i18n.t("home.tab.general").to_string(),
        ),
        (
            HomeTab::Mods,
            VectorIcon::Cube,
            i18n.t("home.tab.mods").to_string(),
        ),
        (
            HomeTab::Shaders,
            VectorIcon::Sparkles,
            i18n.t("home.tab.shaders").to_string(),
        ),
        (
            HomeTab::ResourcePacks,
            VectorIcon::Image,
            i18n.t("home.tab.resourcepacks").to_string(),
        ),
        (
            HomeTab::Worlds,
            VectorIcon::Globe,
            i18n.t("home.tab.worlds").to_string(),
        ),
        (
            HomeTab::Files,
            VectorIcon::Folder,
            i18n.t("home.tab.files").to_string(),
        ),
        (
            HomeTab::Bookmarks,
            VectorIcon::Play,
            i18n.t("home.tab.bookmarks").to_string(),
        ),
        (
            HomeTab::Logs,
            VectorIcon::Terminal,
            i18n.t("home.tab.logs").to_string(),
        ),
        (
            HomeTab::Analytics,
            VectorIcon::Chart,
            i18n.t("home.tab.analytics").to_string(),
        ),
    ];

    sliding_pill_tabs(ui, "home_profile_tabs_pill", &tabs, current)
}

#[cfg(test)]
pub(crate) fn render_tab_general(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
) {
    render_tab_general_with_scroll(app, ui, profile, true);
}

fn render_tab_general_with_scroll(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
    standalone_scroll: bool,
) {
    if app.home.edit_draft.loaded_for_id != profile.id {
        app.sync_profile_edit_draft();
    }

    maybe_home_scroll(ui, "home_tab_general", standalone_scroll, |ui| {
        gaming_card_3d(
            ui,
            "home_general_cfg_card",
            false,
            ACCENT_EMERALD,
            Margin::same(16),
            |ui| {
                ui.set_width(ui.available_width());
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    let name_label = ui.label(RichText::new(app.i18n.t("profile.name")).strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut app.home.edit_draft.name)
                            .desired_width(ui.available_width()),
                    )
                    .labelled_by(name_label.id);

                    let ram_label = ui.label(RichText::new(app.i18n.t("profile.ram")).strong());
                    ui.add(
                        egui::Slider::new(&mut app.home.edit_draft.ram_mb, 1024..=32768)
                            .step_by(256.0)
                            .suffix(" MB"),
                    )
                    .labelled_by(ram_label.id);

                    let prefer_optimal =
                        app.settings.java_runtime_mode == JavaRuntimeMode::OptimalGraalVm;
                    let auto_rt_lbl = thk_java::recommended_runtime_label_for_mc(
                        &profile.game_version,
                        prefer_optimal,
                    );
                    let java_hint = format!("{} ({auto_rt_lbl})", app.i18n.tr("Tự động", "Auto"));
                    let java_label =
                        ui.label(RichText::new(app.i18n.t("profile.java_path")).strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut app.home.edit_draft.java_path_str)
                            .hint_text(java_hint)
                            .desired_width(ui.available_width()),
                    )
                    .labelled_by(java_label.id);
                    ui.horizontal_wrapped(|ui| {
                        if secondary_icon_button(
                            ui,
                            VectorIcon::Folder,
                            app.i18n.tr("Chọn...", "Browse..."),
                        )
                        .clicked()
                        {
                            if let Some(f) = crate::dialogs::pick_file_sync() {
                                app.home.edit_draft.java_path_str = f.to_string_lossy().to_string();
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
                    if app.home.edit_draft.java_path_str.trim().is_empty()
                        && java_runtime_mode_selector(
                            ui,
                            &app.i18n,
                            &mut app.settings.java_runtime_mode,
                            Some(&profile.game_version),
                        )
                    {
                        app.click_sfx();
                        app.persist_settings();
                    }

                    let jvm_label =
                        ui.label(RichText::new(app.i18n.t("profile.jvm_args")).strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut app.home.edit_draft.jvm_args)
                            .hint_text("-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions")
                            .desired_width(ui.available_width()),
                    )
                    .labelled_by(jvm_label.id);

                    ui.label(
                        RichText::new(app.i18n.tr("Cửa sổ & Độ phân giải", "Window & Resolution"))
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
                                    egui::DragValue::new(&mut app.home.edit_draft.window_width)
                                        .range(640..=7680)
                                        .suffix(" px"),
                                )
                                .on_hover_text(app.i18n.tr("Chiều rộng cửa sổ", "Window width"));
                                ui.label("×");
                                ui.add(
                                    egui::DragValue::new(&mut app.home.edit_draft.window_height)
                                        .range(480..=4320)
                                        .suffix(" px"),
                                )
                                .on_hover_text(app.i18n.tr("Chiều cao cửa sổ", "Window height"));
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

        #[cfg(target_os = "android")]
        {
            ui.add_space(10.0);
            gaming_card_3d(
                ui,
                "home_mobile_controls_cfg_card",
                false,
                ACCENT_CYAN,
                Margin::same(14),
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Màn Hình Chơi Game & Phím Điều Khiển Di Động",
                            "Mobile Game Screen & Touch Controls",
                        ))
                        .size(14.2)
                        .strong(),
                    );
                    ui.add_space(4.0);
                    let mut mobile_cfg_changed = false;
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(app.i18n.tr("Trình kết xuất:", "Graphics Renderer:"))
                                .strong(),
                        );
                        for mode in thk_core::models::MobileRendererMode::ALL {
                            let lbl = app.mobile_renderer_label(mode);
                            mobile_cfg_changed |= ui
                                .selectable_value(&mut app.settings.mobile_renderer, mode, lbl)
                                .changed();
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let scale_lbl = ui.label(
                            RichText::new(app.i18n.tr("Tỷ lệ độ phân giải:", "Render Scale:"))
                                .strong(),
                        );
                        let scale_resp = ui.add(
                            egui::Slider::new(
                                &mut app.settings.mobile_render_scale_pct,
                                thk_core::models::MIN_MOBILE_RENDER_SCALE_PCT
                                    ..=thk_core::models::MAX_MOBILE_RENDER_SCALE_PCT,
                            )
                            .step_by(5.0)
                            .suffix("%"),
                        );
                        mobile_cfg_changed |= scale_resp.changed();
                        scale_resp.labelled_by(scale_lbl.id);
                    });
                    if mobile_cfg_changed {
                        app.persist_settings();
                    }
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if primary_icon_button(
                            ui,
                            VectorIcon::Play,
                            app.i18n.tr("Mở Màn Hình Game", "Open Game Screen"),
                        )
                        .clicked()
                        {
                            app.click_sfx();
                            app.open_mobile_game_screen(false);
                        }
                        if secondary_icon_button(
                            ui,
                            VectorIcon::Sliders,
                            app.i18n.tr("Edit Phím Bedrock", "Edit Bedrock Controls"),
                        )
                        .clicked()
                        {
                            app.click_sfx();
                            app.open_mobile_game_screen(true);
                        }
                    });
                },
            );
        }
    });
}

fn find_update_candidate<'a>(
    updates: &'a [ContentUpdateCandidate],
    file_name: &str,
    base_file_name: &str,
) -> Option<&'a ContentUpdateCandidate> {
    if updates.is_empty() {
        return None;
    }
    updates.iter().find(|c| {
        c.file_name == file_name
            || c.file_name == base_file_name
            || c.file_name
                .strip_suffix(".disabled")
                .unwrap_or(&c.file_name)
                == base_file_name
    })
}

fn render_tab_content(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
    kind: ContentKind,
    standalone_scroll: bool,
) {
    let update_count = app.home.content_updates.len();

    ui.horizontal_wrapped(|ui| {
        let filter_w = 210.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
        ui.add(
            egui::TextEdit::singleline(&mut app.home.local_filter)
                .hint_text(
                    app.i18n
                        .tr("Lọc tệp đã cài đặt...", "Filter installed files..."),
                )
                .desired_width(filter_w),
        );
        if secondary_icon_button(ui, VectorIcon::Refresh, app.i18n.tr("Làm mới", "Refresh"))
            .clicked()
        {
            app.refresh_selected_profile_tabs();
        }
        if secondary_icon_button(ui, VectorIcon::Folder, app.i18n.t("home.open_folder")).clicked() {
            if let Ok(sub) =
                thk_core::SafePath::resolve_child(&profile.instance_path, kind.instance_subdir())
            {
                let _ = std::fs::create_dir_all(&sub);
                app.open_local_folder_or_toast(&sub);
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
            app.i18n.tr("Đóng kho trực tuyến", "Close Online Browser")
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
        render_inline_online_browser(app, ui, profile, kind, standalone_scroll);
        ui.add_space(8.0);
    }

    let filter_raw = app.home.local_filter.trim();
    let has_filter = !filter_raw.is_empty();
    let filter_low = if has_filter {
        filter_raw.to_lowercase()
    } else {
        String::new()
    };

    let filtered_indices: Vec<usize> = if has_filter {
        app.home
            .local_items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let title_matches = item.title.to_lowercase().contains(&filter_low);
                let file_matches = item.file_name.to_lowercase().contains(&filter_low);
                if title_matches || file_matches {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let total_items = if has_filter {
        filtered_indices.len()
    } else {
        app.home.local_items.len()
    };

    let mut toggle_req: Option<(String, bool)> = None;
    let mut delete_req: Option<String> = None;
    let mut update_req: Option<ContentUpdateCandidate> = None;

    maybe_home_scroll(
        ui,
        "local_installed_content_scroll",
        standalone_scroll,
        |ui| {
            if app.home.local_items.is_empty() {
                ui.label(
                    RichText::new(app.i18n.tr(
                        "Chưa có tệp nào trong thư mục này.",
                        "No installed packages found in this folder.",
                    ))
                    .weak(),
                );
                return;
            }
            if total_items == 0 {
                ui.label(
                    RichText::new(
                        app.i18n
                            .tr("Không tìm thấy tệp phù hợp.", "No matching files found."),
                    )
                    .weak(),
                );
                return;
            }

            let is_narrow = ui.available_width() < 500.0;
            let row_h = (if is_narrow { 86.0 } else { 72.0 }) * app.settings.font_scale.max(1.0);

            let clip = ui.clip_rect();
            let list_top = ui.cursor().top();

            let (start_idx, end_idx) = if clip.is_finite() && clip.height() > 0.0 {
                let rel_top = (clip.min.y - list_top).max(0.0);
                let rel_bottom = (clip.max.y - list_top).max(0.0);
                let first_visible = (rel_top / row_h).floor() as usize;
                let last_visible = ((rel_bottom / row_h).ceil() as usize).min(total_items);
                let start = first_visible.saturating_sub(2).min(total_items);
                let end = (last_visible + 2).min(total_items);
                (start, end.max(start))
            } else {
                (0, total_items)
            };

            let visible_textures: Vec<Option<egui::TextureHandle>> = (start_idx..end_idx)
                .map(|idx| {
                    let item_idx = if has_filter {
                        filtered_indices[idx]
                    } else {
                        idx
                    };
                    let url = app.home.local_items[item_idx].icon_url.clone();
                    if let Some(ref u) = url {
                        app.request_modpack_texture(u, 64)
                    } else {
                        None
                    }
                })
                .collect();

            if start_idx > 0 {
                ui.add_space(start_idx as f32 * row_h);
            }

            for (offset, idx) in (start_idx..end_idx).enumerate() {
                let item_idx = if has_filter {
                    filtered_indices[idx]
                } else {
                    idx
                };
                let item = &app.home.local_items[item_idx];
                let item_tex = visible_textures.get(offset).and_then(|t| t.as_ref());
                let upd_cand = find_update_candidate(
                    &app.home.content_updates,
                    &item.file_name,
                    &item.base_file_name,
                );
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
                        if is_narrow {
                            ui.horizontal(|ui| {
                                let mut enabled = item.enabled;
                                if ui.checkbox(&mut enabled, "").changed() {
                                    toggle_req = Some((item.file_name.clone(), enabled));
                                }
                                paint_square_thumbnail(
                                    ui,
                                    item_tex,
                                    34.0,
                                    card_accent,
                                    Icon3dKind::ModpackCrate,
                                    false,
                                );
                                let head_avail_w = ui.available_width();
                                ui.vertical(|ui| {
                                    ui.set_max_width(head_avail_w);
                                    bounded_wrap_label(
                                        ui,
                                        RichText::new(&item.title).strong(),
                                        head_avail_w,
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        if !item.version_number.is_empty() {
                                            badge(ui, &item.version_number, ACCENT_BLUE);
                                        }
                                        badge(ui, item.provider.display_name(), ACCENT_EMERALD);
                                        if let Some(cand) = upd_cand {
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
                                });
                            });

                            ui.add_space(3.0);

                            let file_info =
                                format!("{} • {}", item.file_name, format_bytes(item.size_bytes));
                            bounded_wrap_label(
                                ui,
                                RichText::new(file_info).size(11.5).weak(),
                                ui.available_width(),
                            );

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                let status_lbl = if item.enabled {
                                    RichText::new(app.i18n.tr("Đã bật", "Enabled"))
                                        .size(11.5)
                                        .color(ACCENT_EMERALD)
                                } else {
                                    RichText::new(app.i18n.tr("Đã tắt", "Disabled"))
                                        .size(11.5)
                                        .color(Color32::GRAY)
                                };
                                ui.label(status_lbl);

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
                                            delete_req = Some(item.file_name.clone());
                                        }
                                        if let Some(cand) = upd_cand {
                                            let btn_txt = format!(
                                                "{} {}",
                                                app.i18n.t("mods.update"),
                                                cand.latest_version.version_number
                                            );
                                            if primary_icon_button(
                                                ui,
                                                VectorIcon::Download,
                                                btn_txt,
                                            )
                                            .clicked()
                                            {
                                                update_req = Some((*cand).clone());
                                            }
                                        }
                                    },
                                );
                            });
                        } else {
                            let action_w = if upd_cand.is_some() { 240.0 } else { 92.0 };
                            ui.horizontal(|ui| {
                                let mut enabled = item.enabled;
                                if ui.checkbox(&mut enabled, "").changed() {
                                    toggle_req = Some((item.file_name.clone(), enabled));
                                }
                                paint_square_thumbnail(
                                    ui,
                                    item_tex,
                                    34.0,
                                    card_accent,
                                    Icon3dKind::ModpackCrate,
                                    false,
                                );
                                let content_max_w =
                                    (ui.available_width() - action_w - 12.0).max(100.0);
                                ui.vertical(|ui| {
                                    ui.set_max_width(content_max_w);
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new(&item.title).strong());
                                        if !item.version_number.is_empty() {
                                            badge(ui, &item.version_number, ACCENT_BLUE);
                                        }
                                        badge(ui, item.provider.display_name(), ACCENT_EMERALD);
                                        if let Some(cand) = upd_cand {
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
                                    bounded_wrap_label(
                                        ui,
                                        RichText::new(format!(
                                            "{} • {}",
                                            item.file_name,
                                            format_bytes(item.size_bytes)
                                        ))
                                        .size(11.5)
                                        .weak(),
                                        content_max_w,
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
                                            delete_req = Some(item.file_name.clone());
                                        }
                                        if let Some(cand) = upd_cand {
                                            let btn_txt = format!(
                                                "{} {}",
                                                app.i18n.t("mods.update"),
                                                cand.latest_version.version_number
                                            );
                                            if primary_icon_button(
                                                ui,
                                                VectorIcon::Download,
                                                btn_txt,
                                            )
                                            .clicked()
                                            {
                                                update_req = Some((*cand).clone());
                                            }
                                        }
                                    },
                                );
                            });
                        }
                    },
                );
                ui.add_space(4.0);
            }

            if end_idx < total_items {
                ui.add_space((total_items - end_idx) as f32 * row_h);
            }
        },
    );

    if let Some((file_name, enabled)) = toggle_req {
        app.toggle_local_item(kind, &file_name, enabled);
    }
    if let Some(file_name) = delete_req {
        app.delete_local_item(kind, &file_name);
    }
    if let Some(cand) = update_req {
        app.apply_single_content_update(cand, kind);
    }
}

fn render_inline_online_browser(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
    kind: ContentKind,
    standalone_scroll: bool,
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

                let online_q_w = 260.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut app.home.online_query)
                        .hint_text(app.i18n.t("mods.search_placeholder"))
                        .desired_width(online_q_w),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    app.search_inline_profile_content(kind);
                }
                if primary_icon_button(ui, VectorIcon::Search, app.i18n.tr("Tìm kiếm", "Search"))
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
            let mut render_results_body = |ui: &mut egui::Ui| {
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
                    let combo_text = crate::views::format_project_version_option(cur_ver, false);
                    ui.horizontal_wrapped(|ui| {
                        let ver_combo_w = 340.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                        egui::ComboBox::from_id_salt("inline_version_picker_combo")
                            .width(ver_combo_w)
                            .selected_text(combo_text)
                            .show_ui(ui, |ui| {
                                for (idx, v) in versions.iter().enumerate() {
                                    let lbl = crate::views::format_project_version_option(v, false);
                                    ui.selectable_value(
                                        &mut app.home.online_chosen_ver_idx,
                                        idx,
                                        lbl,
                                    );
                                }
                            });
                        if primary_icon_button(ui, VectorIcon::Download, app.i18n.t("mods.install"))
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
                            let canon_ver = ver.canonical_version_label();
                            ui.label(RichText::new(&canon_ver).strong());
                            if ver.name != canon_ver {
                                ui.label(RichText::new(&ver.name).weak());
                            }
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

                        let is_narrow = ui.available_width() < 500.0;
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
                                if is_narrow {
                                    ui.horizontal(|ui| {
                                        paint_square_thumbnail(
                                            ui,
                                            hit_tex.as_ref(),
                                            36.0,
                                            ACCENT_CYAN,
                                            Icon3dKind::ModpackCrate,
                                            false,
                                        );
                                        let head_avail_w = ui.available_width();
                                        ui.vertical(|ui| {
                                            ui.set_max_width(head_avail_w);
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
                                        });
                                    });

                                    ui.add_space(2.0);

                                    bounded_wrap_label(
                                        ui,
                                        RichText::new(&item.summary).size(12.0),
                                        ui.available_width(),
                                    );

                                    ui.add_space(4.0);

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if primary_icon_button(
                                                ui,
                                                VectorIcon::ArrowRight,
                                                app.i18n.tr("Chọn bản cài", "Select Version"),
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
                                } else {
                                    let btn_w = 148.0;
                                    ui.horizontal(|ui| {
                                        paint_square_thumbnail(
                                            ui,
                                            hit_tex.as_ref(),
                                            36.0,
                                            ACCENT_CYAN,
                                            Icon3dKind::ModpackCrate,
                                            false,
                                        );
                                        let content_max_w =
                                            (ui.available_width() - btn_w - 12.0).max(100.0);
                                        ui.vertical(|ui| {
                                            ui.set_max_width(content_max_w);
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
                                            bounded_wrap_label(
                                                ui,
                                                RichText::new(&item.summary).size(12.0),
                                                content_max_w,
                                            );
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if primary_icon_button(
                                                    ui,
                                                    VectorIcon::ArrowRight,
                                                    app.i18n.tr("Chọn bản cài", "Select Version"),
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
                                }
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
                            format!("Load More ({}/{})", hits.len(), app.home.online_total_hits)
                        };
                        if secondary_icon_button(ui, VectorIcon::Download, more_lbl).clicked() {
                            let next_page = app.home.online_page + 1;
                            app.search_inline_profile_content_page(kind, next_page, true);
                        }
                    }
                }
            };
            if standalone_scroll {
                ScrollArea::vertical()
                    .id_salt("inline_online_results")
                    .max_height(245.0)
                    .show(ui, |ui| render_results_body(ui));
            } else {
                render_results_body(ui);
            }
        },
    );
}

fn render_tab_worlds(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
    standalone_scroll: bool,
) {
    let worlds = app.home.worlds.clone();
    maybe_home_scroll(ui, "home_worlds_scroll", standalone_scroll, |ui| {
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
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                                    app.open_local_folder_or_toast(&p);
                                }
                            }
                        });
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
    standalone_scroll: bool,
) {
    ui.horizontal_wrapped(|ui| {
        if secondary_icon_button(ui, VectorIcon::Folder, app.i18n.tr("Gốc (/)", "Root (/)"))
            .clicked()
        {
            app.home.current_rel_dir.clear();
            app.home.selected_entries.clear();
            app.refresh_selected_profile_tabs();
        }
        if !app.home.current_rel_dir.is_empty()
            && secondary_icon_button(ui, VectorIcon::ArrowLeft, app.i18n.tr("Lên cấp trên", "Up"))
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
            app.open_local_folder_or_toast(&target);
        }

        ui.separator();
        let arch_w = 135.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
        ui.add(
            egui::TextEdit::singleline(&mut app.home.archive_name_input)
                .hint_text("instance_backup.zip")
                .desired_width(arch_w),
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
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}: {editing_rel}",
                            app.i18n.tr("Đang chỉnh sửa", "Editing")
                        ))
                        .strong()
                        .color(ACCENT_EMERALD),
                    );
                    if ui.available_width() >= 190.0 {
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
                    } else {
                        if primary_icon_button(
                            ui,
                            VectorIcon::Save,
                            app.i18n.tr("Lưu tệp", "Save File"),
                        )
                        .clicked()
                        {
                            app.save_profile_file_editor();
                        }
                        if secondary_button(ui, app.i18n.tr("Đóng", "Close")).clicked() {
                            app.home.editing_file_rel = None;
                        }
                    }
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
    maybe_home_scroll(ui, "profile_files_list_scroll", standalone_scroll, |ui| {
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
                if entry.is_dir {
                    ACCENT_CYAN
                } else {
                    ACCENT_BLUE
                },
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

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                        });
                    });
                },
            );
            ui.add_space(4.0);
        }
    });
}

fn render_tab_bookmarks(app: &mut LauncherAppState, ui: &mut egui::Ui, standalone_scroll: bool) {
    ui.horizontal_wrapped(|ui| {
        let max_bm_w = (ui.max_rect().width() - 16.0).max(72.0);
        ui.add(
            egui::TextEdit::singleline(&mut app.home.new_bm_name)
                .hint_text(
                    app.i18n
                        .tr("Tên máy chủ (VD: Hypixel)", "Server Name (e.g. Hypixel)"),
                )
                .desired_width(175.0_f32.min(max_bm_w)),
        );
        ui.add(
            egui::TextEdit::singleline(&mut app.home.new_bm_addr)
                .hint_text(app.i18n.tr(
                    "Địa chỉ IP / Domain (VD: mc.hypixel.net)",
                    "IP / Domain (e.g. mc.hypixel.net)",
                ))
                .desired_width(240.0_f32.min(max_bm_w)),
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

    maybe_home_scroll(ui, "home_bookmarks_scroll", standalone_scroll, |ui| {
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
                if is_online {
                    ACCENT_EMERALD
                } else {
                    ACCENT_BLUE
                },
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
                                        ui.label(RichText::new(&st.version_name).size(11.5).weak());
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

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                        });
                    });
                },
            );
            ui.add_space(6.0);
        }
    });
}

#[cfg(not(target_os = "android"))]
pub(super) fn render_detached_game_logs_panel(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    render_compact_profile_bar(app, ui);
    ui.add_space(8.0);
    if let Some(profile) = app.selected_profile().cloned() {
        render_tab_logs(app, ui, &profile, None);
    } else {
        ui.label(RichText::new(app.i18n.t("home.no_profiles")).weak());
    }
}

fn render_tab_logs(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    profile: &thk_core::models::GameProfile,
    workspace_viewport_h: Option<f32>,
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

    let filter_w = (ui.available_width() * 0.42).clamp(140.0, 240.0);
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.home.log_filter)
                .hint_text(app.i18n.tr("Lọc dòng nhật ký...", "Filter log lines..."))
                .desired_width(filter_w),
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
            let joined = lines.join("\n");
            crate::dialogs::sync_clipboard_text(ui.ctx(), &joined);
            let msg = app
                .i18n
                .tr(
                    "Đã sao chép nhật ký vào Clipboard.",
                    "Copied logs to clipboard.",
                )
                .to_string();
            app.push_toast(ToastKind::Info, msg);
        }

        #[cfg(not(target_os = "android"))]
        if ui.ctx().viewport_id() == egui::ViewportId::ROOT
            && secondary_icon_button(
                ui,
                VectorIcon::MultiWindow,
                app.i18n.tr("Tách cửa sổ Nhật ký", "Pop-out Logs"),
            )
            .clicked()
        {
            app.click_sfx();
            app.detached.game_logs_window_open = true;
        }
    });

    ui.add_space(6.0);
    let filter_low = app.home.log_filter.trim().to_ascii_lowercase();
    let filtered: Vec<&String> = lines
        .iter()
        .filter(|l| filter_low.is_empty() || l.to_ascii_lowercase().contains(&filter_low))
        .collect();

    let dark = ui.visuals().dark_mode;
    Frame::canvas(ui.style())
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            let row_h = ui.text_style_height(&egui::TextStyle::Monospace).max(14.0);
            let scroll = if let Some(vh) = workspace_viewport_h {
                let logs_h = (vh - 84.0).clamp(240.0, 640.0);
                ScrollArea::vertical()
                    .id_salt("virtualized_game_logs")
                    .max_height(logs_h)
                    .min_scrolled_height(logs_h)
                    .stick_to_bottom(app.home.auto_scroll_logs)
                    .auto_shrink([false, false])
            } else {
                let logs_h = ui.available_height().max(120.0);
                ScrollArea::vertical()
                    .id_salt("virtualized_game_logs")
                    .max_height(logs_h)
                    .stick_to_bottom(app.home.auto_scroll_logs)
                    .auto_shrink([false, false])
            };
            scroll.show_rows(ui, row_h, filtered.len(), |ui, range| {
                for idx in range {
                    let line = filtered[idx];
                    let col = if line.contains("ERROR") || line.contains("Exception") {
                        readable_accent_ink(ACCENT_RED, dark)
                    } else if line.contains("WARN") {
                        readable_accent_ink(ACCENT_AMBER, dark)
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
    standalone_scroll: bool,
) {
    maybe_home_scroll(ui, "home_analytics_scroll", standalone_scroll, |ui| {
        let dark = ui.visuals().dark_mode;
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

        let kpi_items = [
            (
                "kpi_playtime",
                &lbl_playtime,
                format_duration_hms(profile.total_play_seconds),
                ACCENT_EMERALD,
            ),
            (
                "kpi_sessions",
                &lbl_sessions,
                profile.session_count.to_string(),
                ACCENT_CYAN,
            ),
            (
                "kpi_crashes",
                &lbl_crashes,
                profile.crash_count.to_string(),
                ACCENT_RED,
            ),
        ];

        if ui.available_width() < 480.0 {
            for (id_s, title, val, col) in kpi_items {
                gaming_card_3d(ui, id_s, false, col, Margin::same(12), |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(title).weak());
                    ui.heading(
                        RichText::new(val)
                            .size(20.0)
                            .strong()
                            .color(readable_accent_ink(col, dark)),
                    );
                });
                ui.add_space(6.0);
            }
        } else {
            ui.columns(3, |cols| {
                for (idx, (id_s, title, val, col)) in kpi_items.into_iter().enumerate() {
                    gaming_card_3d(&mut cols[idx], id_s, false, col, Margin::same(14), |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(title).weak());
                        ui.heading(
                            RichText::new(val)
                                .size(22.0)
                                .strong()
                                .color(readable_accent_ink(col, dark)),
                        );
                    });
                }
            });
        }

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
    });
}
