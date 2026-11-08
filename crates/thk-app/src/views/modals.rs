use crate::fx3d::{
    gaming_card_3d, paint_3d_tunnel_conduit, paint_glowing_progress_bar, paint_icon_3d,
    tactile_button_3d, ButtonStyle3d, Icon3dKind, VectorIcon,
};
use crate::state::{DeleteTarget, LauncherAppState, NewServerSourceMode};
use crate::theme::{
    accent_palette, readable_accent_ink, ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD,
    ACCENT_ORANGE, ACCENT_RED, ACCENT_VIOLET,
};
use crate::views::modpacks::{paint_pack_thumbnail, truncate_chars};
use crate::views::{
    badge, centered_window_rect, danger_button, danger_icon_button, font_scale_selector,
    format_bytes, format_project_version_option, java_runtime_mode_selector,
    network_interface_selector_ui, primary_icon_button, render_direct_host_endpoints_ui,
    secondary_button, secondary_icon_button,
};
use egui::{vec2, Align, Color32, Frame, Key, Layout, Margin, Order, RichText, ScrollArea};
use thk_core::models::{
    AccentPreset, CloseBehavior, JavaRuntimeMode, Language, LoaderType, ProviderSource,
    ServerEngine, ThemeMode,
};
use thk_core::RecoveryArtifactKind;
use thk_java::JavaDistroKind;
use thk_launcher::CrashFixAction;
use thk_mods::{ProjectSummary, ServerModpackFormat};
use thk_network::{DirectHostStatus, TunnelStatus};

pub fn render_all_modals(app: &mut LauncherAppState, ctx: &egui::Context) {
    if !app.settings.setup_completed {
        render_initial_setup_modal(app, ctx);
        return;
    }

    if app.show_new_profile_modal {
        render_new_profile_modal(app, ctx);
    }
    if app.show_new_server_modal {
        render_new_server_modal(app, ctx);
    }
    if app.show_java_modal {
        render_java_manager_modal(app, ctx);
    }
    if app.show_lan_modal {
        render_lan_share_modal(app, ctx);
    }
    if app.show_settings_modal {
        render_settings_modal(app, ctx);
    }
    if app.crash_modal.is_some() {
        render_crash_analyzer_modal(app, ctx);
    }
    if app.pending_eula_server_id.is_some() {
        render_server_eula_confirm_modal(app, ctx);
    }
    if app.pending_delete_modal.is_some() {
        render_delete_confirm_modal(app, ctx);
    }
    if app.disk_pack_modal.as_ref().is_some_and(|m| !m.minimized) {
        render_disk_pack_import_modal(app, ctx);
    }
    if app.show_close_confirm_modal {
        render_close_confirm_modal(app, ctx);
    }
    crate::views::modpacks::render_modpack_modals(app, ctx);
}

fn render_initial_setup_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let safe_w = crate::views::safe_content_rect(ctx).width();
    let modal_w = (safe_w - 20.0)
        .clamp(150.0, 500.0)
        .min((safe_w - 8.0).max(120.0));
    egui::Modal::new(egui::Id::unique("thk_initial_setup_modal")).show(ctx, |ui| {
        ui.set_width(modal_w);
        let dark = ui.visuals().dark_mode;
        ui.vertical_centered(|ui| {
            paint_icon_3d(ui, 68.0, Icon3dKind::BrandVoxelCore, true);
            ui.add_space(4.0);
            ui.heading(
                RichText::new(app.i18n.tr(
                    "Chào mừng đến với TriHienKun Laucher!",
                    "Welcome to TriHienKun Laucher!",
                ))
                .size(20.5)
                .strong()
                .color(readable_accent_ink(ACCENT_EMERALD, dark)),
            );
            ui.label(
                RichText::new(app.i18n.tr(
                    "Trình khởi chạy Minecraft gọn nhẹ, tốc độ cao và hoạt động độc lập.",
                    "Fast, lightweight, and standalone Minecraft Launcher.",
                ))
                .size(12.5)
                .weak(),
            );
        });
        ui.add_space(10.0);

        let mut changed = false;

        gaming_card_3d(
            ui,
            "welcome_setup_card",
            true,
            ACCENT_EMERALD,
            Margin::same(14),
            |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Ngôn ngữ / Language:").strong());
                    if ui
                        .selectable_value(&mut app.settings.language, Language::Vi, "Tiếng Việt")
                        .changed()
                    {
                        changed = true;
                    }
                    if ui
                        .selectable_value(&mut app.settings.language, Language::En, "English")
                        .changed()
                    {
                        changed = true;
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(app.i18n.tr("Giao diện màu:", "Color Theme:")).strong());
                    if ui
                        .selectable_value(
                            &mut app.settings.theme,
                            ThemeMode::Light,
                            app.i18n.tr("Sáng", "Light"),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    if ui
                        .selectable_value(
                            &mut app.settings.theme,
                            ThemeMode::Dark,
                            app.i18n.tr("Tối", "Dark"),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                });

                changed |= font_scale_selector(
                    ui,
                    &app.i18n,
                    &mut app.settings.font_scale,
                    &mut app.font_scale_drag_dirty,
                );

                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(app.i18n.tr("RAM mặc định cho Game:", "Default Game RAM:"))
                            .strong(),
                    );
                    if ui
                        .add(
                            egui::Slider::new(&mut app.settings.default_ram_mb, 1024..=16384)
                                .step_by(512.0)
                                .suffix(" MB"),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                });
            },
        );

        if changed {
            app.persist_settings();
        }

        ui.add_space(14.0);
        let btn_w = (ui.available_width() - 12.0)
            .clamp(120.0, 320.0)
            .min(ui.available_width().max(96.0));
        ui.vertical_centered(|ui| {
            if tactile_button_3d(
                ui,
                Some(VectorIcon::Play),
                app.i18n.tr(
                    "Bắt đầu sử dụng TriHienKun Laucher",
                    "Get Started with TriHienKun Laucher",
                ),
                ButtonStyle3d::HeroLaunch,
                vec2(btn_w, 44.0),
            )
            .clicked()
            {
                app.click_sfx();
                app.settings.setup_completed = true;
                app.home.new_draft.ram_mb = app.settings.default_ram_mb;
                app.persist_settings();
            }
        });
    });
}

fn render_new_profile_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let mut open = app.show_new_profile_modal;
    let title = app
        .i18n
        .tr("Tạo Profile Minecraft Mới", "Create New Minecraft Profile")
        .to_string();
    let safe_rect = crate::views::safe_content_rect(ctx);
    let init_rect = centered_window_rect(ctx, vec2(500.0, 520.0));
    egui::Window::new(title)
        .id(egui::Id::unique("thk_modal_new_profile"))
        .default_rect(init_rect)
        .min_size(vec2(300.0, 280.0).min(init_rect.size()))
        .collapsible(false)
        .resizable(true)
        .constrain_to(safe_rect)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                paint_icon_3d(ui, 34.0, Icon3dKind::HomePortal, true);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Khởi tạo Instance Độc lập",
                            "Create Isolated Game Instance",
                        ))
                        .size(15.0)
                        .strong(),
                    );
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Hỗ trợ Vanilla, Fabric, Quilt, Forge & NeoForge",
                            "Supports Vanilla, Fabric, Quilt, Forge & NeoForge",
                        ))
                        .size(11.5)
                        .weak(),
                    );
                });
            });
            ui.add_space(6.0);

            let max_body_h = (ui.available_height() - 48.0).max(150.0);
            let scroll_w = ui.available_width();
            ScrollArea::vertical()
                .id_salt("new_profile_modal_body_scroll")
                .max_height(max_body_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(scroll_w);
                    ui.label(
                        RichText::new(app.i18n.tr("Tên Profile:", "Profile Name:")).strong(),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut app.home.new_draft.name)
                            .hint_text("Survival 1.21.4")
                            .desired_width(f32::INFINITY),
                    );

                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(
                                app.i18n.tr("Phiên bản Minecraft:", "Minecraft Version:"),
                            )
                            .strong(),
                        );
                        let snap_label = app
                            .i18n
                            .tr("Hiện bản Snapshot", "Include Snapshots")
                            .to_string();
                        crate::views::wrapped_checkbox(
                            ui,
                            &mut app.home.include_snapshots,
                            snap_label,
                        );
                    });

                    let include_snaps = app.home.include_snapshots;
                    let mc_versions: Vec<String> = app
                        .home
                        .mojang_versions
                        .iter()
                        .filter(|v| include_snaps || v.release_type == "release")
                        .map(|v| v.id.clone())
                        .collect();

                    let prev_mc = app.home.new_draft.game_version.clone();
                    let mc_combo_w = 260.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                    egui::ComboBox::from_id_salt("new_profile_mc_ver_combo")
                        .width(mc_combo_w)
                        .truncate()
                        .selected_text(&app.home.new_draft.game_version)
                        .show_ui(ui, |ui| {
                            for ver in &mc_versions {
                                ui.selectable_value(
                                    &mut app.home.new_draft.game_version,
                                    ver.clone(),
                                    ver,
                                );
                            }
                        });
                    if app.home.new_draft.game_version != prev_mc {
                        let mc = app.home.new_draft.game_version.clone();
                        let ldr = app.home.new_draft.loader;
                        app.home.new_draft.loader_version.clear();
                        app.ensure_loader_versions(&mc, ldr);
                    }

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            app.i18n
                                .tr("Trình nạp Mod (Mod Loader):", "Mod Loader:"),
                        )
                        .strong(),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for ldr in LoaderType::ALL {
                            if ui
                                .selectable_label(
                                    app.home.new_draft.loader == ldr,
                                    ldr.display_name(),
                                )
                                .clicked()
                            {
                                app.home.new_draft.loader = ldr;
                                app.home.new_draft.loader_version.clear();
                                let mc = app.home.new_draft.game_version.clone();
                                app.ensure_loader_versions(&mc, ldr);
                            }
                        }
                    });

                    if app.home.new_draft.loader != LoaderType::Vanilla {
                        ui.add_space(4.0);
                        let key = (
                            app.home.new_draft.game_version.clone(),
                            app.home.new_draft.loader,
                        );
                        let ldr_entries = app
                            .home
                            .loader_versions
                            .get(&key)
                            .cloned()
                            .unwrap_or_default();

                        if app.home.new_draft.loader_version.is_empty() {
                            if let Some(first) = ldr_entries.first() {
                                app.home.new_draft.loader_version = first.version.clone();
                            }
                        }

                        let auto_lbl = app
                            .i18n
                            .tr("Tự động chọn mới nhất", "Auto-select latest")
                            .to_string();
                        let stable_word = app.i18n.tr("Ổn định", "Stable").to_string();
                        ui.horizontal_wrapped(|ui| {
                            ui.label(app.i18n.tr("Phiên bản Loader:", "Loader Version:"));
                            let sel_txt = if app.home.new_draft.loader_version.is_empty() {
                                auto_lbl.as_str()
                            } else {
                                app.home.new_draft.loader_version.as_str()
                            };
                            let ldr_combo_w =
                                230.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                            egui::ComboBox::from_id_salt("new_profile_loader_ver_combo")
                                .width(ldr_combo_w)
                                .truncate()
                                .selected_text(sel_txt)
                                .show_ui(ui, |ui| {
                                    for entry in ldr_entries {
                                        let tag = if entry.stable {
                                            format!("{} ({stable_word})", entry.version)
                                        } else {
                                            entry.version.clone()
                                        };
                                        ui.selectable_value(
                                            &mut app.home.new_draft.loader_version,
                                            entry.version,
                                            tag,
                                        );
                                    }
                                });
                        });
                    }

                    if app.home.new_draft.loader == LoaderType::Fabric {
                        let perf_lbl = app
                            .i18n
                            .tr(
                                "Tự động cài bộ Mod tối ưu FPS (Sodium, Lithium, FerriteCore, EntityCulling, ModMenu)",
                                "Auto-install FPS Optimizer Suite (Sodium, Lithium, FerriteCore, EntityCulling, ModMenu)",
                            )
                            .to_string();
                        crate::views::wrapped_checkbox(
                            ui,
                            &mut app.home.new_draft.auto_perf_mods,
                            perf_lbl,
                        );
                    }

                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(app.i18n.t("profile.ram")).strong());
                        ui.add(
                            egui::Slider::new(&mut app.home.new_draft.ram_mb, 1024..=16384)
                                .step_by(512.0)
                                .suffix(" MB"),
                        );
                    });

                    ui.add_space(4.0);
                    let draft_mc = app.home.new_draft.game_version.clone();
                    if java_runtime_mode_selector(
                        ui,
                        &app.i18n,
                        &mut app.settings.java_runtime_mode,
                        Some(&draft_mc),
                    ) {
                        app.click_sfx();
                        app.persist_settings();
                    }

                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(
                                app.i18n.tr("Thư mục lưu trữ:", "Instance Folder:"),
                            )
                            .strong(),
                        );
                        let dir_label = app
                            .home
                            .new_draft
                            .custom_dir
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| {
                                app.i18n
                                    .tr("Mặc định (instances/)", "Default (instances/)")
                                    .to_string()
                            });
                        ui.label(RichText::new(dir_label).size(11.5).weak());
                        if secondary_icon_button(
                            ui,
                            VectorIcon::Folder,
                            app.i18n.tr("Chọn thư mục...", "Choose Folder..."),
                        )
                        .clicked()
                        {
                            if let Some(folder) = crate::dialogs::pick_folder_sync() {
                                app.home.new_draft.custom_dir = Some(folder);
                            }
                        }
                        if app.home.new_draft.custom_dir.is_some()
                            && secondary_icon_button(
                                ui,
                                VectorIcon::Refresh,
                                app.i18n.tr("Mặc định", "Reset"),
                            )
                            .clicked()
                        {
                            app.home.new_draft.custom_dir = None;
                        }
                    });
                });

            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if primary_icon_button(ui, VectorIcon::Plus, app.i18n.t("home.new_profile"))
                    .clicked()
                {
                    app.create_profile_from_draft();
                }
                if secondary_button(ui, app.i18n.t("common.cancel")).clicked() {
                    app.show_new_profile_modal = false;
                }
            });
        });
    app.show_new_profile_modal &= open;
}

