use crate::state::{LauncherAppState, TextureSlot};
use crate::theme::{ACCENT_AMBER, ACCENT_BLUE, ACCENT_EMERALD};
use crate::views::{badge, danger_button, primary_button};
use egui::{Frame, Margin, RichText, ScrollArea, Vec2};
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
        .default_size(395.0)
        .size_range(330.0..=520.0)
        .frame(Frame::NONE.inner_margin(Margin {
            left: 14,
            right: 0,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.heading(RichText::new("Mô hình Nhân vật 3D (wgpu)").size(17.5).strong());
            ui.label(
                RichText::new("Kéo chuột để xoay 360° • Cuộn để thu phóng • Nhấp đúp để đặt lại góc nhìn")
                    .size(11.5)
                    .weak(),
            );
            ui.add_space(6.0);

            if let (Some(rs), Some(vp)) = (render_state, app.accounts.skin_viewport.as_mut()) {
                let canvas_h = (ui.available_height() - 185.0).clamp(240.0, 440.0);
                vp.show(ui, rs, Vec2::new(ui.available_width(), canvas_h));

                ui.add_space(8.0);
                Frame::group(ui.style())
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Trang bị lưng:").strong());
                            let prev_mode = vp.back_mode;
                            ui.selectable_value(&mut vp.back_mode, BackEquipmentMode::None, "Không");
                            ui.selectable_value(
                                &mut vp.back_mode,
                                BackEquipmentMode::Cape,
                                "Áo choàng (Cape)",
                            );
                            ui.selectable_value(
                                &mut vp.back_mode,
                                BackEquipmentMode::Elytra,
                                "Cánh (Elytra)",
                            );
                            if vp.back_mode != prev_mode {
                                app.accounts.loaded_viewport_key.clear();
                                if vp.back_mode != BackEquipmentMode::None {
                                    vp.reset_camera(true);
                                }
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.checkbox(&mut vp.show_outer_layer, "Lớp giáp ngoài (Outer Layer)");
                            ui.checkbox(&mut vp.animate_walk, "Đi bộ");
                            ui.checkbox(&mut vp.auto_rotate, "Tự xoay");
                        });
                    });
            } else {
                ui.label("Trình kết xuất GPU wgpu chưa sẵn sàng.");
            }

            if app.selected_account().is_some() {
                ui.add_space(6.0);
                let cur_variant = app
                    .selected_account()
                    .map(|a| a.skin_variant)
                    .unwrap_or(SkinVariant::Wide);
                ui.horizontal(|ui| {
                    ui.label("Dáng tay:");
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
                });

                ui.horizontal_wrapped(|ui| {
                    if primary_button(ui, "👕 Chọn Skin (.png)").clicked() {
                        app.pick_account_texture(TextureSlot::Skin);
                    }
                    if ui.button("🧣 Chọn Cape (.png)").clicked() {
                        app.pick_account_texture(TextureSlot::Cape);
                    }
                    if ui.button("🪽 Chọn Elytra (.png)").clicked() {
                        app.pick_account_texture(TextureSlot::Elytra);
                    }
                });
            }
        });

    ScrollArea::vertical()
        .id_salt("accounts_left_scroll")
        .show(ui, |ui| {
            ui.heading(RichText::new("Quản lý Tài khoản").size(20.0).strong());
            ui.add_space(8.0);

            Frame::group(ui.style())
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("🔐  {}", app.i18n.t("account.add_ms")))
                            .size(14.5)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(
                            "Xác thực trực tiếp Xbox Live → XSTS → Minecraft Services và lưu khóa an toàn trong OS Keychain.",
                        )
                        .size(12.0)
                        .weak(),
                    );
                    ui.add_space(6.0);

                    ui.horizontal_wrapped(|ui| {
                        if primary_button(ui, "🔑 Đăng nhập bằng Mã Thiết bị (microsoft.com/link)")
                            .clicked()
                        {
                            app.start_ms_device_flow();
                        }
                        if ui.button("🌐 Đăng nhập Trình duyệt (Dán URL)").clicked() {
                            app.open_ms_desktop_redirect_flow();
                        }
                    });

                    if let Some(sess) = app.accounts.device_session.clone() {
                        ui.add_space(8.0);
                        Frame::popup(ui.style())
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Nhập mã này trên trang Microsoft:");
                                    ui.label(
                                        RichText::new(&sess.user_code)
                                            .monospace()
                                            .size(18.0)
                                            .strong()
                                            .color(ACCENT_EMERALD),
                                    );
                                    if ui.button("📋 Sao chép mã").clicked() {
                                        ui.ctx().copy_text(sess.user_code.clone());
                                    }
                                    if ui.button("Mở trang").clicked() {
                                        let _ = open::that(&sess.verification_uri);
                                    }
                                });
                            });
                    }

                    if app.accounts.desktop_challenge.is_some() {
                        ui.add_space(8.0);
                        ui.label(
                            "Sau khi đăng nhập trên trình duyệt, hãy sao chép toàn bộ URL thanh địa chỉ (bắt đầu bằng https://login.live.com/oauth20_desktop.srf?code=...) và dán vào đây:",
                        );
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut app.accounts.desktop_redirect_input)
                                    .hint_text("https://login.live.com/oauth20_desktop.srf?code=...")
                                    .desired_width(310.0),
                            );
                            if primary_button(ui, "Xác nhận").clicked() {
                                app.submit_ms_desktop_redirect();
                            }
                        });
                    }

                    ui.collapsing("Nâng cao: Đăng nhập Loopback PKCE với Azure Client ID riêng", |ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut app.accounts.custom_client_id)
                                    .hint_text("Azure Application (client) ID")
                                    .desired_width(250.0),
                            );
                            if ui.button("Mở cổng Loopback PKCE").clicked() {
                                app.start_ms_loopback_pkce_flow();
                            }
                        });
                    });
                });

            ui.add_space(10.0);

            Frame::group(ui.style())
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("🎮  {}", app.i18n.t("account.add_offline")))
                            .size(14.5)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(
                            "Tự động tạo OfflinePlayer UUID v3 chuẩn Mojang và bơm Skin/Cape qua máy chủ Yggdrasil cục bộ.",
                        )
                        .size(12.0)
                        .weak(),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut app.accounts.offline_username)
                                .hint_text("Tên nhân vật (3-16 ký tự A-Z, 0-9, _)")
                                .desired_width(230.0),
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
                        if primary_button(ui, "➕ Thêm").clicked() {
                            app.add_offline_account();
                        }
                    });
                });

            ui.add_space(12.0);
            ui.label(RichText::new("Danh sách Tài khoản đã lưu").size(15.0).strong());
            ui.add_space(4.0);

            let accts = app.accounts.accounts.clone();
            let sel_id = app.accounts.selected_account_id.clone();

            if accts.is_empty() {
                ui.label(RichText::new("Chưa có tài khoản nào.").weak());
            }

            for acct in accts {
                let is_sel = sel_id.as_deref() == Some(&acct.id);
                Frame::group(ui.style())
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.radio(is_sel, "").clicked() {
                                app.select_account(acct.id.clone());
                            }
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&acct.username).size(14.5).strong());
                                    match acct.account_type {
                                        AccountType::Microsoft => {
                                            badge(ui, "Microsoft Bản quyền", ACCENT_EMERALD)
                                        }
                                        AccountType::Offline => {
                                            badge(ui, "Offline + Yggdrasil Skin", ACCENT_AMBER)
                                        }
                                    }
                                    badge(ui, acct.skin_variant.as_str(), ACCENT_BLUE);
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
                                    if is_sel && danger_button(ui, "Xóa").clicked() {
                                        app.delete_selected_account();
                                    }
                                    if is_sel
                                        && acct.account_type == AccountType::Microsoft
                                        && ui.button("🔄 Làm mới Token").clicked()
                                    {
                                        app.refresh_selected_ms_token();
                                    }
                                    if !is_sel && ui.button("Chọn").clicked() {
                                        app.select_account(acct.id.clone());
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(4.0);
            }
        });
}
