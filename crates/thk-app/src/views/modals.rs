use crate::fx3d::{
    gaming_card_3d, paint_3d_tunnel_conduit, paint_icon_3d, tactile_button_3d, ButtonStyle3d,
    Icon3dKind, VectorIcon,
};
use crate::state::LauncherAppState;
use crate::theme::{
    ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_ORANGE, ACCENT_RED,
    ACCENT_VIOLET,
};
use crate::views::{
    badge, danger_button, danger_icon_button, format_bytes, primary_icon_button,
    secondary_button, secondary_icon_button,
};
use egui::{vec2, Align2, Color32, Frame, Margin, RichText, ScrollArea};
use thk_core::models::{CloseBehavior, Language, LoaderType, ServerEngine, ThemeMode};
use thk_java::JavaDistroKind;
use thk_launcher::CrashFixAction;
use thk_network::TunnelStatus;

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
    if app.show_close_confirm_modal {
        render_close_confirm_modal(app, ctx);
    }
    crate::views::modpacks::render_modpack_modals(app, ctx);
}

fn render_initial_setup_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    egui::Modal::new(egui::Id::unique("thk_initial_setup_modal")).show(ctx, |ui| {
        ui.set_width(490.0);
        ui.vertical_centered(|ui| {
            paint_icon_3d(ui, 68.0, Icon3dKind::BrandVoxelCore, true);
            ui.add_space(4.0);
            ui.heading(
                RichText::new("Chào mừng đến với TriHienKun Laucher!")
                    .size(20.5)
                    .strong()
                    .color(ACCENT_EMERALD),
            );
            ui.label(
                RichText::new(
                    "Trình khởi chạy Minecraft thế hệ mới viết thuần 100% bằng Rust + wgpu — nhanh, nhẹ và độc lập.",
                )
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
                ui.horizontal(|ui| {
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

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Giao diện màu:").strong());
                    if ui
                        .selectable_value(
                            &mut app.settings.theme,
                            ThemeMode::Dark,
                            "Tối (Gaming 3D Dark)",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    if ui
                        .selectable_value(
                            &mut app.settings.theme,
                            ThemeMode::Light,
                            "Sáng (Light)",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("RAM mặc định cho Game:").strong());
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
        ui.vertical_centered(|ui| {
            if tactile_button_3d(
                ui,
                Some(VectorIcon::Play),
                "Bắt đầu sử dụng TriHienKun Laucher",
                ButtonStyle3d::HeroLaunch,
                vec2(320.0, 44.0),
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
    egui::Window::new("Tạo Profile Minecraft Mới")
        .id(egui::Id::unique("thk_modal_new_profile"))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(470.0);

            ui.horizontal(|ui| {
                paint_icon_3d(ui, 34.0, Icon3dKind::HomePortal, true);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Khởi tạo Instance Độc lập").size(15.0).strong());
                    ui.label(
                        RichText::new("Hỗ trợ Vanilla, Fabric, Quilt, Forge & NeoForge")
                            .size(11.5)
                            .weak(),
                    );
                });
            });
            ui.add_space(6.0);

            ui.label(RichText::new("Tên Profile:").strong());
            ui.add(
                egui::TextEdit::singleline(&mut app.home.new_draft.name)
                    .hint_text("Ví dụ: Survival 1.21.4")
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Phiên bản Minecraft:").strong());
                ui.checkbox(&mut app.home.include_snapshots, "Hiện bản Snapshot");
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
            egui::ComboBox::from_id_salt("new_profile_mc_ver_combo")
                .width(260.0)
                .selected_text(&app.home.new_draft.game_version)
                .show_ui(ui, |ui| {
                    for ver in &mc_versions {
                        ui.selectable_value(&mut app.home.new_draft.game_version, ver.clone(), ver);
                    }
                });
            if app.home.new_draft.game_version != prev_mc {
                let mc = app.home.new_draft.game_version.clone();
                let ldr = app.home.new_draft.loader;
                app.home.new_draft.loader_version.clear();
                app.ensure_loader_versions(&mc, ldr);
            }

            ui.add_space(6.0);
            ui.label(RichText::new("Trình nạp Mod (Mod Loader):").strong());
            ui.horizontal_wrapped(|ui| {
                for ldr in LoaderType::ALL {
                    if ui
                        .selectable_label(app.home.new_draft.loader == ldr, ldr.display_name())
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

                ui.horizontal(|ui| {
                    ui.label("Phiên bản Loader:");
                    let sel_txt = if app.home.new_draft.loader_version.is_empty() {
                        "Tự động chọn mới nhất"
                    } else {
                        app.home.new_draft.loader_version.as_str()
                    };
                    egui::ComboBox::from_id_salt("new_profile_loader_ver_combo")
                        .width(230.0)
                        .selected_text(sel_txt)
                        .show_ui(ui, |ui| {
                            for entry in ldr_entries {
                                let tag = if entry.stable {
                                    format!("{} (Ổn định)", entry.version)
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
                ui.checkbox(
                    &mut app.home.new_draft.auto_perf_mods,
                    "Tự động cài bộ Mod tối ưu FPS (Sodium, Lithium, FerriteCore, EntityCulling, ModMenu)",
                );
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(app.i18n.t("profile.ram")).strong());
                ui.add(
                    egui::Slider::new(&mut app.home.new_draft.ram_mb, 1024..=16384)
                        .step_by(512.0)
                        .suffix(" MB"),
                );
            });

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(app.i18n.tr("Thư mục lưu trữ:", "Instance Folder:")).strong(),
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
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
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

            ui.add_space(12.0);
            ui.horizontal(|ui| {
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
    egui::Window::new("Tạo Máy Chủ Minecraft Cục Bộ")
        .id(egui::Id::unique("thk_modal_new_server"))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(490.0);

            ui.horizontal(|ui| {
                paint_icon_3d(ui, 34.0, Icon3dKind::ServerBladeTower, true);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Triển khai Dedicated Server").size(15.0).strong());
                    ui.label(
                        RichText::new("Tự động tải JAR, cấu hình EULA và quản lý tiến trình")
                            .size(11.5)
                            .weak(),
                    );
                });
            });
            ui.add_space(6.0);

            ui.label(RichText::new("Tên Máy chủ:").strong());
            ui.add(
                egui::TextEdit::singleline(&mut app.servers.new_draft.name)
                    .hint_text("TriHienKun SMP")
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(6.0);
            ui.label(RichText::new("Lõi Máy chủ (Server Engine):").strong());
            ui.horizontal_wrapped(|ui| {
                for eng in ServerEngine::ALL {
                    if ui
                        .selectable_label(app.servers.new_draft.engine == eng, eng.display_name())
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

            ui.horizontal(|ui| {
                ui.label(RichText::new("Phiên bản Minecraft:").strong());
                if eng_vers.is_empty() {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.servers.new_draft.game_version)
                            .desired_width(140.0),
                    );
                } else {
                    egui::ComboBox::from_id_salt("new_server_ver_combo")
                        .width(180.0)
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

                ui.label("Cổng (Port):");
                ui.add(egui::DragValue::new(&mut app.servers.new_draft.port).range(1024..=65535));
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("RAM tối thiểu:");
                ui.add(
                    egui::Slider::new(&mut app.servers.new_draft.ram_min_mb, 512..=8192)
                        .step_by(256.0)
                        .suffix(" MB"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("RAM tối đa:");
                ui.add(
                    egui::Slider::new(&mut app.servers.new_draft.ram_max_mb, 1024..=16384)
                        .step_by(512.0)
                        .suffix(" MB"),
                );
            });

            ui.checkbox(
                &mut app.servers.new_draft.install_immediately,
                app.i18n.tr(
                    "Tự động tải lõi JAR và chuẩn bị môi trường ngay sau khi tạo",
                    "Automatically download server JAR and initialize directory on creation",
                ),
            );

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(app.i18n.tr("Thư mục máy chủ:", "Server Folder:")).strong());
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
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
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

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if primary_icon_button(ui, VectorIcon::Plus, app.i18n.t("server.create")).clicked()
                {
                    app.create_server_from_draft();
                }
                if secondary_button(ui, app.i18n.t("common.cancel")).clicked() {
                    app.show_new_server_modal = false;
                }
            });
        });
    app.show_new_server_modal &= open;
}

fn render_java_manager_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let mut open = app.show_java_modal;
    egui::Window::new("Quản lý & Tải Tự Động Java Runtime (JRE/JDK)")
        .id(egui::Id::unique("thk_modal_java_manager"))
        .collapsible(false)
        .resizable(true)
        .default_width(690.0)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                paint_icon_3d(ui, 36.0, Icon3dKind::JavaReactor, true);
                ui.heading(
                    RichText::new("Các bản Java hiện có trên hệ thống")
                        .size(16.0)
                        .strong(),
                );
                if secondary_icon_button(ui, VectorIcon::Refresh, "Quét lại").clicked() {
                    app.rescan_java_runtimes();
                }
            });
            ui.add_space(6.0);

            let installed = app.java_modal.installed.clone();
            ScrollArea::vertical()
                .id_salt("java_installed_scroll")
                .max_height(165.0)
                .show(ui, |ui| {
                    if installed.is_empty() {
                        ui.label(
                            RichText::new(
                                "Chưa phát hiện Java cài sẵn. Launcher vẫn sẽ tự động tải Mojang JRE khi vào game hoặc bạn có thể cài thủ công bên dưới.",
                            )
                            .weak(),
                        );
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
                                        badge(ui, "Launcher quản lý", ACCENT_BLUE);
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let p_str = j.path.to_string_lossy().to_string();
                                            if secondary_icon_button(
                                                ui,
                                                VectorIcon::Copy,
                                                "Chép đường dẫn",
                                            )
                                            .clicked()
                                            {
                                                ui.ctx().copy_text(p_str);
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
                ui.label(
                    RichText::new("Tải về bản phân phối Java (8 / 11 / 17 / 21 / 25):").strong(),
                );
                for d in JavaDistroKind::ALL {
                    if ui
                        .selectable_label(app.java_modal.selected_distro == d, d.display_name())
                        .clicked()
                    {
                        app.load_java_distro_packages(d);
                    }
                }
            });

            ui.add_space(6.0);
            let pkgs = app.java_modal.packages.clone();
            ScrollArea::vertical()
                .id_salt("java_distro_pkgs_scroll")
                .max_height(215.0)
                .show(ui, |ui| {
                    if pkgs.is_empty() {
                        ui.label(
                            RichText::new(
                                "Đang tải hoặc chưa có gói phù hợp cho kiến trúc CPU này.",
                            )
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
                                        let sz_txt = pkg
                                            .size_bytes
                                            .map(format_bytes)
                                            .unwrap_or_else(|| "Gói chính thức".into());
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
                                                "Cài đặt",
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
    egui::Window::new("Chia sẻ Thế giới LAN & Đường hầm Bore")
        .id(egui::Id::unique("thk_modal_lan_share"))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(520.0);

            ui.horizontal(|ui| {
                paint_icon_3d(ui, 38.0, Icon3dKind::LanRadar, true);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Radar LAN & Bore TCP Tunnel").size(15.5).strong());
                    if let Some(ref ip) = app.lan_modal.primary_lan_ip {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("IPv4 nội bộ:").size(12.0).weak());
                            badge(ui, ip, ACCENT_BLUE);
                        });
                    }
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
                        RichText::new(
                            "1. Tự động phát hiện 'Open to LAN' an toàn (UDP 224.0.2.60:4445)",
                        )
                        .strong(),
                    );
                    ui.label(
                        RichText::new(
                            "Chỉ chấp nhận gói phát sóng xuất phát từ chính card mạng của máy bạn (chống giả mạo gói tin từ máy khác trong LAN).",
                        )
                        .size(11.5)
                        .weak(),
                    );
                    ui.add_space(6.0);
                    ui.checkbox(
                        &mut app.lan_modal.auto_open_tunnel,
                        "Tự động mở cổng công khai bore.pub ngay khi phát hiện thế giới LAN",
                    );

                    ui.horizontal(|ui| {
                        if app.lan_modal.scanner_active {
                            if danger_icon_button(ui, VectorIcon::Stop, "Dừng quét LAN").clicked() {
                                app.toggle_lan_scanner();
                            }
                            ui.spinner();
                            ui.label("Đang lắng nghe 'Open to LAN' trong Minecraft...");
                        } else if primary_icon_button(ui, VectorIcon::Globe, "Bắt đầu quét LAN")
                            .clicked()
                        {
                            app.toggle_lan_scanner();
                        }
                    });

                    if let Some(ref world) = app.lan_modal.detected_world {
                        ui.add_space(4.0);
                        badge(
                            ui,
                            &format!(
                                "Đã phát hiện: {} ({}:{})",
                                world.motd, world.sender_ip, world.port
                            ),
                            ACCENT_EMERALD,
                        );
                    }
                    if app.lan_modal.ignored_foreign_packets > 0 {
                        ui.label(
                            RichText::new(format!(
                                "Đã chặn {} gói quảng bá LAN ngoại lai không thuộc máy này.",
                                app.lan_modal.ignored_foreign_packets
                            ))
                            .size(11.0)
                            .color(ACCENT_AMBER),
                        );
                    }
                },
            );

            ui.add_space(8.0);

            let tun_running = matches!(app.lan_modal.tunnel_status, TunnelStatus::Running { .. });
            let tun_connecting =
                matches!(app.lan_modal.tunnel_status, TunnelStatus::Connecting { .. });
            let local_lbl = format!("127.0.0.1:{}", app.lan_modal.manual_port_input.trim());
            let remote_lbl = match &app.lan_modal.tunnel_status {
                TunnelStatus::Running {
                    remote_addr,
                    remote_port,
                    ..
                } => format!("{remote_addr}:{remote_port}"),
                TunnelStatus::Connecting { .. } => "bore.pub (...)".to_string(),
                _ => "bore.pub (idle)".to_string(),
            };

            gaming_card_3d(
                ui,
                "lan_manual_bore_card",
                tun_running,
                ACCENT_EMERALD,
                Margin::same(12),
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("2. Mở Đường hầm Bore Công khai Thủ công").strong());
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
                        ui.label("Cổng nội bộ (Local Port):");
                        ui.add(
                            egui::TextEdit::singleline(&mut app.lan_modal.manual_port_input)
                                .desired_width(90.0),
                        );
                        if primary_icon_button(ui, VectorIcon::Globe, "Mở bore.pub").clicked() {
                            app.start_manual_bore_tunnel();
                        }
                        if (tun_running || tun_connecting)
                            && danger_icon_button(ui, VectorIcon::Stop, "Đóng cổng").clicked()
                        {
                            app.lan_coordinator.stop_tunnel();
                            app.lan_modal.tunnel_status = TunnelStatus::Stopped;
                        }
                    });

                    ui.add_space(6.0);
                    match &app.lan_modal.tunnel_status {
                        TunnelStatus::Idle | TunnelStatus::Stopped => {
                            ui.label(RichText::new("Trạng thái: Chưa mở đường hầm.").weak());
                        }
                        TunnelStatus::Connecting { local_port } => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(format!(
                                    "Đang kết nối cổng {local_port} tới bore.pub..."
                                ));
                            });
                        }
                        TunnelStatus::Running {
                            local_port,
                            remote_addr,
                            remote_port,
                        } => {
                            let endpoint = format!("{remote_addr}:{remote_port}");
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Gửi địa chỉ này cho bạn bè:");
                                badge(ui, &endpoint, ACCENT_EMERALD);
                                if primary_icon_button(ui, VectorIcon::Copy, "Sao chép").clicked() {
                                    ui.ctx().copy_text(endpoint);
                                }
                            });
                            ui.label(
                                RichText::new(format!("Đích nội bộ: 127.0.0.1:{local_port}"))
                                    .size(11.0)
                                    .weak(),
                            );
                        }
                        TunnelStatus::Error(err) => {
                            ui.label(RichText::new(format!("Lỗi: {err}")).color(ACCENT_RED));
                        }
                    }
                },
            );
        });
    app.show_lan_modal &= open;
}

fn render_crash_analyzer_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let Some((profile_id, report)) = app.crash_modal.clone() else {
        return;
    };

    let mut keep_open = true;
    let mut close_clicked = false;
    egui::Window::new("Phân Tích Sự Cố & Tự Động Khắc Phục Crash")
        .id(egui::Id::unique("thk_modal_crash_analyzer"))
        .collapsible(false)
        .resizable(true)
        .default_width(630.0)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut keep_open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                badge(
                    ui,
                    &format!("Exit Code {}", report.exit_code),
                    ACCENT_RED,
                );
                ui.label(
                    RichText::new(&report.summary_message)
                        .size(15.0)
                        .strong()
                        .color(ACCENT_RED),
                );
            });

            if let Some(ref root_line) = report.root_cause_line {
                ui.add_space(6.0);
                Frame::group(ui.style())
                    .fill(Color32::from_rgb(20, 10, 16))
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(root_line).monospace().size(11.5));
                    });
            }

            if !report.solutions.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new("Hướng xử lý đề xuất:").strong());
                for sol in &report.solutions {
                    ui.label(format!("• {sol}"));
                }
            }

            if !report.missing_mods.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Khắc phục tự động 1 chạm từ Modrinth:")
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
                                    let req_ver = advice
                                        .required_version
                                        .as_deref()
                                        .unwrap_or("bất kỳ");
                                    ui.label(
                                        RichText::new(format!(
                                            "{} ({}) — Yêu cầu bản: {req_ver}",
                                            advice.display_name, advice.mod_id
                                        ))
                                        .strong(),
                                    );
                                    if !advice.required_by.is_empty() {
                                        ui.label(
                                            RichText::new(format!(
                                                "Cần bởi: {}",
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
                                        CrashFixAction::InstallMod | CrashFixAction::UpdateMod => {
                                            if primary_icon_button(
                                                ui,
                                                VectorIcon::Wrench,
                                                "Tự động tải & sửa",
                                            )
                                            .clicked()
                                            {
                                                app.auto_fix_crash_mod(&profile_id, &advice.mod_id);
                                            }
                                        }
                                        CrashFixAction::UpgradeJava => {
                                            if primary_icon_button(
                                                ui,
                                                VectorIcon::Sliders,
                                                "Mở Quản lý Java",
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
                ui.collapsing("Xem chi tiết đoạn nhật ký lỗi (Stacktrace)", |ui| {
                    ScrollArea::vertical()
                        .id_salt("crash_relevant_lines_scroll")
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for line in &report.relevant_lines {
                                ui.label(RichText::new(line).monospace().size(11.5));
                            }
                        });
                });
            }

            ui.add_space(10.0);
            if danger_button(ui, "Đóng cửa sổ chẩn đoán").clicked() {
                close_clicked = true;
            }
        });

    if !keep_open || close_clicked {
        app.crash_modal = None;
    }
}