fn render_new_server_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let mut open = app.show_new_server_modal;
    let title = app
        .i18n
        .tr(
            "Tạo Máy Chủ Minecraft Cục Bộ",
            "Create Local Minecraft Server",
        )
        .to_string();
    let safe_rect = crate::views::safe_content_rect(ctx);
    let init_rect = centered_window_rect(ctx, vec2(575.0, 575.0));
    egui::Window::new(title)
        .id(egui::Id::unique("thk_modal_new_server"))
        .default_rect(init_rect)
        .min_size(vec2(300.0, 290.0).min(init_rect.size()))
        .collapsible(false)
        .resizable(true)
        .constrain_to(safe_rect)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                paint_icon_3d(ui, 34.0, Icon3dKind::ServerBladeTower, true);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Triển khai Máy chủ Cục bộ",
                            "Deploy Local Server",
                        ))
                        .size(15.0)
                        .strong(),
                    );
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Tạo từ lõi tiêu chuẩn, Modpack Cloud, tệp nén hoặc Profile có sẵn",
                            "Create from standard engine, Cloud Modpack, archive, or profile",
                        ))
                        .size(11.5)
                        .weak(),
                    );
                });
            });
            ui.add_space(6.0);

            let creating = app.servers.new_draft.creating;
            let create_task_prog = app
                .active_tasks
                .get("server-pack-create")
                .map(|t| t.progress.clone());
            let mut ask_pick_pack = false;
            let mut ask_cloud_search: Option<(usize, bool)> = None;
            let mut ask_select_cloud: Option<ProjectSummary> = None;
            let mut ask_create = false;
            let mut ask_cancel_create = false;

            let max_body_h = (ui.available_height() - 48.0).max(160.0);
            ScrollArea::vertical()
                .id_salt("new_server_modal_body_scroll")
                .max_height(max_body_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_enabled_ui(!creating, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(app.i18n.tr("Nguồn tạo:", "Source:")).strong());
                    if ui
                        .selectable_value(
                            &mut app.servers.new_draft.source_mode,
                            NewServerSourceMode::StandardEngine,
                            app.i18n.tr("Lõi tiêu chuẩn", "Standard"),
                        )
                        .clicked()
                    {
                        app.servers.new_draft.create_error = None;
                    }
                    if ui
                        .selectable_value(
                            &mut app.servers.new_draft.source_mode,
                            NewServerSourceMode::CloudModpack,
                            app.i18n.tr("Từ Cloud", "Cloud Modpack"),
                        )
                        .clicked()
                    {
                        app.servers.new_draft.create_error = None;
                        if app.servers.new_draft.cloud_hits.is_empty()
                            && !app.servers.new_draft.cloud_searching
                        {
                            ask_cloud_search = Some((0, false));
                        }
                    }
                    if ui
                        .selectable_value(
                            &mut app.servers.new_draft.source_mode,
                            NewServerSourceMode::ModpackArchive,
                            app.i18n.tr("Từ tệp nén", "Archive File"),
                        )
                        .clicked()
                    {
                        app.servers.new_draft.create_error = None;
                    }
                    if ui
                        .selectable_value(
                            &mut app.servers.new_draft.source_mode,
                            NewServerSourceMode::ExistingProfile,
                            app.i18n.tr("Từ Profile có sẵn", "Installed Profile"),
                        )
                        .clicked()
                    {
                        app.servers.new_draft.create_error = None;
                        if app.servers.new_draft.source_profile_id.is_none() {
                            app.servers.new_draft.source_profile_id = app
                                .home
                                .selected_profile_id
                                .clone()
                                .or_else(|| app.home.profiles.first().map(|p| p.id.clone()));
                        }
                    }
                });

                ui.add_space(6.0);
                ui.label(RichText::new(app.i18n.tr("Tên Máy chủ:", "Server Name:")).strong());
                ui.add(
                    egui::TextEdit::singleline(&mut app.servers.new_draft.name)
                        .hint_text("TriHienKun SMP")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);

                match app.servers.new_draft.source_mode {
                    NewServerSourceMode::StandardEngine => {
                        ui.label(
                            RichText::new(app.i18n.tr("Lõi Máy chủ:", "Server Engine:")).strong(),
                        );
                        ui.horizontal_wrapped(|ui| {
                            for eng in ServerEngine::ALL {
                                if ui
                                    .selectable_label(
                                        app.servers.new_draft.engine == eng,
                                        eng.display_name(),
                                    )
                                    .clicked()
                                {
                                    app.servers.new_draft.engine = eng;
                                    app.ensure_engine_versions(eng);
                                    if let Some(first_ver) = app
                                        .servers
                                        .engine_versions
                                        .get(&eng)
                                        .and_then(|v| v.first())
                                        .cloned()
                                    {
                                        app.servers.new_draft.game_version = first_ver;
                                    }
                                }
                            }
                        });

                        ui.add_space(6.0);
                        let cur_eng = app.servers.new_draft.engine;
                        let eng_vers = app
                            .servers
                            .engine_versions
                            .get(&cur_eng)
                            .cloned()
                            .unwrap_or_default();

                        ui.horizontal_wrapped(|ui| {
                            let max_row_w = (ui.max_rect().width() - 16.0).max(72.0);
                            ui.label(
                                RichText::new(
                                    app.i18n.tr("Phiên bản Minecraft:", "Minecraft Version:"),
                                )
                                .strong(),
                            );
                            if eng_vers.is_empty() {
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut app.servers.new_draft.game_version,
                                    )
                                    .desired_width(140.0_f32.min(max_row_w)),
                                );
                            } else {
                                egui::ComboBox::from_id_salt("new_server_ver_combo")
                                    .width(180.0_f32.min(max_row_w))
                                    .truncate()
                                    .selected_text(&app.servers.new_draft.game_version)
                                    .show_ui(ui, |ui| {
                                        for v in eng_vers {
                                            ui.selectable_value(
                                                &mut app.servers.new_draft.game_version,
                                                v.clone(),
                                                v,
                                            );
                                        }
                                    });
                            }

                            ui.label(app.i18n.tr("Cổng:", "Port:"));
                            ui.add(
                                egui::DragValue::new(&mut app.servers.new_draft.port)
                                    .range(1024..=65535),
                            );
                        });
                    }
                    NewServerSourceMode::CloudModpack => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(app.i18n.tr("Kho lưu trữ:", "Provider:")).strong(),
                            );
                            for prov in [
                                ProviderSource::Modrinth,
                                ProviderSource::CurseForge,
                                ProviderSource::Ftb,
                                ProviderSource::Technic,
                            ] {
                                if ui
                                    .selectable_label(
                                        app.servers.new_draft.cloud_provider == prov,
                                        prov.display_name(),
                                    )
                                    .clicked()
                                    && app.servers.new_draft.cloud_provider != prov
                                {
                                    app.servers.new_draft.cloud_provider = prov;
                                    app.servers.new_draft.cloud_selected = None;
                                    ask_cloud_search = Some((0, false));
                                }
                            }
                        });

                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            let max_row_w = (ui.max_rect().width() - 16.0).max(72.0);
                            let query_w = (ui.available_width() - 210.0)
                                .max(150.0)
                                .min(max_row_w);
                            let q_resp = ui.add(
                                egui::TextEdit::singleline(&mut app.servers.new_draft.cloud_query)
                                    .hint_text(
                                        app.i18n.tr("Tìm Modpack...", "Search modpacks..."),
                                    )
                                    .desired_width(query_w),
                            );
                            let mc_resp = ui.add(
                                egui::TextEdit::singleline(
                                    &mut app.servers.new_draft.cloud_mc_filter,
                                )
                                .hint_text(app.i18n.tr("MC (vd 1.21.1)", "MC (e.g. 1.21.1)"))
                                .desired_width(110.0_f32.min(max_row_w)),
                            );
                            let enter_pressed = (q_resp.lost_focus() || mc_resp.lost_focus())
                                && ui.input(|i| i.key_pressed(Key::Enter));
                            if secondary_icon_button(
                                ui,
                                VectorIcon::Search,
                                app.i18n.tr("Tìm", "Search"),
                            )
                            .clicked()
                                || enter_pressed
                            {
                                ask_cloud_search = Some((0, false));
                            }
                        });

                        ui.add_space(4.0);
                        let selected_pid = app
                            .servers
                            .new_draft
                            .cloud_selected
                            .as_ref()
                            .map(|(s, _)| s.project_id.clone());
                        let cloud_hits = app.servers.new_draft.cloud_hits.clone();
                        let hits_scroll_h = (max_body_h * 0.42).clamp(150.0, 320.0);
                        Frame::group(ui.style())
                            .inner_margin(Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let max_hit_chars =
                                    ((ui.available_width() - 180.0) / 7.2).clamp(32.0, 96.0)
                                        as usize;
                                ScrollArea::vertical()
                                    .id_salt("new_srv_cloud_hits_scroll")
                                    .max_height(hits_scroll_h)
                                    .show(ui, |ui| {
                                        if app.servers.new_draft.cloud_searching
                                            && cloud_hits.is_empty()
                                        {
                                            ui.horizontal(|ui| {
                                                ui.spinner();
                                                ui.label(app.i18n.tr(
                                                    "Đang tìm kiếm Modpack...",
                                                    "Searching modpacks...",
                                                ));
                                            });
                                        } else if cloud_hits.is_empty() {
                                            ui.label(
                                                RichText::new(app.i18n.tr(
                                                    "Không tìm thấy gói Modpack phù hợp.",
                                                    "No matching modpacks found.",
                                                ))
                                                .size(11.8)
                                                .weak(),
                                            );
                                        } else {
                                            for hit in &cloud_hits {
                                                let is_sel = selected_pid.as_deref()
                                                    == Some(hit.project_id.as_str());
                                                let hit_tex = hit
                                                    .icon_url
                                                    .as_deref()
                                                    .and_then(|u| app.request_modpack_texture(u, 64));
                                                ui.horizontal(|ui| {
                                                    paint_pack_thumbnail(
                                                        ui,
                                                        hit_tex.as_ref(),
                                                        30.0,
                                                        if is_sel {
                                                            ACCENT_EMERALD
                                                        } else {
                                                            ACCENT_CYAN
                                                        },
                                                        is_sel,
                                                    );
                                                    ui.label(
                                                        RichText::new(truncate_chars(
                                                            &hit.title,
                                                            max_hit_chars,
                                                        ))
                                                        .size(12.2)
                                                        .strong()
                                                        .color(if is_sel {
                                                            ACCENT_EMERALD
                                                        } else {
                                                            ui.visuals().text_color()
                                                        }),
                                                    );
                                                    if !hit.author.trim().is_empty() {
                                                        ui.label(
                                                            RichText::new(truncate_chars(
                                                                &hit.author,
                                                                16,
                                                            ))
                                                            .size(11.0)
                                                            .weak(),
                                                        );
                                                    }
                                                    ui.with_layout(
                                                        Layout::right_to_left(Align::Center),
                                                        |ui| {
                                                            let btn_lbl = if is_sel {
                                                                app.i18n
                                                                    .tr("Đã chọn", "Selected")
                                                            } else {
                                                                app.i18n.tr("Chọn", "Select")
                                                            };
                                                            if ui
                                                                .selectable_label(is_sel, btn_lbl)
                                                                .clicked()
                                                                && !is_sel
                                                            {
                                                                ask_select_cloud =
                                                                    Some(hit.clone());
                                                            }
                                                        },
                                                    );
                                                });
                                                ui.add_space(2.0);
                                            }
                                            if cloud_hits.len()
                                                < app.servers.new_draft.cloud_total_hits
                                            {
                                                ui.add_space(4.0);
                                                if app.servers.new_draft.cloud_searching {
                                                    ui.spinner();
                                                } else if secondary_button(
                                                    ui,
                                                    app.i18n.tr("Tải thêm...", "Load More..."),
                                                )
                                                .clicked()
                                                {
                                                    ask_cloud_search = Some((
                                                        app.servers.new_draft.cloud_page + 1,
                                                        true,
                                                    ));
                                                }
                                            }
                                        }
                                    });
                            });

                        ui.add_space(4.0);
                        if app.servers.new_draft.cloud_loading_versions {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(app.i18n.tr(
                                    "Đang tải danh sách phiên bản...",
                                    "Loading versions...",
                                ));
                            });
                        } else if let Some((sel_summary, sel_vers)) =
                            app.servers.new_draft.cloud_selected.clone()
                        {
                            let sel_tex = sel_summary
                                .icon_url
                                .as_deref()
                                .and_then(|u| app.request_modpack_texture(u, 64));
                            ui.horizontal(|ui| {
                                paint_pack_thumbnail(
                                    ui,
                                    sel_tex.as_ref(),
                                    24.0,
                                    ACCENT_EMERALD,
                                    true,
                                );
                                ui.label(
                                    RichText::new(truncate_chars(&sel_summary.title, 48))
                                        .size(12.2)
                                        .strong()
                                        .color(ACCENT_EMERALD),
                                );
                            });
                            ui.add_space(2.0);

                            if sel_vers.is_empty() {
                                ui.label(
                                    RichText::new(app.i18n.tr(
                                        "Dự án chưa có phiên bản tải xuống khả dụng.",
                                        "No downloadable versions available.",
                                    ))
                                    .size(11.8)
                                    .color(ACCENT_AMBER),
                                );
                            } else {
                                let mut ver_idx = app
                                    .servers
                                    .new_draft
                                    .cloud_chosen_ver_idx
                                    .min(sel_vers.len() - 1);
                                let cur_lbl =
                                    format_project_version_option(&sel_vers[ver_idx], false);
                                let has_srv_pack = sel_vers[ver_idx].server_pack_file_id.is_some();

                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(app.i18n.tr("Phiên bản:", "Version:"))
                                            .strong(),
                                    );
                                    let combo_w = (ui.available_width() - 235.0)
                                        .max(220.0)
                                        .min((ui.max_rect().width() - 16.0).max(72.0));
                                    egui::ComboBox::from_id_salt("new_srv_cloud_ver_combo")
                                        .width(combo_w)
                                        .truncate()
                                        .selected_text(cur_lbl)
                                        .show_ui(ui, |ui| {
                                            for (idx, v) in sel_vers.iter().enumerate() {
                                                ui.selectable_value(
                                                    &mut ver_idx,
                                                    idx,
                                                    format_project_version_option(v, true),
                                                );
                                            }
                                        });
                                    if has_srv_pack {
                                        badge(ui, "Server Pack", ACCENT_EMERALD);
                                    }
                                    ui.label(app.i18n.tr("Cổng:", "Port:"));
                                    ui.add(
                                        egui::DragValue::new(&mut app.servers.new_draft.port)
                                            .range(1024..=65535),
                                    );
                                });
                                app.servers.new_draft.cloud_chosen_ver_idx = ver_idx;
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(app.i18n.tr("Cổng:", "Port:")).strong());
                                ui.add(
                                    egui::DragValue::new(&mut app.servers.new_draft.port)
                                        .range(1024..=65535),
                                );
                            });
                        }
                    }
                    NewServerSourceMode::ModpackArchive => {
                        ui.horizontal_wrapped(|ui| {
                            if secondary_icon_button(
                                ui,
                                VectorIcon::Folder,
                                app.i18n
                                    .tr("Chọn gói (.mrpack / .zip)...", "Choose (.mrpack / .zip)..."),
                            )
                            .clicked()
                            {
                                ask_pick_pack = true;
                            }
                            let fname = app
                                .servers
                                .new_draft
                                .modpack_archive_path
                                .as_ref()
                                .and_then(|p| p.file_name())
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| {
                                    app.i18n
                                        .tr("Chưa chọn tệp", "No archive selected")
                                        .to_string()
                                });
                            ui.label(RichText::new(fname).size(11.8).weak());
                        });

                        if let Some(ref info) = app.servers.new_draft.modpack_inspection {
                            ui.add_space(4.0);
                            let fmt_lbl = match info.format {
                                ServerModpackFormat::ModrinthMrpack => "Modrinth",
                                ServerModpackFormat::CurseForgeManifest => "CurseForge",
                                ServerModpackFormat::RawServerZip => "Server Pack",
                            };
                            ui.horizontal_wrapped(|ui| {
                                badge(ui, fmt_lbl, ACCENT_CYAN);
                                badge(ui, &format!("MC {}", info.game_version), ACCENT_EMERALD);
                                badge(ui, info.engine.display_name(), ACCENT_VIOLET);
                                badge(
                                    ui,
                                    &format!(
                                        "{} {}",
                                        info.file_count,
                                        app.i18n.tr("tệp", "files")
                                    ),
                                    ACCENT_BLUE,
                                );
                            });
                        }

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(app.i18n.tr("Cổng:", "Port:")).strong());
                            ui.add(
                                egui::DragValue::new(&mut app.servers.new_draft.port)
                                    .range(1024..=65535),
                            );
                        });
                    }
                    NewServerSourceMode::ExistingProfile => {
                        let profiles_snap: Vec<(String, String, String, LoaderType, u32)> = app
                            .home
                            .profiles
                            .iter()
                            .map(|p| {
                                (
                                    p.id.clone(),
                                    p.name.clone(),
                                    p.game_version.clone(),
                                    p.loader,
                                    p.ram_mb,
                                )
                            })
                            .collect();
                        if profiles_snap.is_empty() {
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Chưa có Profile nào để sao chép sang máy chủ.",
                                    "No installed profiles available yet.",
                                ))
                                .color(ACCENT_AMBER),
                            );
                        } else {
                            if app.servers.new_draft.source_profile_id.is_none() {
                                app.servers.new_draft.source_profile_id =
                                    Some(profiles_snap[0].0.clone());
                            }
                            let selected_label = app
                                .servers
                                .new_draft
                                .source_profile_id
                                .as_deref()
                                .and_then(|id| profiles_snap.iter().find(|p| p.0 == id))
                                .map(|p| format!("{} (MC {} • {})", p.1, p.2, p.3.display_name()))
                                .unwrap_or_default();

                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(app.i18n.tr("Profile nguồn:", "Source Profile:"))
                                        .strong(),
                                );
                                let prof_combo_w =
                                    260.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                                egui::ComboBox::from_id_salt("new_server_from_profile_combo")
                                    .width(prof_combo_w)
                                    .truncate()
                                    .selected_text(selected_label)
                                    .show_ui(ui, |ui| {
                                        for (pid, pname, pmc, ploader, pram) in &profiles_snap {
                                            let row_txt = format!(
                                                "{pname} (MC {pmc} • {})",
                                                ploader.display_name()
                                            );
                                            if ui
                                                .selectable_label(
                                                    app.servers
                                                        .new_draft
                                                        .source_profile_id
                                                        .as_deref()
                                                        == Some(pid.as_str()),
                                                    row_txt,
                                                )
                                                .clicked()
                                            {
                                                app.servers.new_draft.source_profile_id =
                                                    Some(pid.clone());
                                                app.servers.new_draft.name =
                                                    format!("{pname} Server");
                                                app.servers.new_draft.ram_max_mb =
                                                    (*pram).clamp(2048, 16384);
                                            }
                                        }
                                    });
                                ui.label(app.i18n.tr("Cổng:", "Port:"));
                                ui.add(
                                    egui::DragValue::new(&mut app.servers.new_draft.port)
                                        .range(1024..=65535),
                                );
                            });
                        }
                    }
                }

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(app.i18n.tr("RAM tối thiểu:", "Min RAM:"));
                    ui.add(
                        egui::Slider::new(&mut app.servers.new_draft.ram_min_mb, 512..=8192)
                            .step_by(256.0)
                            .suffix(" MB"),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(app.i18n.tr("RAM tối đa:", "Max RAM:"));
                    ui.add(
                        egui::Slider::new(&mut app.servers.new_draft.ram_max_mb, 1024..=16384)
                            .step_by(512.0)
                            .suffix(" MB"),
                    );
                });

                let srv_mc_ver: Option<String> = match app.servers.new_draft.source_mode {
                    NewServerSourceMode::StandardEngine => {
                        Some(app.servers.new_draft.game_version.clone())
                    }
                    NewServerSourceMode::CloudModpack => app
                        .servers
                        .new_draft
                        .cloud_selected
                        .as_ref()
                        .and_then(|(_, vers)| {
                            vers.get(app.servers.new_draft.cloud_chosen_ver_idx)
                                .or_else(|| vers.first())
                        })
                        .and_then(|v| v.game_versions.first().cloned()),
                    NewServerSourceMode::ModpackArchive => app
                        .servers
                        .new_draft
                        .modpack_inspection
                        .as_ref()
                        .map(|i| i.game_version.clone()),
                    NewServerSourceMode::ExistingProfile => app
                        .servers
                        .new_draft
                        .source_profile_id
                        .as_deref()
                        .and_then(|pid| app.home.profiles.iter().find(|p| p.id == pid))
                        .map(|p| p.game_version.clone()),
                };
                if java_runtime_mode_selector(
                    ui,
                    &app.i18n,
                    &mut app.settings.java_runtime_mode,
                    srv_mc_ver.as_deref(),
                ) {
                    app.click_sfx();
                    app.persist_settings();
                }

                if app.servers.new_draft.source_mode == NewServerSourceMode::StandardEngine {
                    let auto_jar_lbl = app
                        .i18n
                        .tr(
                            "Tự động tải lõi máy chủ và chuẩn bị môi trường ngay sau khi tạo",
                            "Automatically download server core and initialize directory on creation",
                        )
                        .to_string();
                    crate::views::wrapped_checkbox(
                        ui,
                        &mut app.servers.new_draft.install_immediately,
                        auto_jar_lbl,
                    );
                }

                ui.horizontal_wrapped(|ui| {
                    let eula_lbl = app
                        .i18n
                        .tr(
                            "Tôi đã đọc và đồng ý với điều khoản Minecraft EULA",
                            "I have read and agree to the Minecraft EULA",
                        )
                        .to_string();
                    crate::views::wrapped_checkbox(
                        ui,
                        &mut app.servers.new_draft.accept_eula,
                        eula_lbl,
                    );
                    ui.hyperlink_to("(minecraft.net/eula)", "https://www.minecraft.net/eula");
                });

                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(app.i18n.tr("Thư mục máy chủ:", "Server Folder:")).strong(),
                    );
                    let dir_label = app
                        .servers
                        .new_draft
                        .custom_dir
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| {
                            app.i18n
                                .tr("Mặc định (servers/)", "Default (servers/)")
                                .to_string()
                        });
                    ui.label(RichText::new(dir_label).size(11.5).weak());
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Folder,
                        app.i18n.tr("Chọn thư mục...", "Choose Folder..."),
                    )
                    .clicked()
                    {
                        if let Some(folder) = crate::dialogs::pick_folder_sync() {
                            app.servers.new_draft.custom_dir = Some(folder);
                        }
                    }
                    if app.servers.new_draft.custom_dir.is_some()
                        && secondary_icon_button(
                            ui,
                            VectorIcon::Refresh,
                            app.i18n.tr("Mặc định", "Reset"),
                        )
                        .clicked()
                    {
                        app.servers.new_draft.custom_dir = None;
                    }
                });
            });

            if let Some(ref err) = app.servers.new_draft.create_error {
                ui.add_space(6.0);
                ui.label(RichText::new(err).size(12.0).color(ACCENT_RED));
            }

            if creating {
                ui.add_space(8.0);
                if let Some(prog) = create_task_prog {
                    let frac = (prog.percent / 100.0).clamp(0.0, 1.0);
                    paint_glowing_progress_bar(ui, frac, ui.available_width(), 16.0);
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        badge(ui, &prog.phase, ACCENT_CYAN);
                        ui.label(RichText::new(&prog.message).size(11.6));
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(app.i18n.tr(
                            "Đang thiết lập máy chủ Modpack & cài lõi chạy...",
                            "Setting up Modpack server & installing runtime...",
                        ));
                    });
                }
            }
            });

            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if creating {
                    if danger_icon_button(ui, VectorIcon::Stop, app.i18n.t("common.cancel"))
                        .clicked()
                    {
                        ask_cancel_create = true;
                    }
                } else {
                    if primary_icon_button(ui, VectorIcon::Plus, app.i18n.t("server.create"))
                        .clicked()
                    {
                        ask_create = true;
                    }
                    if secondary_button(ui, app.i18n.t("common.cancel")).clicked() {
                        app.show_new_server_modal = false;
                    }
                }
            });

            if let Some((page, append)) = ask_cloud_search {
                app.click_sfx();
                app.search_new_server_cloud_modpacks(page, append);
            } else if let Some(summary) = ask_select_cloud {
                app.click_sfx();
                app.select_new_server_cloud_modpack(summary);
            } else if ask_pick_pack {
                app.click_sfx();
                app.pick_new_server_modpack_archive();
            } else if ask_create {
                app.click_sfx();
                app.create_server_from_draft();
            } else if ask_cancel_create {
                app.click_sfx();
                app.cancel_new_server_creation();
            }
        });
    if !app.servers.new_draft.creating {
        app.show_new_server_modal &= open;
    }
}

