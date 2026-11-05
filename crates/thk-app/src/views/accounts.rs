use crate::fx3d::{gaming_card_3d, paint_icon_3d, Icon3dKind, VectorIcon};
use crate::state::{LauncherAppState, TextureSlot};
use crate::theme::{ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_VIOLET};
use crate::views::{
    accent_icon_button, badge, danger_icon_button, primary_button, primary_icon_button,
    secondary_icon_button,
};
use egui::{Align2, Color32, CornerRadius, FontId, Frame, Margin, RichText, ScrollArea, Stroke, Vec2};
use thk_core::models::{AccountType, SkinVariant};
use thk_renderer3d::BackEquipmentMode;

pub fn render_accounts_view(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    render_state: Option<&egui_wgpu::RenderState>,
) {
    app.sync_3d_viewport_textures(render_state);

    egui::Panel::right("thk_3d_skin_panel")
        .resizable(true)
        .default_size(415.0)
        .size_range(345.0..=550.0)
        .frame(Frame::NONE.inner_margin(Margin {
            left: 14,
            right: 0,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                paint_icon_3d(ui, 30.0, Icon3dKind::AccountCharacter, true);
                ui.heading(
                    RichText::new(
                        app.i18n
                            .tr("Mô hình Nhân vật 3D (wgpu)", "3D Character Preview (wgpu)"),
                    )
                    .size(17.5)
                    .strong(),
                );
            });
            ui.label(
                RichText::new(app.i18n.tr(
                    "Kéo xoay 360° • Cuộn thu phóng • Kéo-thả tệp .png vào khung để áp Skin",
                    "Drag 360° • Scroll to zoom • Drag & drop a .png file onto viewport to apply Skin",
                ))
                .size(11.3)
                .weak(),
            );
            ui.add_space(6.0);

            let hovered_png = ui.ctx().input(|i| {
                i.raw.hovered_files.iter().any(|f| {
                    f.path
                        .as_ref()
                        .and_then(|p| p.extension())
                        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                })
            });
            let dropped_png = ui.ctx().input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .find(|p| {
                        p.extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                    })
            });

            let has_selected_acct = app.selected_account().is_some();

            if let (Some(rs), Some(vp)) = (render_state, app.accounts.skin_viewport.as_mut()) {
                let canvas_h = (ui.available_height() - 255.0).clamp(220.0, 425.0);
                let before_top = ui.cursor().min;
                let canvas_w = ui.available_width();
                vp.show(ui, rs, Vec2::new(canvas_w, canvas_h));
                let vp_rect = egui::Rect::from_min_size(before_top, Vec2::new(canvas_w, canvas_h));

                if hovered_png && has_selected_acct {
                    let painter = ui.painter();
                    painter.rect(
                        vp_rect,
                        CornerRadius::same(12),
                        Color32::from_rgba_unmultiplied(16, 219, 134, 38),
                        Stroke::new(2.0, ACCENT_EMERALD),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        vp_rect.center(),
                        Align2::CENTER_CENTER,
                        app.i18n.tr(
                            "Thả tệp .PNG vào đây để cập nhật Skin 3D",
                            "Drop .PNG here to apply 3D Skin",
                        ),
                        FontId::proportional(14.5),
                        Color32::WHITE,
                    );
                }

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Cube,
                        app.i18n.tr("Chính diện", "Front View"),
                    )
                    .clicked()
                    {
                        vp.snap_to_preset(0.35, 0.14, 48.0);
                    }
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Shield,
                        app.i18n.tr("Áo choàng sau", "Back / Cape"),
                    )
                    .clicked()
                    {
                        vp.snap_to_preset(std::f32::consts::PI - 0.35, 0.16, 48.0);
                    }
                    if accent_icon_button(
                        ui,
                        VectorIcon::Sparkles,
                        app.i18n.tr("Góc Hero 3D", "3D Hero Angle"),
                    )
                    .clicked()
                    {
                        vp.snap_to_preset(0.72, 0.28, 44.0);
                    }
                });

                ui.add_space(6.0);
                gaming_card_3d(
                    ui,
                    "acct_3d_studio_controls",
                    false,
                    ACCENT_VIOLET,
                    Margin::same(10),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(app.i18n.tr("Trang bị lưng:", "Back Gear:")).strong(),
                            );
                            let prev_mode = vp.back_mode;
                            ui.selectable_value(
                                &mut vp.back_mode,
                                BackEquipmentMode::None,
                                app.i18n.tr("Không", "None"),
                            );
                            ui.selectable_value(
                                &mut vp.back_mode,
                                BackEquipmentMode::Cape,
                                app.i18n.tr("Áo choàng (Cape)", "Cape"),
                            );
                            ui.selectable_value(
                                &mut vp.back_mode,
                                BackEquipmentMode::Elytra,
                                app.i18n.tr("Cánh (Elytra)", "Elytra"),
                            );
                            if vp.back_mode != prev_mode {
                                app.accounts.loaded_viewport_key.clear();
                                if vp.back_mode != BackEquipmentMode::None {
                                    vp.reset_camera(true);
                                }
                            }
                        });

                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(
                                &mut vp.show_stage,
                                app.i18n.tr("Bệ 3D Hologram", "3D Hologram Stage"),
                            );
                            ui.checkbox(
                                &mut vp.show_outer_layer,
                                app.i18n.tr("Lớp giáp ngoài", "Outer Layer"),
                            );
                            ui.checkbox(
                                &mut vp.animate_walk,
                                app.i18n.tr("Đi bộ", "Walk Cycle"),
                            );
                            ui.checkbox(
                                &mut vp.auto_rotate,
                                app.i18n.tr("Tự xoay", "Auto Orbit"),
                            );
                        });
                    },
                );
            } else {
                ui.label(app.i18n.tr(
                    "Trình kết xuất GPU wgpu chưa sẵn sàng.",
                    "wgpu 3D renderer is not ready.",
                ));
            }

            if let Some(png_path) = dropped_png {
                if app.selected_account().is_some() {
                    app.apply_account_texture_from_path(TextureSlot::Skin, png_path);
                }
            }

            if let Some(sel_acct) = app.selected_account().cloned() {
                ui.add_space(6.0);
                let cur_variant = sel_acct.skin_variant;
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(app.i18n.tr("Dáng tay:", "Arm Model:")).strong());
                    if ui
                        .selectable_label(
                            cur_variant == SkinVariant::Wide,
                            app.i18n.t("account.model_wide"),
                        )
                        .clicked()
                    {
                        app.update_selected_account_variant(SkinVariant::Wide);
                    }
                    if ui
                        .selectable_label(
                            cur_variant == SkinVariant::Slim,
                            app.i18n.t("account.model_slim"),
                        )
                        .clicked()
                    {
                        app.update_selected_account_variant(SkinVariant::Slim);
                    }
                    if sel_acct.account_type == AccountType::Microsoft {
                        badge(
                            ui,
                            app.i18n.tr("Đồng bộ Mojang Cloud", "Mojang Cloud Sync"),
                            ACCENT_EMERALD,
                        );
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    if primary_icon_button(
                        ui,
                        VectorIcon::Upload,
                        app.i18n.tr("Skin (.png)", "Skin (.png)"),
                    )
                    .clicked()
                    {
                        app.pick_account_texture(TextureSlot::Skin);
                    }
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Upload,
                        app.i18n.tr("Cape (.png)", "Cape (.png)"),
                    )
                    .clicked()
                    {
                        app.pick_account_texture(TextureSlot::Cape);
                    }
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Upload,
                        app.i18n.tr("Elytra (.png)", "Elytra (.png)"),
                    )
                    .clicked()
                    {
                        app.pick_account_texture(TextureSlot::Elytra);
                    }
                    if secondary_icon_button(
                        ui,
                        VectorIcon::Refresh,
                        app.i18n.tr("Mặc định", "Reset"),
                    )
                    .clicked()
                    {
                        app.reset_selected_account_textures();
                    }
                });
            }
        });

    ScrollArea::vertical()
        .id_salt("accounts_left_scroll")
        .show(ui, |ui| {
            ui.heading(RichText::new(app.i18n.t("nav.accounts")).size(20.5).strong());
            ui.add_space(8.0);

            gaming_card_3d(
                ui,
                "acct_ms_oauth_card",
                false,
                ACCENT_EMERALD,
                Margin::same(14),
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(app.i18n.t("account.add_ms"))
                            .size(15.0)
                            .strong()
                            .color(ACCENT_EMERALD),
                    );
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Xác thực trực tiếp Xbox Live → XSTS → Minecraft Services, lưu khóa trong OS Keyring & tự động đồng bộ Skin/Cape bản quyền.",
                            "Direct Xbox Live → XSTS → Minecraft Services OAuth2 with OS Keyring storage & automatic Mojang Skin/Cape cloud sync.",
                        ))
                        .size(12.0)
                        .weak(),
                    );
                    ui.add_space(8.0);

                    ui.horizontal_wrapped(|ui| {
                        if primary_icon_button(
                            ui,
                            VectorIcon::Key,
                            app.i18n.tr(
                                "Đăng nhập Mã Thiết bị (microsoft.com/link)",
                                "Device Code Sign-In (microsoft.com/link)",
                            ),
                        )
                        .clicked()
                        {
                            app.start_ms_device_flow();
                        }
                        if secondary_icon_button(
                            ui,
                            VectorIcon::Globe,
                            app.i18n.tr(
                                "Đăng nhập Trình duyệt (Dán URL)",
                                "Browser Sign-In (Paste Redirect URL)",
                            ),
                        )
                        .clicked()
                        {
                            app.open_ms_desktop_redirect_flow();
                        }
                    });

                    if let Some(sess) = app.accounts.device_session.clone() {
                        ui.add_space(8.0);
                        Frame::popup(ui.style())
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spinner();
                                    ui.label(app.i18n.tr(
                                        "Nhập mã này trên trang Microsoft:",
                                        "Enter this code on Microsoft's page:",
                                    ));
                                    ui.label(
                                        RichText::new(&sess.user_code)
                                            .monospace()
                                            .size(18.0)
                                            .strong()
                                            .color(ACCENT_EMERALD),
                                    );
                                    if secondary_icon_button(
                                        ui,
                                        VectorIcon::Copy,
                                        app.i18n.tr("Sao chép mã", "Copy Code"),
                                    )
                                    .clicked()
                                    {
                                        ui.ctx().copy_text(sess.user_code.clone());
                                    }
                                    if secondary_icon_button(
                                        ui,
                                        VectorIcon::Globe,
                                        app.i18n.tr("Mở trang", "Open Browser"),
                                    )
                                    .clicked()
                                    {
                                        let _ = open::that(&sess.verification_uri);
                                    }
                                });
                            });
                    }

                    if app.accounts.desktop_challenge.is_some() {
                        ui.add_space(8.0);
                        ui.label(app.i18n.tr(
                            "Sau khi đăng nhập trên trình duyệt, hãy sao chép toàn bộ URL thanh địa chỉ (bắt đầu bằng https://login.live.com/oauth20_desktop.srf?code=...) và dán vào đây:",
                            "After signing in on your browser, copy the full address bar URL (starting with https://login.live.com/oauth20_desktop.srf?code=...) and paste it below:",
                        ));
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(
                                    &mut app.accounts.desktop_redirect_input,
                                )
                                .hint_text("https://login.live.com/oauth20_desktop.srf?code=...")
                                .desired_width(300.0),
                            );
                            if primary_button(ui, app.i18n.tr("Xác nhận", "Verify")).clicked() {
                                app.submit_ms_desktop_redirect();
                            }
                        });
                    }

                    ui.add_space(4.0);
                    ui.collapsing(
                        app.i18n.tr(
                            "Nâng cao: Đăng nhập Loopback PKCE với Azure Client ID riêng",
                            "Advanced: Loopback PKCE Sign-In with Custom Azure Client ID",
                        ),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.accounts.custom_client_id)
                                        .hint_text("Azure Application (client) ID")
                                        .desired_width(240.0),
                                );
                                if secondary_icon_button(
                                    ui,
                                    VectorIcon::Key,
                                    app.i18n.tr("Mở cổng Loopback PKCE", "Start Loopback PKCE"),
                                )
                                .clicked()
                                {
                                    app.start_ms_loopback_pkce_flow();
                                }
                            });
                        },
                    );
                },
            );

            ui.add_space(10.0);

            gaming_card_3d(
                ui,
                "acct_offline_ygg_card",
                false,
                ACCENT_AMBER,
                Margin::same(14),
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(app.i18n.t("account.add_offline"))
                            .size(15.0)
                            .strong()
                            .color(ACCENT_AMBER),
                    );
                    ui.label(
                        RichText::new(app.i18n.tr(
                            "Tự động tạo OfflinePlayer UUID v3 chuẩn Mojang và bơm Skin/Cape qua máy chủ Yggdrasil cục bộ.",
                            "Generates Mojang-compatible OfflinePlayer v3 UUIDs and injects custom Skin/Cape via local Yggdrasil authlib-injector.",
                        ))
                        .size(12.0)
                        .weak(),
                    );
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut app.accounts.offline_username)
                                .hint_text(app.i18n.tr(
                                    "Tên nhân vật (3-16 ký tự A-Z, 0-9, _)",
                                    "Username (3-16 chars A-Z, 0-9, _)",
                                ))
                                .desired_width(235.0),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            app.add_offline_account();
                        }
                        ui.selectable_value(
                            &mut app.accounts.offline_variant,
                            SkinVariant::Wide,
                            "Wide",
                        );
                        ui.selectable_value(
                            &mut app.accounts.offline_variant,
                            SkinVariant::Slim,
                            "Slim",
                        );
                        if primary_icon_button(
                            ui,
                            VectorIcon::Plus,
                            app.i18n.tr("Thêm", "Add"),
                        )
                        .clicked()
                        {
                            app.add_offline_account();
                        }
                    });
                },
            );

            ui.add_space(12.0);
            ui.label(
                RichText::new(
                    app.i18n
                        .tr("Danh sách Tài khoản đã lưu", "Saved Accounts"),
                )
                .size(15.0)
                .strong(),
            );
            ui.add_space(6.0);

            let accts = app.accounts.accounts.clone();
            let sel_id = app.accounts.selected_account_id.clone();

            if accts.is_empty() {
                ui.label(
                    RichText::new(
                        app.i18n
                            .tr("Chưa có tài khoản nào.", "No accounts added yet."),
                    )
                    .weak(),
                );
            }

            for acct in accts {
                let is_sel = sel_id.as_deref() == Some(&acct.id);
                let acct_col = match acct.account_type {
                    AccountType::Microsoft => ACCENT_EMERALD,
                    AccountType::Offline => ACCENT_CYAN,
                };

                gaming_card_3d(
                    ui,
                    ("saved_acct_card", &acct.id),
                    is_sel,
                    acct_col,
                    Margin::symmetric(12, 9),
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if ui.radio(is_sel, "").clicked() {
                                app.select_account(acct.id.clone());
                            }
                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(&acct.username).size(14.5).strong());
                                    match acct.account_type {
                                        AccountType::Microsoft => badge(
                                            ui,
                                            app.i18n
                                                .tr("Microsoft Bản quyền", "Microsoft Official"),
                                            ACCENT_EMERALD,
                                        ),
                                        AccountType::Offline => badge(
                                            ui,
                                            app.i18n.tr(
                                                "Offline + Yggdrasil Skin",
                                                "Offline + Yggdrasil Skin",
                                            ),
                                            ACCENT_AMBER,
                                        ),
                                    }
                                    badge(ui, acct.skin_variant.as_str(), ACCENT_BLUE);
                                    if acct.cape_url.is_some() || acct.custom_cape_path.is_some() {
                                        badge(ui, "Cape", ACCENT_VIOLET);
                                    }
                                });
                                ui.label(
                                    RichText::new(format!("UUID: {}", acct.uuid))
                                        .monospace()
                                        .size(11.0)
                                        .weak(),
                                );
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if is_sel
                                        && danger_icon_button(
                                            ui,
                                            VectorIcon::Trash,
                                            app.i18n.t("common.delete"),
                                        )
                                        .clicked()
                                    {
                                        app.delete_selected_account();
                                    }
                                    if is_sel
                                        && acct.account_type == AccountType::Microsoft
                                        && secondary_icon_button(
                                            ui,
                                            VectorIcon::Refresh,
                                            app.i18n.tr("Đồng bộ Token & Skin", "Sync Token & Skin"),
                                        )
                                        .clicked()
                                    {
                                        app.refresh_selected_ms_token();
                                    }
                                    if !is_sel
                                        && primary_button(
                                            ui,
                                            app.i18n.tr("Chọn", "Select"),
                                        )
                                        .clicked()
                                    {
                                        app.select_account(acct.id.clone());
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