fn render_settings_modal(app: &mut LauncherAppState, ctx: &egui::Context) {
    let mut open = app.show_settings_modal;
    egui::Window::new("Cài đặt Hệ thống & Cập nhật")
        .id(egui::Id::unique("thk_modal_settings"))
        .collapsible(false)
        .resizable(true)
        .default_width(595.0)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            let mut changed = false;

            ui.horizontal(|ui| {
                paint_icon_3d(ui, 36.0, Icon3dKind::SettingsGyro, true);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Trung tâm Tuỳ chỉnh Hệ thống").size(15.5).strong());
                    ui.label(
                        RichText::new("Giao diện 3D, Âm thanh, Hiệu năng tải & Cập nhật SHA-256")
                            .size(11.5)
                            .weak(),
                    );
                });
            });
            ui.add_space(6.0);

            ScrollArea::vertical()
                .id_salt("settings_modal_scroll")
                .max_height(500.0)
                .show(ui, |ui| {
                    gaming_card_3d(
                        ui,
                        "settings_ui_card",
                        false,
                        ACCENT_VIOLET,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new("Giao diện & Hành vi cửa sổ")
                                    .size(14.5)
                                    .strong(),
                            );
                            ui.horizontal(|ui| {
                                ui.label("Ngôn ngữ:");
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

                            ui.horizontal(|ui| {
                                ui.label("Chủ đề màu:");
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.theme,
                                        ThemeMode::Dark,
                                        "Tối (Gaming 3D Dark)",
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.theme,
                                        ThemeMode::Light,
                                        "Sáng (Light)",
                                    )
                                    .changed();
                            });

                            ui.horizontal_wrapped(|ui| {
                                ui.label("Khi đóng cửa sổ:");
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.close_behavior,
                                        CloseBehavior::Ask,
                                        "Luôn hỏi",
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(
                                        &mut app.settings.close_behavior,
                                        CloseBehavior::MinimizeToTray,
                                        "Thu xuống khay",
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
                                        app.i18n.tr("Mặc định 3D Grid", "Default 3D Grid").to_string()
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
                    gaming_card_3d(
                        ui,
                        "settings_perf_audio_card",
                        false,
                        ACCENT_EMERALD,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new("Hiệu năng, Âm thanh & Discord RPC")
                                    .size(14.5)
                                    .strong(),
                            );
                            ui.horizontal(|ui| {
                                ui.label("RAM mặc định:");
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
                            ui.horizontal(|ui| {
                                ui.label("Số luồng tải song song:");
                                changed |= ui
                                    .add(egui::Slider::new(
                                        &mut app.settings.max_concurrent_downloads,
                                        2..=32,
                                    ))
                                    .changed();
                            });

                            ui.horizontal(|ui| {
                                changed |= ui
                                    .checkbox(
                                        &mut app.settings.sfx_enabled,
                                        "Hiệu ứng âm thanh UI (SFX)",
                                    )
                                    .changed();
                                changed |= ui
                                    .checkbox(&mut app.settings.music_enabled, "Nhạc nền Ambient")
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

                            changed |= ui
                                .checkbox(
                                    &mut app.settings.discord_rpc_enabled,
                                    "Hiển thị trạng thái chơi trên Discord Rich Presence",
                                )
                                .changed();

                            if app.settings.discord_rpc_enabled {
                                let mut cid_buf =
                                    app.settings.discord_client_id.clone().unwrap_or_default();
                                ui.horizontal(|ui| {
                                    ui.label("Discord App ID (tuỳ chọn):");
                                    if ui
                                        .add(
                                            egui::TextEdit::singleline(&mut cid_buf)
                                                .hint_text("Mặc định tích hợp sẵn")
                                                .desired_width(220.0),
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
                        "settings_cf_api_card",
                        false,
                        ACCENT_ORANGE,
                        Margin::same(12),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new("Khoá API CurseForge (Tuỳ chọn)")
                                    .size(14.5)
                                    .strong(),
                            );
                            let mut cf_buf =
                                app.settings.curseforge_api_key.clone().unwrap_or_default();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut cf_buf)
                                        .password(true)
                                        .hint_text(
                                            "$2a$10$... (Để trống nếu chỉ dùng Modrinth/FTB/Technic)",
                                        )
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
                                RichText::new("Cập nhật Tự động (Xác thực Băm SHA-256)")
                                    .size(14.5)
                                    .strong(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.label("GitHub Repo:");
                                changed |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut app.settings.github_repo)
                                            .desired_width(230.0),
                                    )
                                    .changed();
                                changed |= ui
                                    .checkbox(
                                        &mut app.settings.auto_check_updates,
                                        "Tự kiểm tra khi mở",
                                    )
                                    .changed();
                            });

                            ui.horizontal_wrapped(|ui| {
                                if app.updater.checking {
                                    ui.spinner();
                                } else if secondary_icon_button(
                                    ui,
                                    VectorIcon::Search,
                                    "Kiểm tra bản cập nhật mới",
                                )
                                .clicked()
                                {
                                    app.check_app_updates(false);
                                }
                                if let Some(ref st) = app.updater.status_line {
                                    ui.label(RichText::new(st).size(12.0));
                                }
                            });

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
                                        "Tải & Xác thực SHA-256",
                                    )
                                    .clicked()
                                    {
                                        app.download_verified_app_update();
                                    }
                                    if secondary_icon_button(ui, VectorIcon::Globe, "Xem Release")
                                        .clicked()
                                    {
                                        let _ = open::that(&rel.html_url);
                                    }
                                });
                                if !rel.release_notes.is_empty() {
                                    ui.label(RichText::new(&rel.release_notes).size(11.5).weak());
                                }
                            }

                            if let Some(ref dl_file) = app.updater.downloaded_file {
                                ui.label(
                                    RichText::new(format!(
                                        "Đã tải và kiểm chứng thành công: {}",
                                        dl_file.display()
                                    ))
                                    .size(11.5)
                                    .color(ACCENT_EMERALD),
                                );
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
    egui::Modal::new(egui::Id::unique("thk_close_confirm_modal")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(
            RichText::new(app.i18n.t("settings.close_ask_title"))
                .size(17.5)
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(app.i18n.t("settings.close_ask_desc"));
        ui.add_space(8.0);
        ui.checkbox(
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