fn render_java_manager_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let mut open = app.show_java_modal;
    let title = app.i18n.tr("Quản lý Java", "Java Manager").to_string();
    let safe_rect = crate::views::safe_content_rect(ctx);
    let init_rect = centered_window_rect(ctx, vec2(710.0, 560.0));
    egui::Window::new(title)
        .id(egui::Id::unique("thk_modal_java_manager"))
        .default_rect(init_rect)
        .min_size(vec2(300.0, 300.0).min(init_rect.size()))
        .collapsible(false)
        .resizable(true)
        .constrain_to(safe_rect)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                paint_icon_3d(ui, 36.0, Icon3dKind::JavaReactor, true);
                ui.heading(
                    RichText::new(app.i18n.tr(
                        "Các bản Java hiện có trên hệ thống",
                        "Detected Java Installations",
                    ))
                    .size(16.0)
                    .strong(),
                );
                if secondary_icon_button(
                    ui,
                    VectorIcon::Refresh,
                    app.i18n.tr("Quét lại", "Rescan"),
                )
                .clicked()
                {
                    app.rescan_java_runtimes();
                }
            });
            ui.add_space(6.0);
            if java_runtime_mode_selector(ui, &app.i18n, &mut app.settings.java_runtime_mode, None)
            {
                app.click_sfx();
                app.persist_settings();
            }
            ui.add_space(6.0);

            let rem_h = ui.available_height().max(240.0);
            let inst_h = (rem_h * 0.36).max(105.0);
            let installed = app.java_modal.installed.clone();
            ScrollArea::vertical()
                .id_salt("java_installed_scroll")
                .max_height(inst_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if installed.is_empty() {
                        let empty_msg =
                            if app.settings.java_runtime_mode == JavaRuntimeMode::OptimalGraalVm {
                                app.i18n.tr(
                                    "Chưa phát hiện Java cài sẵn. Launcher sẽ tự chọn và tải GraalVM / Java tương thích khi vào game, hoặc bạn có thể cài thủ công bên dưới.",
                                    "No system Java detected yet. The launcher will automatically provision optimal GraalVM / Java on game launch, or you can install a distribution below.",
                                )
                            } else {
                                app.i18n.tr(
                                    "Chưa phát hiện Java cài sẵn. Launcher vẫn sẽ tự động tải Mojang JRE khi vào game hoặc bạn có thể cài thủ công bên dưới.",
                                    "No system Java detected yet. The launcher will automatically provision Mojang JRE on game launch, or you can install a distribution below.",
                                )
                            };
                        ui.label(RichText::new(empty_msg).weak());
                    }
                    for (idx, j) in installed.into_iter().enumerate() {
                        gaming_card_3d(
                            ui,
                            ("java_inst_row", idx),
                            j.is_managed,
                            ACCENT_ORANGE,
                            Margin::symmetric(10, 6),
                            |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    badge(ui, &format!("Java {}", j.major_version), ACCENT_EMERALD);
                                    ui.label(RichText::new(&j.vendor).strong());
                                    ui.label(RichText::new(&j.full_version).size(12.0).weak());
                                    if j.is_managed {
                                        badge(
                                            ui,
                                            app.i18n.tr("Launcher quản lý", "Managed"),
                                            ACCENT_BLUE,
                                        );
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let p_str = j.path.to_string_lossy().to_string();
                                            if secondary_icon_button(
                                                ui,
                                                VectorIcon::Copy,
                                                app.i18n.tr("Chép đường dẫn", "Copy Path"),
                                            )
                                            .clicked()
                                            {
                                                app.copy_text_with_toast(ui.ctx(), p_str);
                                            }
                                        },
                                    );
                                });
                            },
                        );
                        ui.add_space(4.0);
                    }
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                let distro_header = if cfg!(target_os = "android") {
                    app.i18n.tr(
                        "Tải về bản phân phối Java (8 / 17 / 21 / 25):",
                        "Download Official Java Distributions (8 / 17 / 21 / 25):",
                    )
                } else {
                    app.i18n.tr(
                        "Tải về bản phân phối Java (8 / 11 / 17 / 21 / 25):",
                        "Download Official Java Distributions (8 / 11 / 17 / 21 / 25):",
                    )
                };
                ui.label(RichText::new(distro_header).strong());
                for &d in JavaDistroKind::platform_distros() {
                    if ui
                        .selectable_label(app.java_modal.selected_distro == d, d.display_name())
                        .clicked()
                    {
                        app.load_java_distro_packages(d);
                    }
                }
            });

            ui.add_space(6.0);
            let pkgs_h = ui.available_height().max(120.0);
            let pkgs = app.java_modal.packages.clone();
            ScrollArea::vertical()
                .id_salt("java_distro_pkgs_scroll")
                .max_height(pkgs_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if pkgs.is_empty() {
                        ui.label(
                            RichText::new(app.i18n.tr(
                                "Đang tải hoặc chưa có gói phù hợp cho kiến trúc CPU này.",
                                "Loading packages or none available for this CPU architecture.",
                            ))
                            .weak(),
                        );
                    }
                    for (idx, pkg) in pkgs.into_iter().enumerate() {
                        gaming_card_3d(
                            ui,
                            ("java_pkg_row", idx),
                            false,
                            ACCENT_CYAN,
                            Margin::symmetric(10, 6),
                            |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    badge(
                                        ui,
                                        &format!("JDK/JRE {}", pkg.major_version),
                                        ACCENT_AMBER,
                                    );
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(&pkg.release_name).strong());
                                        let sz_txt = pkg.size_bytes.map(format_bytes).unwrap_or_else(
                                            || {
                                                app.i18n
                                                    .tr("Gói chính thức", "Official Archive")
                                                    .to_string()
                                            },
                                        );
                                        ui.label(
                                            RichText::new(format!("{} • {sz_txt}", pkg.file_name))
                                                .size(11.5)
                                                .weak(),
                                        );
                                    });
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if primary_icon_button(
                                                ui,
                                                VectorIcon::Download,
                                                app.i18n.tr("Cài đặt", "Install"),
                                            )
                                            .clicked()
                                            {
                                                app.install_java_package(pkg);
                                            }
                                        },
                                    );
                                });
                            },
                        );
                        ui.add_space(4.0);
                    }
                });
        });
    app.show_java_modal &= open;
}

fn render_lan_share_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let mut open = app.show_lan_modal;
    let title = app
        .i18n
        .tr(
            "Chia sẻ Thế giới LAN & Kết nối từ xa",
            "LAN World Sharing & Remote Tunnel",
        )
        .to_string();
    let safe_rect = crate::views::safe_content_rect(ctx);
    let init_rect = centered_window_rect(ctx, vec2(600.0, 620.0));
    egui::Window::new(title)
        .id(egui::Id::unique("thk_modal_lan_share"))
        .default_rect(init_rect)
        .min_size(vec2(300.0, 280.0).min(init_rect.size()))
        .collapsible(false)
        .resizable(true)
        .constrain_to(safe_rect)
        .open(&mut open)
        .show(ctx, |ui| {
            let scroll_w = ui.available_width();
            ui.set_width(scroll_w);
            ScrollArea::vertical()
                .id_salt("lan_share_modal_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(scroll_w);
                    let mut ask_refresh_net = false;
                    let mut ask_sync_template = false;
                    let mut ask_toggle_direct = false;
                    let mut copy_target: Option<String> = None;

                    ui.horizontal(|ui| {
                        paint_icon_3d(ui, 38.0, Icon3dKind::LanRadar, true);
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(app.i18n.tr(
                                        "Mở Port Theo Mạng Đang Kết Nối & Chơi LAN Online",
                                        "Connected Network Port Host & Online LAN",
                                    ))
                                    .size(15.5)
                                    .strong(),
                                );
                                if app.servers.probing_network {
                                    ui.spinner();
                                } else if secondary_icon_button(
                                    ui,
                                    VectorIcon::Refresh,
                                    app.i18n.tr("Làm mới mạng", "Refresh Network"),
                                )
                                .clicked()
                                {
                                    ask_refresh_net = true;
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                if let Some(ref ip) = app.lan_modal.primary_lan_ip {
                                    ui.label(
                                        RichText::new(app.i18n.tr("Mạng nội bộ:", "Local LAN:"))
                                            .size(12.0)
                                            .weak(),
                                    );
                                    badge(ui, ip, ACCENT_BLUE);
                                }
                                if let Some(v6) = app.lan_modal.network_snapshot.global_ipv6 {
                                    badge(ui, &format!("IPv6 [{v6}]"), ACCENT_EMERALD);
                                }
                            });
                        });
                    });

                    ui.add_space(6.0);
                    gaming_card_3d(
                        ui,
                        "lan_scanner_card",
                        app.lan_modal.scanner_active,
                        ACCENT_CYAN,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "1. Tự động phát hiện thế giới 'Open to LAN' trên máy này",
                                    "1. Detect Local 'Open to LAN' Worlds",
                                ))
                                .strong(),
                            );
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Chỉ chấp nhận gói phát sóng xuất phát từ chính card mạng của máy bạn (chống giả mạo gói tin từ máy khác trong LAN).",
                                    "Only accepts multicast packets originating from this machine's NICs (blocks foreign LAN spoofing).",
                                ))
                                .size(11.5)
                                .weak(),
                            );
                            ui.add_space(6.0);
                            let auto_direct_lbl = app
                                .i18n
                                .tr(
                                    "Tự động mở Port mạng hiện tại ngay khi phát hiện thế giới LAN",
                                    "Automatically open port on current network when a local LAN world is detected",
                                )
                                .to_string();
                            if crate::views::wrapped_checkbox(
                                ui,
                                &mut app.lan_modal.auto_open_direct_host,
                                auto_direct_lbl,
                            )
                            .changed()
                            {
                                ask_sync_template = true;
                            }
                            let auto_tun_lbl = app
                                .i18n
                                .tr(
                                    "Tự động mở đường hầm dự phòng bore.pub khi phát hiện thế giới LAN",
                                    "Automatically open fallback bore.pub tunnel when a local LAN world is detected",
                                )
                                .to_string();
                            crate::views::wrapped_checkbox(
                                ui,
                                &mut app.lan_modal.auto_open_tunnel,
                                auto_tun_lbl,
                            );

                            ui.horizontal_wrapped(|ui| {
                                if app.lan_modal.scanner_active {
                                    if danger_icon_button(
                                        ui,
                                        VectorIcon::Stop,
                                        app.i18n.tr("Dừng quét LAN", "Stop LAN Radar"),
                                    )
                                    .clicked()
                                    {
                                        app.toggle_lan_scanner();
                                    }
                                    ui.spinner();
                                    ui.label(app.i18n.tr(
                                        "Đang lắng nghe 'Open to LAN' trong Minecraft...",
                                        "Listening for 'Open to LAN' in Minecraft...",
                                    ));
                                } else if primary_icon_button(
                                    ui,
                                    VectorIcon::Globe,
                                    app.i18n.tr("Bắt đầu quét LAN", "Start LAN Radar"),
                                )
                                .clicked()
                                {
                                    app.toggle_lan_scanner();
                                }
                            });

                            if let Some(ref world) = app.lan_modal.detected_world {
                                ui.add_space(4.0);
                                let det_lbl = if app.settings.language == Language::Vi {
                                    format!(
                                        "Đã phát hiện: {} ({}:{})",
                                        world.motd, world.sender_ip, world.port
                                    )
                                } else {
                                    format!(
                                        "Detected: {} ({}:{})",
                                        world.motd, world.sender_ip, world.port
                                    )
                                };
                                badge(ui, &det_lbl, ACCENT_EMERALD);
                            }
                            if app.lan_modal.ignored_foreign_packets > 0 {
                                let blk_lbl = if app.settings.language == Language::Vi {
                                    format!(
                                        "Đã chặn {} gói quảng bá LAN ngoại lai không thuộc máy này.",
                                        app.lan_modal.ignored_foreign_packets
                                    )
                                } else {
                                    format!(
                                        "Blocked {} foreign LAN announcement packet(s).",
                                        app.lan_modal.ignored_foreign_packets
                                    )
                                };
                                ui.label(RichText::new(blk_lbl).size(11.0).color(ACCENT_AMBER));
                            }
                        },
                    );

                    ui.add_space(8.0);

                    let direct_running =
                        matches!(app.lan_modal.direct_host_status, DirectHostStatus::Running(_));
                    let direct_starting = matches!(
                        app.lan_modal.direct_host_status,
                        DirectHostStatus::Starting { .. }
                    );

                    gaming_card_3d(
                        ui,
                        "lan_direct_host_card",
                        direct_running,
                        ACCENT_EMERALD,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "2. Mở Port Theo Mạng Đang Kết Nối (Chơi LAN & Online)",
                                    "2. Open Port on Connected Network (LAN & Online)",
                                ))
                                .strong(),
                            );
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Mở cổng cố định trực tiếp trên Wi-Fi / LAN / VPN đang kết nối kèm Online IPv6 & mở cổng Modem tự động (không vòng qua máy chủ trung gian nước ngoài).",
                                    "Host a fixed port directly on your connected Wi-Fi / LAN / VPN with Direct Online IPv6 and automatic router port mapping (no overseas relay).",
                                ))
                                .size(11.5)
                                .weak(),
                            );
                            ui.add_space(6.0);

                            ui.add_enabled_ui(!direct_running && !direct_starting, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    if network_interface_selector_ui(
                                        ui,
                                        "lan_direct_bind_nic",
                                        &app.i18n,
                                        &app.lan_modal.network_snapshot,
                                        &mut app.lan_modal.direct_bind_ip,
                                    ) {
                                        ask_sync_template = true;
                                    }
                                });
                                ui.horizontal_wrapped(|ui| {
                                    let src_lbl = ui.label(
                                        RichText::new(
                                            app.i18n.tr("Cổng nội bộ:", "Local Port:"),
                                        )
                                        .strong(),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(
                                            &mut app.lan_modal.manual_port_input,
                                        )
                                        .hint_text("25565")
                                        .desired_width(78.0),
                                    )
                                    .labelled_by(src_lbl.id);

                                    let dst_lbl = ui.label(
                                        RichText::new(
                                            app.i18n.tr("Cổng mở cố định:", "Host Port:"),
                                        )
                                        .strong(),
                                    );
                                    if ui
                                        .add(
                                            egui::TextEdit::singleline(
                                                &mut app.lan_modal.direct_host_port_input,
                                            )
                                            .hint_text("25565")
                                            .desired_width(78.0),
                                        )
                                        .labelled_by(dst_lbl.id)
                                        .changed()
                                    {
                                        ask_sync_template = true;
                                    }
                                });
                                ui.horizontal_wrapped(|ui| {
                                    if crate::views::wrapped_checkbox(
                                        ui,
                                        &mut app.lan_modal.direct_announce_lan,
                                        app.i18n.tr(
                                            "Hiện tự động trong Chơi mạng LAN",
                                            "Auto-show in Multiplayer LAN",
                                        ),
                                    )
                                    .changed()
                                    {
                                        ask_sync_template = true;
                                    }
                                    if crate::views::wrapped_checkbox(
                                        ui,
                                        &mut app.lan_modal.direct_try_upnp,
                                        app.i18n.tr(
                                            "Mở cổng tự động trên Modem",
                                            "Auto-open router port",
                                        ),
                                    )
                                    .changed()
                                    {
                                        ask_sync_template = true;
                                    }
                                });
                            });

                            ui.add_space(6.0);
                            let direct_status_clone = app.lan_modal.direct_host_status.clone();
                            match &direct_status_clone {
                                DirectHostStatus::Idle | DirectHostStatus::Stopped => {
                                    if primary_icon_button(
                                        ui,
                                        VectorIcon::Globe,
                                        app.i18n.tr(
                                            "Mở Port Mạng Hiện Tại",
                                            "Open Port on Current Network",
                                        ),
                                    )
                                    .clicked()
                                    {
                                        ask_toggle_direct = true;
                                    }
                                }
                                DirectHostStatus::Starting { host_port, .. } => {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spinner();
                                        ui.label(if app.settings.language == Language::Vi {
                                            format!(
                                                "Đang mở cổng {host_port} trên mạng đang kết nối..."
                                            )
                                        } else {
                                            format!(
                                                "Opening port {host_port} on connected network..."
                                            )
                                        });
                                        if danger_icon_button(
                                            ui,
                                            VectorIcon::Stop,
                                            app.i18n.t("common.cancel"),
                                        )
                                        .clicked()
                                        {
                                            ask_toggle_direct = true;
                                        }
                                    });
                                }
                                DirectHostStatus::Running(info) => {
                                    if let Some(ep) = render_direct_host_endpoints_ui(
                                        ui, &app.i18n, info, None, None,
                                    ) {
                                        copy_target = Some(ep);
                                    }
                                    ui.add_space(4.0);
                                    if danger_icon_button(
                                        ui,
                                        VectorIcon::Stop,
                                        app.i18n.tr("Đóng Port Mạng", "Close Hosted Port"),
                                    )
                                    .clicked()
                                    {
                                        ask_toggle_direct = true;
                                    }
                                }
                                DirectHostStatus::Error(err) => {
                                    ui.label(RichText::new(err).color(ACCENT_RED));
                                    if primary_icon_button(
                                        ui,
                                        VectorIcon::Refresh,
                                        app.i18n.tr(
                                            "Mở Port Mạng Hiện Tại",
                                            "Open Port on Current Network",
                                        ),
                                    )
                                    .clicked()
                                    {
                                        ask_toggle_direct = true;
                                    }
                                }
                            }
                        },
                    );

                    ui.add_space(8.0);

                    let tun_running =
                        matches!(app.lan_modal.tunnel_status, TunnelStatus::Running { .. });
                    let tun_connecting =
                        matches!(app.lan_modal.tunnel_status, TunnelStatus::Connecting { .. });
                    let local_lbl = format!("127.0.0.1:{}", app.lan_modal.manual_port_input.trim());
                    let remote_lbl = match &app.lan_modal.tunnel_status {
                        st @ TunnelStatus::Running { .. } => st
                            .public_endpoint()
                            .unwrap_or_else(|| "bore.pub".to_string()),
                        TunnelStatus::Connecting { .. } => "bore.pub (...)".to_string(),
                        _ => "bore.pub (idle)".to_string(),
                    };

                    gaming_card_3d(
                        ui,
                        "lan_manual_bore_card",
                        tun_running,
                        ACCENT_VIOLET,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "3. Đường hầm Công khai Dự phòng (bore.pub)",
                                    "3. Fallback Public Bore Tunnel (bore.pub)",
                                ))
                                .strong(),
                            );
                            ui.add_space(4.0);
                            paint_3d_tunnel_conduit(
                                ui,
                                tun_running,
                                tun_connecting,
                                &local_lbl,
                                &remote_lbl,
                            );
                            ui.add_space(6.0);

                            ui.horizontal_wrapped(|ui| {
                                let bore_port_lbl =
                                    ui.label(app.i18n.tr("Cổng nội bộ:", "Local Port:"));
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut app.lan_modal.manual_port_input,
                                    )
                                    .hint_text("25565")
                                    .desired_width(90.0),
                                )
                                .labelled_by(bore_port_lbl.id);
                                if primary_icon_button(
                                    ui,
                                    VectorIcon::Globe,
                                    app.i18n.tr("Mở bore.pub", "Open bore.pub"),
                                )
                                .clicked()
                                {
                                    app.start_manual_bore_tunnel();
                                }
                                if (tun_running || tun_connecting)
                                    && danger_icon_button(
                                        ui,
                                        VectorIcon::Stop,
                                        app.i18n.tr("Đóng cổng", "Close Tunnel"),
                                    )
                                    .clicked()
                                {
                                    app.lan_coordinator.stop_tunnel();
                                    app.lan_modal.tunnel_status = TunnelStatus::Stopped;
                                }
                            });

                            ui.add_space(6.0);
                            match &app.lan_modal.tunnel_status {
                                TunnelStatus::Idle | TunnelStatus::Stopped => {
                                    ui.label(
                                        RichText::new(app.i18n.tr(
                                            "Trạng thái: Chưa mở đường hầm.",
                                            "Status: Tunnel idle.",
                                        ))
                                        .weak(),
                                    );
                                }
                                TunnelStatus::Connecting { local_port } => {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        let txt = if app.settings.language == Language::Vi {
                                            format!("Đang kết nối cổng {local_port} tới bore.pub...")
                                        } else {
                                            format!(
                                                "Connecting local port {local_port} to bore.pub..."
                                            )
                                        };
                                        ui.label(txt);
                                    });
                                }
                                st @ TunnelStatus::Running { local_port, .. } => {
                                    let endpoint = st
                                        .public_endpoint()
                                        .unwrap_or_else(|| "bore.pub".to_string());
                                    let local_port = *local_port;
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(app.i18n.tr(
                                            "Gửi địa chỉ này cho bạn bè:",
                                            "Share this address with friends:",
                                        ));
                                        badge(ui, &endpoint, ACCENT_EMERALD);
                                        if primary_icon_button(
                                            ui,
                                            VectorIcon::Copy,
                                            app.i18n.tr("Sao chép", "Copy"),
                                        )
                                        .clicked()
                                        {
                                            copy_target = Some(endpoint);
                                        }
                                    });
                                    ui.label(
                                        RichText::new(format!("127.0.0.1:{local_port}"))
                                            .size(11.0)
                                            .weak(),
                                    );
                                }
                                TunnelStatus::Error(err) => {
                                    ui.label(RichText::new(err).color(ACCENT_RED));
                                }
                            }
                        },
                    );

                    if ask_refresh_net {
                        app.click_sfx();
                        app.refresh_connected_network_snapshot();
                    }
                    if ask_sync_template && app.lan_modal.scanner_active {
                        app.sync_lan_auto_host_template();
                    }
                    if ask_toggle_direct {
                        app.click_sfx();
                        app.toggle_lan_direct_host();
                    }
                    if let Some(txt) = copy_target {
                        app.copy_text_with_toast(ui.ctx(), txt);
                    }
                });
        });
    app.show_lan_modal &= open;
}

fn render_crash_analyzer_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let Some((profile_id, report)) = app.crash_modal.clone() else {
        return;
    };

    let mut keep_open = true;
    let mut close_clicked = false;
    let title = app
        .i18n
        .tr(
            "Phân Tích Sự Cố & Tự Động Khắc Phục Crash",
            "Crash Diagnostics & 1-Click Auto-Fix",
        )
        .to_string();
    let safe_rect = crate::views::safe_content_rect(ctx);
    let init_rect = centered_window_rect(ctx, vec2(650.0, 480.0));
    egui::Window::new(title)
        .id(egui::Id::unique("thk_modal_crash_analyzer"))
        .default_rect(init_rect)
        .min_size(vec2(300.0, 260.0).min(init_rect.size()))
        .collapsible(false)
        .resizable(true)
        .constrain_to(safe_rect)
        .open(&mut keep_open)
        .show(ctx, |ui| {
            let scroll_w = ui.available_width();
            ui.set_width(scroll_w);
            let crash_body_h = (ui.available_height() - 48.0).max(140.0);
            ScrollArea::vertical()
                .id_salt("crash_analyzer_body_scroll")
                .max_height(crash_body_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(scroll_w);
                    ui.horizontal_wrapped(|ui| {
                        badge(ui, &format!("Exit Code {}", report.exit_code), ACCENT_RED);
                        ui.label(
                            RichText::new(&report.summary_message)
                                .size(15.0)
                                .strong()
                                .color(readable_accent_ink(ACCENT_RED, ui.visuals().dark_mode)),
                        );
                    });

                    if let Some(ref root_line) = report.root_cause_line {
                        ui.add_space(6.0);
                        let err_bg = if ui.visuals().dark_mode {
                            Color32::from_rgb(28, 14, 20)
                        } else {
                            Color32::from_rgb(254, 242, 242)
                        };
                        Frame::group(ui.style())
                            .fill(err_bg)
                            .inner_margin(Margin::same(8))
                            .show(ui, |ui| {
                                ui.label(RichText::new(root_line).monospace().size(11.5));
                            });
                    }

                    if !report.solutions.is_empty() {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(
                                app.i18n
                                    .tr("Hướng xử lý đề xuất:", "Recommended Solutions:"),
                            )
                            .strong(),
                        );
                        for sol in &report.solutions {
                            ui.label(format!("• {sol}"));
                        }
                    }

                    if !report.missing_mods.is_empty() {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(app.i18n.tr(
                                "Khắc phục tự động 1 chạm từ Modrinth:",
                                "1-Click Automated Fix via Modrinth:",
                            ))
                            .strong()
                            .color(ACCENT_EMERALD),
                        );
                        for advice in &report.missing_mods {
                            gaming_card_3d(
                                ui,
                                ("crash_fix_mod", &advice.mod_id),
                                true,
                                ACCENT_EMERALD,
                                Margin::symmetric(10, 7),
                                |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            let any_lbl = app.i18n.tr("bất kỳ", "any");
                                            let req_ver = advice
                                                .required_version
                                                .as_deref()
                                                .unwrap_or(any_lbl);
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} ({}) — {req_ver}",
                                                    advice.display_name, advice.mod_id
                                                ))
                                                .strong(),
                                            );
                                            if !advice.required_by.is_empty() {
                                                let req_by = app.i18n.tr("Cần bởi", "Required by");
                                                ui.label(
                                                    RichText::new(format!(
                                                        "{req_by}: {}",
                                                        advice.required_by.join(", ")
                                                    ))
                                                    .size(11.5)
                                                    .weak(),
                                                );
                                            }
                                        });

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| match advice.action {
                                                CrashFixAction::InstallMod
                                                | CrashFixAction::UpdateMod => {
                                                    if primary_icon_button(
                                                        ui,
                                                        VectorIcon::Wrench,
                                                        app.i18n.tr(
                                                            "Tự động tải & sửa",
                                                            "Auto-Install Fix",
                                                        ),
                                                    )
                                                    .clicked()
                                                    {
                                                        app.auto_fix_crash_mod(
                                                            &profile_id,
                                                            &advice.mod_id,
                                                        );
                                                    }
                                                }
                                                CrashFixAction::UpgradeLoader => {
                                                    if primary_icon_button(
                                                        ui,
                                                        VectorIcon::Wrench,
                                                        app.i18n.tr(
                                                            "Nâng cấp Mod Loader",
                                                            "Upgrade Mod Loader",
                                                        ),
                                                    )
                                                    .clicked()
                                                    {
                                                        app.auto_upgrade_profile_loader(
                                                            &profile_id,
                                                            advice.required_version.as_deref(),
                                                        );
                                                    }
                                                }
                                                CrashFixAction::UpgradeJava => {
                                                    if primary_icon_button(
                                                        ui,
                                                        VectorIcon::Sliders,
                                                        app.i18n.tr(
                                                            "Mở Quản lý Java",
                                                            "Open Java Manager",
                                                        ),
                                                    )
                                                    .clicked()
                                                    {
                                                        app.open_java_manager();
                                                    }
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                            ui.add_space(4.0);
                        }
                    }

                    if !report.relevant_lines.is_empty() {
                        ui.add_space(6.0);
                        ui.collapsing(
                            app.i18n.tr(
                                "Xem chi tiết đoạn nhật ký lỗi (Stacktrace)",
                                "Inspect Relevant Stacktrace Lines",
                            ),
                            |ui| {
                                ScrollArea::vertical()
                                    .id_salt("crash_relevant_lines_scroll")
                                    .max_height(200.0)
                                    .show(ui, |ui| {
                                        for line in &report.relevant_lines {
                                            ui.label(RichText::new(line).monospace().size(11.5));
                                        }
                                    });
                            },
                        );
                    }
                });

            ui.add_space(10.0);
            if danger_button(
                ui,
                app.i18n
                    .tr("Đóng cửa sổ chẩn đoán", "Close Crash Diagnostics"),
            )
            .clicked()
            {
                close_clicked = true;
            }
        });

    if !keep_open || close_clicked {
        app.crash_modal = None;
    }
}

fn render_settings_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let mut open = app.show_settings_modal;
    let title = app
        .i18n
        .tr("Cài đặt Hệ thống & Cập nhật", "System Settings & Updates")
        .to_string();
    let safe_rect = crate::views::safe_content_rect(ctx);
    let init_rect = centered_window_rect(ctx, vec2(650.0, 580.0));
    egui::Window::new(title)
        .id(egui::Id::unique("thk_modal_settings"))
        .default_rect(init_rect)
        .min_size(vec2(300.0, 290.0).min(init_rect.size()))
        .collapsible(false)
        .resizable(true)
        .constrain_to(safe_rect)
        .open(&mut open)
        .show(ctx, |ui| {
            let modal_w = ui.available_width();
            ui.set_width(modal_w);
            let mut changed = false;

            ui.horizontal(|ui| {
                paint_icon_3d(ui, 36.0, Icon3dKind::SettingsGyro, true);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Trung tâm Tuỳ chỉnh Hệ thống",
                            "System Control Center",
                        ))
                        .size(15.5)
                        .strong(),
                    );
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Giao diện, Khởi chạy, Lưu trữ & Cập nhật",
                            "Appearance, Launch Behavior, Storage & Updates",
                        ))
                        .size(11.5)
                        .weak(),
                    );
                });
            });
            ui.add_space(6.0);

            let max_settings_h = ui.available_height().max(180.0);
            ScrollArea::vertical()
                .id_salt("settings_modal_scroll")
                .max_height(max_settings_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(modal_w);
                    let narrow_slider_cap = (modal_w - 155.0).clamp(56.0, 150.0);
                    if ui.spacing().slider_width > narrow_slider_cap {
                        ui.spacing_mut().slider_width = narrow_slider_cap;
                    }
                    gaming_card_3d(
                        ui,
                        "settings_ui_card",
                        false,
                        ACCENT_VIOLET,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Giao diện, Màu nhấn & Cửa sổ",
                                    "Appearance, Accent & Window",
                                ))
                                .size(14.5)
                                .strong(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.label(app.i18n.tr("Ngôn ngữ:", "Language:"));
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.language,
                                        Language::Vi,
                                        "Tiếng Việt",
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.language,
                                        Language::En,
                                        "English",
                                    )
                                    .changed();
                            });

                            ui.horizontal_wrapped(|ui| {
                                ui.label(app.i18n.tr("Chủ đề màu:", "Theme:"));
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.theme,
                                        ThemeMode::Dark,
                                        app.i18n.tr("Tối", "Dark"),
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.theme,
                                        ThemeMode::Light,
                                        app.i18n.tr("Sáng", "Light"),
                                    )
                                    .changed();
                            });

                            changed |= font_scale_selector(
                                ui,
                                &app.i18n,
                                &mut app.settings.font_scale,
                                &mut app.font_scale_drag_dirty,
                            );

                            ui.horizontal_wrapped(|ui| {
                                ui.label(app.i18n.tr("Tông màu chủ đạo:", "Accent Color:"));
                                for preset in AccentPreset::ALL {
                                    let (swatch_col, _) = accent_palette(preset);
                                    let swatch_ink = crate::theme::readable_accent_ink(
                                        swatch_col,
                                        ui.visuals().dark_mode,
                                    );
                                    let selected = app.settings.accent_preset == preset;
                                    let label =
                                        RichText::new(format!("● {}", preset.display_name()))
                                            .color(if selected && ui.visuals().dark_mode {
                                                Color32::WHITE
                                            } else {
                                                swatch_ink
                                            })
                                            .strong();
                                    if ui.selectable_label(selected, label).clicked() {
                                        app.settings.accent_preset = preset;
                                        changed = true;
                                    }
                                }
                            });

                            #[cfg(target_os = "android")]
                            ui.horizontal_wrapped(|ui| {
                                ui.label(app.i18n.tr("Xoay màn hình:", "Screen Orientation:"));
                                for mode in thk_core::models::MobileOrientationMode::ALL {
                                    let label = app.mobile_orientation_label(mode);
                                    changed |= ui
                                        .selectable_value(
                                            &mut app.settings.mobile_orientation,
                                            mode,
                                            label,
                                        )
                                        .changed();
                                }
                            });

                            ui.horizontal_wrapped(|ui| {
                                ui.label(app.i18n.tr("Khi đóng cửa sổ:", "On Close:"));
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.close_behavior,
                                        CloseBehavior::Ask,
                                        app.i18n.tr("Luôn hỏi", "Always Ask"),
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.close_behavior,
                                        CloseBehavior::MinimizeToTray,
                                        app.i18n.tr("Thu xuống khay", "Minimize to Tray"),
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.close_behavior,
                                        CloseBehavior::Quit,
                                        app.i18n.t("settings.close_quit"),
                                    )
                                    .changed();
                            });

                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!("{}:", app.i18n.t("settings.custom_bg")));
                                let bg_status = app
                                    .settings
                                    .custom_background_path
                                    .as_ref()
                                    .and_then(|p| p.file_name())
                                    .map(|f| f.to_string_lossy().to_string())
                                    .unwrap_or_else(|| {
                                        app.i18n.tr("Mặc định", "Default").to_string()
                                    });
                                badge(ui, &bg_status, ACCENT_BLUE);
                                if secondary_icon_button(
                                    ui,
                                    VectorIcon::Upload,
                                    app.i18n.tr("Chọn ảnh...", "Choose Image..."),
                                )
                                .clicked()
                                {
                                    app.pick_custom_background();
                                }
                                if app.settings.custom_background_path.is_some()
                                    && danger_icon_button(
                                        ui,
                                        VectorIcon::Trash,
                                        app.i18n.tr("Xoá ảnh nền", "Clear"),
                                    )
                                    .clicked()
                                {
                                    app.clear_custom_background();
                                }
                            });
                        },
                    );

                    ui.add_space(8.0);
                    #[cfg(target_os = "android")]
                    {
                        gaming_card_3d(
                            ui,
                            "settings_android_bg_permissions_card",
                            false,
                            ACCENT_CYAN,
                            Margin::same(12),
                            |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    RichText::new(app.i18n.tr(
                                        "Quyền Chạy Ngầm, Giữ Mạng Khi Tắt Màn Hình & Hiển Thị Nổi",
                                        "Background Network, Screen-Off Keep-Alive & Overlay Permissions",
                                    ))
                                    .size(14.5)
                                    .strong(),
                                );
                                ui.add_space(4.0);

                                let keep_lbl = app
                                    .i18n
                                    .tr(
                                        "Giữ kết nối mạng & tiến trình khi tắt màn hình hoặc chạy ngầm",
                                        "Keep network & active sessions alive when screen turns off",
                                    )
                                    .to_string();
                                changed |= crate::views::wrapped_checkbox(
                                    ui,
                                    &mut app.settings.mobile_keep_alive,
                                    RichText::new(keep_lbl).strong(),
                                )
                                .changed();

                                let overlay_lbl = app
                                    .i18n
                                    .tr(
                                        "Hiện nút nổi mở nhanh HUB UI khi thu nhỏ ra ngoài ứng dụng khác",
                                        "Show floating quick-return button over other apps when minimized",
                                    )
                                    .to_string();
                                changed |= crate::views::wrapped_checkbox(
                                    ui,
                                    &mut app.settings.mobile_floating_overlay,
                                    overlay_lbl,
                                )
                                .changed();

                                ui.add_space(4.0);
                                let granted_txt = app.i18n.tr("Đã cấp", "Granted").to_string();
                                let missing_txt = app.i18n.tr("Chưa cấp", "Not Granted").to_string();

                                ui.horizontal_wrapped(|ui| {
                                    ui.label(app.i18n.tr(
                                        "Chạy nền không giới hạn pin:",
                                        "Unrestricted Battery:",
                                    ));
                                    if app.android_permissions.ignore_battery_optimizations {
                                        badge(ui, &granted_txt, ACCENT_EMERALD);
                                    } else {
                                        badge(ui, &missing_txt, ACCENT_AMBER);
                                        if ui
                                            .button(app.i18n.tr("Cấp quyền ngay", "Grant Now"))
                                            .clicked()
                                        {
                                            app.request_android_ignore_battery();
                                        }
                                    }
                                });

                                ui.horizontal_wrapped(|ui| {
                                    ui.label(app.i18n.tr(
                                        "Hiển thị trên ứng dụng khác:",
                                        "Display Over Other Apps:",
                                    ));
                                    if app.android_permissions.can_draw_overlays {
                                        badge(ui, &granted_txt, ACCENT_EMERALD);
                                    } else {
                                        badge(ui, &missing_txt, ACCENT_AMBER);
                                        if ui
                                            .button(app.i18n.tr("Mở cài đặt quyền", "Grant Overlay"))
                                            .clicked()
                                        {
                                            app.request_android_overlay_permission();
                                        }
                                    }
                                });

                                ui.horizontal_wrapped(|ui| {
                                    ui.label(app.i18n.tr(
                                        "Thông báo dịch vụ chạy ngầm:",
                                        "Background Service Notification:",
                                    ));
                                    if app.android_permissions.notifications_enabled {
                                        badge(ui, &granted_txt, ACCENT_EMERALD);
                                    } else {
                                        badge(ui, &missing_txt, ACCENT_AMBER);
                                        if ui
                                            .button(app.i18n.tr("Cho phép thông báo", "Allow"))
                                            .clicked()
                                        {
                                            app.request_android_notification_permission();
                                        }
                                    }
                                });

                                ui.separator();
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(app.i18n.tr("Trình kết xuất:", "Renderer:"));
                                    for mode in thk_core::models::MobileRendererMode::ALL {
                                        let lbl = app.mobile_renderer_label(mode);
                                        changed |= ui
                                            .selectable_value(
                                                &mut app.settings.mobile_renderer,
                                                mode,
                                                lbl,
                                            )
                                            .changed();
                                    }
                                });
                                ui.horizontal_wrapped(|ui| {
                                    let scale_lbl = ui.label(
                                        app.i18n.tr("Tỷ lệ khung hình kết xuất:", "Render Scale:"),
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
                                    changed |= scale_resp.changed();
                                    scale_resp.labelled_by(scale_lbl.id);
                                });
                            },
                        );
                        ui.add_space(8.0);
                    }
                    gaming_card_3d(
                        ui,
                        "settings_perf_audio_card",
                        false,
                        ACCENT_EMERALD,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Khởi chạy Game, Hiệu năng & Âm thanh",
                                    "Launch Behavior, Performance & Audio",
                                ))
                                .size(14.5)
                                .strong(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.label(app.i18n.tr("RAM mặc định:", "Default RAM:"));
                                changed |= ui
                                    .add(
                                        egui::Slider::new(
                                            &mut app.settings.default_ram_mb,
                                            1024..=16384,
                                        )
                                        .step_by(512.0)
                                        .suffix(" MB"),
                                    )
                                    .changed();
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label(app.i18n.tr(
                                    "Số luồng tải song song:",
                                    "Max Concurrent Downloads:",
                                ));
                                changed |= ui
                                    .add(egui::Slider::new(
                                        &mut app.settings.max_concurrent_downloads,
                                        2..=32,
                                    ))
                                    .changed();
                            });

                            changed |= java_runtime_mode_selector(
                                ui,
                                &app.i18n,
                                &mut app.settings.java_runtime_mode,
                                None,
                            );
                            ui.add_space(4.0);

                            let low_ram_lbl = app
                                .i18n
                                .tr(
                                    "Chế độ siêu tiết kiệm RAM cho Launcher",
                                    "Ultra Low-RAM Mode for Launcher",
                                )
                                .to_string();
                            changed |= crate::views::wrapped_checkbox(
                                ui,
                                &mut app.settings.low_ram_mode,
                                RichText::new(low_ram_lbl).strong(),
                            )
                            .changed();
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Tắt hoạt ảnh nền, giới hạn bộ nhớ ảnh và giải phóng tài nguyên đồ họa khi chạy nền.",
                                    "Disables idle animations, caps image caches, and frees graphics memory in the background.",
                                ))
                                .size(11.3)
                                .weak(),
                            );
                            ui.add_space(4.0);

                            let hide_lbl = app
                                .i18n
                                .tr(
                                    "Ẩn Launcher khi vào game và tự hiện lại khi thoát game",
                                    "Hide launcher while playing and restore on game exit",
                                )
                                .to_string();
                            changed |= crate::views::wrapped_checkbox(
                                ui,
                                &mut app.settings.hide_launcher_on_launch,
                                hide_lbl,
                            )
                            .changed();

                            let logs_lbl = app
                                .i18n
                                .tr(
                                    "Tự động chuyển sang tab Nhật ký (Logs) khi bấm Vào Game",
                                    "Automatically switch to Logs tab when launching",
                                )
                                .to_string();
                            changed |= crate::views::wrapped_checkbox(
                                ui,
                                &mut app.settings.open_logs_on_launch,
                                logs_lbl,
                            )
                            .changed();

                            let prio_lbl = app
                                .i18n
                                .tr(
                                    "Tăng độ ưu tiên CPU cho tiến trình game",
                                    "Boost game process CPU priority",
                                )
                                .to_string();
                            changed |= crate::views::wrapped_checkbox(
                                ui,
                                &mut app.settings.high_process_priority,
                                prio_lbl,
                            )
                            .changed();

                            ui.horizontal_wrapped(|ui| {
                                let sfx_lbl = app
                                    .i18n
                                    .tr("Âm thanh giao diện", "Interface Sounds")
                                    .to_string();
                                let bgm_lbl = app
                                    .i18n
                                    .tr("Nhạc nền Ambient", "Ambient Music")
                                    .to_string();
                                changed |= crate::views::wrapped_checkbox(
                                    ui,
                                    &mut app.settings.sfx_enabled,
                                    sfx_lbl,
                                )
                                .changed();
                                changed |= crate::views::wrapped_checkbox(
                                    ui,
                                    &mut app.settings.music_enabled,
                                    bgm_lbl,
                                )
                                .changed();
                            });
                            if app.settings.music_enabled {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        app.i18n.tr("Âm lượng nhạc nền:", "Music Volume:"),
                                    );
                                    changed |= ui
                                        .add(egui::Slider::new(
                                            &mut app.settings.music_volume,
                                            0.05..=1.0,
                                        ))
                                        .changed();
                                    if secondary_icon_button(
                                        ui,
                                        VectorIcon::Folder,
                                        app.i18n.tr("Tệp nhạc riêng...", "Custom Audio..."),
                                    )
                                    .clicked()
                                    {
                                        app.pick_custom_music_file();
                                    }
                                    if app.has_custom_music_file()
                                        && secondary_icon_button(
                                            ui,
                                            VectorIcon::Refresh,
                                            app.i18n.tr("Dùng Ambient gốc", "Default Synth"),
                                        )
                                        .clicked()
                                    {
                                        app.clear_custom_music_file();
                                    }
                                });
                            }

                            let rpc_lbl = app
                                .i18n
                                .tr(
                                    "Hiển thị trạng thái chơi trên Discord Rich Presence",
                                    "Broadcast playing status via Discord Rich Presence",
                                )
                                .to_string();
                            changed |= crate::views::wrapped_checkbox(
                                ui,
                                &mut app.settings.discord_rpc_enabled,
                                rpc_lbl,
                            )
                            .changed();

                            if app.settings.discord_rpc_enabled {
                                let mut cid_buf =
                                    app.settings.discord_client_id.clone().unwrap_or_default();
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(app.i18n.tr(
                                        "Discord App ID (tuỳ chọn):",
                                        "Discord App ID (optional):",
                                    ));
                                    let cid_w =
                                        220.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                                    if ui
                                        .add(
                                            egui::TextEdit::singleline(&mut cid_buf)
                                                .hint_text(app.i18n.tr(
                                                    "Mặc định tích hợp sẵn",
                                                    "Built-in default",
                                                ))
                                                .desired_width(cid_w),
                                        )
                                        .changed()
                                    {
                                        app.settings.discord_client_id = (!cid_buf
                                            .trim()
                                            .is_empty())
                                        .then(|| cid_buf.trim().to_string());
                                        changed = true;
                                    }
                                });
                            }
                        },
                    );

                    ui.add_space(8.0);
                    gaming_card_3d(
                        ui,
                        "settings_storage_cache_card",
                        false,
                        ACCENT_AMBER,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Lưu trữ & Bộ nhớ đệm",
                                    "Storage & Cache",
                                ))
                                .size(14.5)
                                .strong(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.label(app.i18n.tr("Dung lượng cache tạm:", "Temporary Cache:"));
                                badge(ui, &format_bytes(app.cached_dir_bytes), ACCENT_AMBER);
                                if secondary_icon_button(
                                    ui,
                                    VectorIcon::Refresh,
                                    app.i18n.tr("Tính lại", "Refresh"),
                                )
                                .clicked()
                                {
                                    app.refresh_cache_size();
                                    app.refresh_recovery_artifacts(false);
                                }
                                if !app.confirm_clear_cache {
                                    if danger_icon_button(
                                        ui,
                                        VectorIcon::Trash,
                                        app.i18n.tr("Dọn sạch Cache", "Clear Cache"),
                                    )
                                    .clicked()
                                    {
                                        app.confirm_clear_cache = true;
                                    }
                                } else {
                                    if danger_icon_button(
                                        ui,
                                        VectorIcon::Trash,
                                        app.i18n
                                            .tr("Xác nhận xóa Cache", "Confirm Clear Cache"),
                                    )
                                    .clicked()
                                    {
                                        app.clear_app_cache();
                                    }
                                    if secondary_button(ui, app.i18n.t("common.cancel")).clicked() {
                                        app.confirm_clear_cache = false;
                                    }
                                }
                                if secondary_icon_button(
                                    ui,
                                    VectorIcon::Folder,
                                    app.i18n.tr("Mở thư mục dữ liệu", "Open Data Root"),
                                )
                                .clicked()
                                {
                                    let root = app.paths.root.clone();
                                    app.open_local_folder_or_toast(&root);
                                }
                                if secondary_icon_button(
                                    ui,
                                    VectorIcon::Copy,
                                    app.i18n.tr("Sao chép nhật ký", "Copy Log"),
                                )
                                .clicked()
                                {
                                    app.copy_launcher_log_to_clipboard(ui.ctx());
                                }
                            });

                            if !app.recovery_artifacts.is_empty() {
                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(4.0);
                                let total_rec_bytes: u64 =
                                    app.recovery_artifacts.iter().map(|r| r.size_bytes).sum();
                                let rec_count = app.recovery_artifacts.len();
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(app.i18n.tr(
                                            "Tệp phục hồi cài đặt / cập nhật còn sót:",
                                            "Leftover install / update recovery files:",
                                        ))
                                        .strong()
                                        .color(ACCENT_AMBER),
                                    );
                                    badge(
                                        ui,
                                        &format!(
                                            "{rec_count} • {}",
                                            format_bytes(total_rec_bytes)
                                        ),
                                        ACCENT_AMBER,
                                    );
                                });
                                let snapshot = app.recovery_artifacts.clone();
                                for item in snapshot.iter().take(6) {
                                    let kind_lbl = match item.kind {
                                        RecoveryArtifactKind::ModpackStaging => "Modpack Staging",
                                        RecoveryArtifactKind::ContentBatchStaging => "Addon Staging",
                                        RecoveryArtifactKind::UpdaterBackup => "Updater Backup",
                                        RecoveryArtifactKind::UpdaterFailed => "Updater Failed",
                                        RecoveryArtifactKind::UpdaterLock => "Updater Lock",
                                    };
                                    ui.horizontal_wrapped(|ui| {
                                        badge(ui, kind_lbl, ACCENT_CYAN);
                                        ui.label(
                                            RichText::new(item.path.display().to_string())
                                                .size(11.2)
                                                .weak(),
                                        );
                                        if secondary_icon_button(
                                            ui,
                                            VectorIcon::Folder,
                                            app.i18n.tr("Mở vị trí", "Open Location"),
                                        )
                                        .clicked()
                                        {
                                            let target =
                                                item.path.parent().unwrap_or(&item.path);
                                            app.open_local_folder_or_toast(target);
                                        }
                                    });
                                }
                                let tasks_busy =
                                    !app.active_tasks.is_empty() || app.updater.applying;
                                ui.add_space(4.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.add_enabled_ui(!tasks_busy, |ui| {
                                        if !app.confirm_clear_recovery {
                                            if danger_icon_button(
                                                ui,
                                                VectorIcon::Trash,
                                                app.i18n.tr(
                                                    "Dọn dẹp tệp phục hồi cũ",
                                                    "Clean Leftover Recovery Files",
                                                ),
                                            )
                                            .clicked()
                                            {
                                                app.confirm_clear_recovery = true;
                                            }
                                        } else {
                                            if danger_icon_button(
                                                ui,
                                                VectorIcon::Trash,
                                                app.i18n.tr(
                                                    "Xác nhận xóa tệp phục hồi",
                                                    "Confirm Delete Recovery Files",
                                                ),
                                            )
                                            .clicked()
                                            {
                                                app.clean_recovery_artifacts();
                                            }
                                            if secondary_button(ui, app.i18n.t("common.cancel"))
                                                .clicked()
                                            {
                                                app.confirm_clear_recovery = false;
                                            }
                                        }
                                    });
                                    if tasks_busy {
                                        ui.label(
                                            RichText::new(app.i18n.tr(
                                                "(Tạm khóa khi đang có tác vụ cài đặt/cập nhật chạy nền)",
                                                "(Disabled while install/update tasks are active)",
                                            ))
                                            .size(11.0)
                                            .weak(),
                                        );
                                    }
                                });
                            }
                        },
                    );

                    ui.add_space(8.0);
                    gaming_card_3d(
                        ui,
                        "settings_cf_api_card",
                        false,
                        ACCENT_ORANGE,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Khoá API CurseForge (Tuỳ chọn)",
                                    "CurseForge API Key (Optional)",
                                ))
                                .size(14.5)
                                .strong(),
                            );
                            let mut cf_buf =
                                app.settings.curseforge_api_key.clone().unwrap_or_default();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut cf_buf)
                                        .password(true)
                                        .hint_text(app.i18n.tr(
                                            "$2a$10$... (Để trống nếu dùng chế độ mặc định)",
                                            "$2a$10$... (Leave blank for default public access)",
                                        ))
                                        .desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                app.settings.curseforge_api_key =
                                    (!cf_buf.trim().is_empty()).then(|| cf_buf.trim().to_string());
                                changed = true;
                            }
                        },
                    );

                    ui.add_space(8.0);
                    gaming_card_3d(
                        ui,
                        "settings_updater_card",
                        false,
                        ACCENT_CYAN,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(app.i18n.tr(
                                    "Cập nhật Tự động",
                                    "Automatic Updates",
                                ))
                                .size(14.5)
                                .strong(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.label("GitHub Repo:");
                                let repo_w =
                                    230.0_f32.min((ui.max_rect().width() - 16.0).max(72.0));
                                changed |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut app.settings.github_repo)
                                            .desired_width(repo_w),
                                    )
                                    .changed();
                                let auto_upd_lbl = app
                                    .i18n
                                    .tr("Tự kiểm tra khi mở", "Check on startup")
                                    .to_string();
                                changed |= crate::views::wrapped_checkbox(
                                    ui,
                                    &mut app.settings.auto_check_updates,
                                    auto_upd_lbl,
                                )
                                .changed();
                            });

                            ui.horizontal(|ui| {
                                if app.updater.checking {
                                    ui.spinner();
                                } else if secondary_icon_button(
                                    ui,
                                    VectorIcon::Search,
                                    app.i18n.tr("Kiểm tra bản cập nhật mới", "Check for Updates"),
                                )
                                .clicked()
                                {
                                    app.check_app_updates();
                                }
                            });
                            if let Some(ref st) = app.updater.status_line {
                                ui.add_space(4.0);
                                let st_col = if app.updater.last_error {
                                    ACCENT_RED
                                } else if app.updater.available.is_some() {
                                    ACCENT_EMERALD
                                } else {
                                    ui.visuals().weak_text_color()
                                };
                                ui.label(RichText::new(st).size(12.0).color(st_col));
                            }

                            if let Some(rel) = app.updater.available.clone() {
                                ui.add_space(4.0);
                                ui.horizontal_wrapped(|ui| {
                                    badge(ui, &rel.tag_name, ACCENT_EMERALD);
                                    ui.label(format!(
                                        "{} ({})",
                                        rel.asset_name,
                                        format_bytes(rel.asset_size)
                                    ));
                                    if primary_icon_button(
                                        ui,
                                        VectorIcon::Download,
                                        app.i18n.tr(
                                            "Tải & Xác thực SHA-256",
                                            "Download & Verify SHA-256",
                                        ),
                                    )
                                    .clicked()
                                    {
                                        app.download_verified_app_update();
                                    }
                                    if secondary_icon_button(
                                        ui,
                                        VectorIcon::Globe,
                                        app.i18n.tr("Xem Release", "Release Notes"),
                                    )
                                    .clicked()
                                    {
                                        let _ = crate::dialogs::open_external_url(&rel.html_url);
                                    }
                                });
                                if !rel.release_notes.is_empty() {
                                    ui.label(RichText::new(&rel.release_notes).size(11.5).weak());
                                }
                            }

                            if let Some(ref dl_file) = app.updater.downloaded_file.clone() {
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(format!(
                                        "{}: {}",
                                        app.i18n.tr("Sẵn sàng cài đặt", "Verified & Ready"),
                                        dl_file.path.display()
                                    ))
                                    .size(11.5)
                                    .color(ACCENT_EMERALD),
                                );
                                ui.horizontal_wrapped(|ui| {
                                    if primary_icon_button(
                                        ui,
                                        VectorIcon::Sparkles,
                                        app.i18n.tr(
                                            "Áp dụng & Khởi động lại ngay",
                                            "Apply & Restart Now",
                                        ),
                                    )
                                    .clicked()
                                    {
                                        app.apply_verified_app_update();
                                    }
                                    if secondary_icon_button(
                                        ui,
                                        VectorIcon::Folder,
                                        app.i18n
                                            .tr("Mở thư mục bản cập nhật", "Open Update Folder"),
                                    )
                                    .clicked()
                                    {
                                        app.open_updates_folder();
                                    }
                                });
                            }
                        },
                    );
                });

            if changed {
                app.persist_settings();
            }
        });
    app.show_settings_modal &= open;
}

fn render_close_confirm_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let max_mw = (crate::views::safe_content_rect(ctx).width() - 24.0).max(140.0);
    egui::Modal::new(egui::Id::unique("thk_close_confirm_modal")).show(ctx, |ui| {
        ui.set_width(420.0_f32.min(max_mw));
        ui.heading(
            RichText::new(app.i18n.t("settings.close_ask_title"))
                .size(17.5)
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(app.i18n.t("settings.close_ask_desc"));
        ui.add_space(8.0);
        crate::views::wrapped_checkbox(
            ui,
            &mut app.remember_close_choice,
            app.i18n.t("settings.remember_choice"),
        );
        ui.add_space(10.0);

        ui.horizontal_wrapped(|ui| {
            if primary_icon_button(ui, VectorIcon::Download, app.i18n.t("settings.close_tray"))
                .clicked()
            {
                if app.remember_close_choice {
                    app.settings.close_behavior = CloseBehavior::MinimizeToTray;
                    app.persist_settings();
                }
                app.show_close_confirm_modal = false;
                app.pending_memory_trim = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }

            if danger_icon_button(ui, VectorIcon::Stop, app.i18n.t("settings.close_quit")).clicked()
            {
                if app.remember_close_choice {
                    app.settings.close_behavior = CloseBehavior::Quit;
                    app.persist_settings();
                }
                app.show_close_confirm_modal = false;
                app.allow_immediate_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            if secondary_button(ui, app.i18n.t("common.cancel")).clicked() {
                app.show_close_confirm_modal = false;
            }
        });
    });
}

fn render_server_eula_confirm_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let Some((ref srv_id, _)) = app.pending_eula_server_id else {
        return;
    };
    let srv_info = app
        .servers
        .servers
        .iter()
        .find(|s| &s.id == srv_id)
        .map(|s| (s.name.clone(), s.server_dir.display().to_string()));
    let Some((srv_name, srv_dir_str)) = srv_info else {
        app.pending_eula_server_id = None;
        return;
    };

    let max_mw = (crate::views::safe_content_rect(ctx).width() - 24.0).max(140.0);
    egui::Modal::new(egui::Id::unique("thk_server_eula_confirm_modal")).show(ctx, |ui| {
        ui.set_width(470.0_f32.min(max_mw));
        ui.horizontal(|ui| {
            paint_icon_3d(ui, 38.0, Icon3dKind::ServerBladeTower, true);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(app.i18n.tr(
                        "Xác nhận Điều khoản Minecraft EULA",
                        "Accept Minecraft Server EULA",
                    ))
                    .size(16.5)
                    .strong(),
                );
                ui.label(
                    RichText::new(format!("{srv_name} • {srv_dir_str}"))
                        .size(11.5)
                        .weak(),
                );
            });
        });
        ui.add_space(8.0);
        ui.label(app.i18n.tr(
            "Trước khi khởi chạy Dedicated Server, Mojang yêu cầu bạn đọc và đồng ý tường minh với Thỏa thuận Người dùng Cuối của Minecraft (Minecraft EULA). Khi xác nhận, trình khởi chạy sẽ ghi eula=true vào tệp eula.txt của máy chủ này.",
            "Before starting a local Dedicated Server, Mojang requires explicit acceptance of the Minecraft End User License Agreement (EULA). Confirming will write eula=true into this server's eula.txt.",
        ));
        ui.add_space(6.0);
        ui.hyperlink_to(
            "https://www.minecraft.net/eula",
            "https://www.minecraft.net/eula",
        );
        ui.add_space(12.0);

        ui.horizontal_wrapped(|ui| {
            if primary_icon_button(
                ui,
                VectorIcon::Play,
                app.i18n
                    .tr("Tôi đồng ý EULA & Khởi động", "Accept EULA & Start"),
            )
            .clicked()
            {
                app.click_sfx();
                app.confirm_server_eula_and_start();
            }
            if secondary_button(ui, app.i18n.t("common.cancel")).clicked() {
                app.click_sfx();
                app.pending_eula_server_id = None;
            }
        });
    });
}

fn render_delete_confirm_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let Some(modal_snap) = app.pending_delete_modal.clone() else {
        return;
    };

    let heading_txt = app
        .i18n
        .tr("Xác nhận thao tác xóa", "Confirm Deletion")
        .to_string();
    let disk_chk_lbl = app
        .i18n
        .tr(
            "Đồng thời xóa vĩnh viễn toàn bộ thư mục dữ liệu trên ổ đĩa",
            "Also permanently delete the entire data folder from disk",
        )
        .to_string();
    let custom_warn_lbl = app
        .i18n
        .tr(
            "Thư mục này là đường dẫn tuỳ chỉnh bên ngoài vùng quản lý mặc định. Hãy kiểm tra kỹ trước khi bật xóa trên ổ đĩa.",
            "This uses a custom external directory path. Verify carefully before enabling disk deletion.",
        )
        .to_string();
    let confirm_lbl = app.i18n.tr("Xác nhận xóa", "Confirm Delete").to_string();
    let cancel_lbl = app.i18n.t("common.cancel").to_string();
    let lang = app.settings.language;

    let mut delete_on_disk = modal_snap.delete_files_on_disk;
    let mut do_confirm = false;
    let mut do_cancel = false;

    let max_mw = (crate::views::safe_content_rect(ctx).width() - 24.0).max(140.0);
    egui::Modal::new(egui::Id::unique("thk_delete_confirm_modal")).show(ctx, |ui| {
        ui.set_width(480.0_f32.min(max_mw));
        ui.heading(RichText::new(&heading_txt).size(17.0).strong().color(ACCENT_RED));
        ui.add_space(6.0);

        match &modal_snap.target {
            DeleteTarget::Profile {
                name,
                instance_path,
                is_custom_path,
                ..
            } => {
                let desc = if lang == Language::Vi {
                    format!("Bạn sắp xóa Profile '{name}'.")
                } else {
                    format!("You are about to delete profile '{name}'.")
                };
                ui.label(RichText::new(desc).strong());
                ui.label(
                    RichText::new(instance_path.display().to_string())
                        .size(11.5)
                        .weak(),
                );
                ui.add_space(8.0);
                crate::views::wrapped_checkbox(ui, &mut delete_on_disk, &disk_chk_lbl);
                if *is_custom_path {
                    ui.label(RichText::new(&custom_warn_lbl).size(11.5).color(ACCENT_AMBER));
                }
            }
            DeleteTarget::Server {
                name,
                server_dir,
                is_custom_dir,
                ..
            } => {
                let desc = if lang == Language::Vi {
                    format!("Bạn sắp xóa máy chủ cục bộ '{name}'.")
                } else {
                    format!("You are about to delete local server '{name}'.")
                };
                ui.label(RichText::new(desc).strong());
                ui.label(
                    RichText::new(server_dir.display().to_string())
                        .size(11.5)
                        .weak(),
                );
                ui.add_space(8.0);
                crate::views::wrapped_checkbox(ui, &mut delete_on_disk, &disk_chk_lbl);
                if *is_custom_dir {
                    ui.label(RichText::new(&custom_warn_lbl).size(11.5).color(ACCENT_AMBER));
                }
            }
            DeleteTarget::World { folder_name, .. } => {
                let desc = if lang == Language::Vi {
                    format!(
                        "Xóa vĩnh viễn thế giới chơi đơn '{folder_name}'? Dữ liệu bản đồ trong saves/{folder_name} sẽ không thể khôi phục."
                    )
                } else {
                    format!(
                        "Permanently delete singleplayer world '{folder_name}'? World data inside saves/{folder_name} cannot be recovered."
                    )
                };
                ui.label(desc);
            }
            DeleteTarget::ProfileFiles { targets, .. }
            | DeleteTarget::ServerFiles { targets, .. } => {
                let desc = if lang == Language::Vi {
                    format!(
                        "Xóa vĩnh viễn {} tệp / thư mục đã chọn khỏi ổ đĩa?",
                        targets.len()
                    )
                } else {
                    format!(
                        "Permanently delete {} selected file(s) / folder(s) from disk?",
                        targets.len()
                    )
                };
                ui.label(RichText::new(desc).strong());
                ui.add_space(4.0);
                for rel in targets.iter().take(5) {
                    ui.label(RichText::new(format!("• {rel}")).size(11.5).weak());
                }
                if targets.len() > 5 {
                    ui.label(
                        RichText::new(format!("… +{}", targets.len() - 5))
                            .size(11.5)
                            .weak(),
                    );
                }
            }
        }

        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            if danger_icon_button(ui, VectorIcon::Trash, &confirm_lbl).clicked() {
                do_confirm = true;
            }
            if secondary_button(ui, &cancel_lbl).clicked() {
                do_cancel = true;
            }
        });
    });

    if let Some(m) = app.pending_delete_modal.as_mut() {
        m.delete_files_on_disk = delete_on_disk;
    }
    if do_confirm {
        app.click_sfx();
        app.confirm_pending_delete();
    } else if do_cancel {
        app.click_sfx();
        app.pending_delete_modal = None;
    }
}

fn render_disk_pack_import_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let Some(snap) = app.disk_pack_modal.clone() else {
        return;
    };

    let title = app
        .i18n
        .tr(
            "Nhập Gói Modpack từ Ổ Đĩa (.mrpack / .zip)",
            "Import Modpack Archive (.mrpack / .zip)",
        )
        .to_string();
    let lbl_name = app
        .i18n
        .tr("Tên Profile mới:", "New Profile Name:")
        .to_string();
    let lbl_ram = app.i18n.tr("Phân bổ RAM:", "Allocated RAM:").to_string();
    let lbl_dir = app
        .i18n
        .tr("Thư mục lưu trữ Instance:", "Instance Folder:")
        .to_string();
    let default_dir_lbl = app
        .i18n
        .tr("Mặc định (instances/)", "Default (instances/)")
        .to_string();
    let pick_dir_lbl = app
        .i18n
        .tr("Chọn thư mục...", "Choose Folder...")
        .to_string();
    let reset_dir_lbl = app.i18n.tr("Mặc định", "Reset").to_string();
    let install_lbl = app
        .i18n
        .tr("Bắt đầu Nhập & Cài đặt", "Import & Install")
        .to_string();
    let minimize_lbl = app
        .i18n
        .tr("Chạy nền (Thu nhỏ)", "Run in Background")
        .to_string();
    let cancel_install_lbl = app.i18n.tr("Hủy cài đặt", "Cancel Install").to_string();
    let close_lbl = app.i18n.t("common.cancel").to_string();
    let task_prog = app
        .active_tasks
        .get("disk-pack-import")
        .map(|t| t.progress.clone());

    let mut profile_name = snap.profile_name.clone();
    let mut ram_mb = snap.ram_mb;
    let mut java_mode = app.settings.java_runtime_mode;
    let mut java_mode_changed = false;
    let mut clear_custom_dir = false;
    let mut ask_pick_dir = false;
    let mut trigger_start = false;
    let mut trigger_minimize = false;
    let mut trigger_cancel = false;
    let mut trigger_close = false;

    let safe_rect = crate::views::safe_content_rect(ctx);
    let init_rect = centered_window_rect(ctx, vec2(540.0, 440.0));
    egui::Window::new(&title)
        .id(egui::Id::unique("thk_disk_pack_import_modal"))
        .default_rect(init_rect)
        .min_size(vec2(300.0, 270.0).min(init_rect.size()))
        .collapsible(false)
        .resizable(true)
        .constrain_to(safe_rect)
        .order(Order::Foreground)
        .show(ctx, |ui| {
            let scroll_w = ui.available_width();
            ui.set_width(scroll_w);
            let body_h = (ui.available_height() - 52.0).max(140.0);
            ScrollArea::vertical()
                .id_salt("disk_pack_import_body_scroll")
                .max_height(body_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(scroll_w);
                    ui.horizontal(|ui| {
                        paint_icon_3d(ui, 38.0, Icon3dKind::ModpackCrate, true);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&title).size(16.5).strong());
                            let fname = snap
                                .archive_path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            ui.label(RichText::new(fname.to_string()).size(11.5).weak());
                        });
                    });
                    ui.add_space(8.0);

                    ui.horizontal_wrapped(|ui| {
                        badge(ui, &snap.format_label, ACCENT_CYAN);
                        badge(
                            ui,
                            &format!("MC {}", snap.detected_mc_version),
                            ACCENT_EMERALD,
                        );
                        badge(ui, snap.detected_loader.display_name(), ACCENT_VIOLET);
                        badge(
                            ui,
                            &format!("{} files", snap.detected_file_count),
                            ACCENT_BLUE,
                        );
                    });
                    ui.add_space(8.0);

                    ui.add_enabled_ui(!snap.installing, |ui| {
                        ui.label(RichText::new(&lbl_name).strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut profile_name)
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(6.0);

                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&lbl_ram).strong());
                            ui.add(
                                egui::Slider::new(&mut ram_mb, 2048..=16384)
                                    .step_by(512.0)
                                    .suffix(" MB"),
                            );
                        });
                        ui.add_space(4.0);

                        if java_runtime_mode_selector(
                            ui,
                            &app.i18n,
                            &mut java_mode,
                            Some(&snap.detected_mc_version),
                        ) {
                            java_mode_changed = true;
                        }
                        ui.add_space(4.0);

                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&lbl_dir).strong());
                            let dir_txt = snap
                                .custom_dir
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| default_dir_lbl.clone());
                            ui.label(RichText::new(dir_txt).size(11.5).weak());
                            if secondary_icon_button(ui, VectorIcon::Folder, &pick_dir_lbl)
                                .clicked()
                            {
                                ask_pick_dir = true;
                            }
                            if snap.custom_dir.is_some()
                                && secondary_icon_button(ui, VectorIcon::Refresh, &reset_dir_lbl)
                                    .clicked()
                            {
                                clear_custom_dir = true;
                            }
                        });
                    });

                    if let Some(ref err) = snap.error_msg {
                        ui.add_space(8.0);
                        ui.label(RichText::new(err).size(12.0).color(ACCENT_RED));
                    }

                    if snap.installing {
                        ui.add_space(10.0);
                        if let Some(prog) = task_prog {
                            let frac = (prog.percent / 100.0).clamp(0.0, 1.0);
                            paint_glowing_progress_bar(ui, frac, ui.available_width(), 18.0);
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                badge(ui, &prog.phase, ACCENT_CYAN);
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({}/{})",
                                        prog.message, prog.completed_files, prog.total_files
                                    ))
                                    .size(11.8),
                                );
                            });
                        } else {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(app.i18n.tr(
                                    "Đang giải nén và kiểm tra gói Modpack...",
                                    "Extracting and verifying Modpack archive...",
                                ));
                            });
                        }
                    }
                });

            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                if snap.installing {
                    if secondary_icon_button(ui, VectorIcon::Download, &minimize_lbl).clicked() {
                        trigger_minimize = true;
                    }
                    if danger_icon_button(ui, VectorIcon::Close, &cancel_install_lbl).clicked() {
                        trigger_cancel = true;
                    }
                } else {
                    if primary_icon_button(ui, VectorIcon::Download, &install_lbl).clicked() {
                        trigger_start = true;
                    }
                    if secondary_button(ui, &close_lbl).clicked() {
                        trigger_close = true;
                    }
                }
            });
        });

    if let Some(m) = app.disk_pack_modal.as_mut() {
        if !m.installing {
            m.profile_name = profile_name;
            m.ram_mb = ram_mb;
            if clear_custom_dir {
                m.custom_dir = None;
            }
        }
        if trigger_minimize {
            m.minimized = true;
        }
    }
    if java_mode_changed {
        app.settings.java_runtime_mode = java_mode;
        app.click_sfx();
        app.persist_settings();
    }
    if ask_pick_dir {
        app.click_sfx();
        app.pick_disk_pack_custom_dir();
    }
    if trigger_start {
        app.click_sfx();
        app.start_disk_modpack_import();
    } else if trigger_cancel {
        app.click_sfx();
        app.cancel_disk_pack_import();
    } else if trigger_close {
        app.click_sfx();
        app.disk_pack_modal = None;
    }
}
