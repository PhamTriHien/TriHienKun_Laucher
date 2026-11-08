#[cfg(not(target_os = "android"))]
use crate::theme::generate_brand_icon_image;
#[cfg(not(target_os = "android"))]
use std::sync::mpsc::{self, Receiver};
use thk_core::models::Language;
#[cfg(not(target_os = "android"))]
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
#[cfg(not(target_os = "android"))]
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[cfg_attr(target_os = "android", allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    RestoreWindow,
    QuitApp,
}

#[cfg(not(target_os = "android"))]
pub struct TrayController {
    _tray: TrayIcon,
    open_item: MenuItem,
    quit_item: MenuItem,
    action_rx: Receiver<TrayAction>,
    lang: Language,
}

#[cfg(target_os = "android")]
pub struct TrayController;

#[cfg(not(target_os = "android"))]
fn tray_labels(lang: Language) -> (&'static str, &'static str) {
    match lang {
        Language::Vi => ("Mở TriHienKun Laucher", "Thoát hoàn toàn"),
        Language::En => ("Open TriHienKun Laucher", "Quit Completely"),
    }
}

#[cfg(not(target_os = "android"))]
impl TrayController {
    pub fn try_new(lang: Language, egui_ctx: egui::Context) -> Option<Self> {
        let rgba_img = generate_brand_icon_image(64);
        let (width, height) = rgba_img.dimensions();
        let icon = Icon::from_rgba(rgba_img.into_raw(), width, height).ok()?;

        let (open_lbl, quit_lbl) = tray_labels(lang);
        let menu = Menu::new();
        let open_item = MenuItem::new(open_lbl, true, None);
        let quit_item = MenuItem::new(quit_lbl, true, None);
        let open_id = open_item.id().clone();
        let quit_id = quit_item.id().clone();

        menu.append(&open_item).ok()?;
        menu.append(&quit_item).ok()?;

        let tray = TrayIconBuilder::new()
            .with_tooltip("TriHienKun Laucher")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .ok()?;

        let (action_tx, action_rx) = mpsc::channel();
        let menu_tx = action_tx.clone();
        let menu_ctx = egui_ctx.clone();
        MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
            let action = if ev.id == open_id {
                Some(TrayAction::RestoreWindow)
            } else if ev.id == quit_id {
                Some(TrayAction::QuitApp)
            } else {
                None
            };
            if let Some(act) = action {
                let _ = menu_tx.send(act);
                menu_ctx.request_repaint();
            }
        }));

        let icon_tx = action_tx;
        let icon_ctx = egui_ctx;
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| match ev {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => {
                let _ = icon_tx.send(TrayAction::RestoreWindow);
                icon_ctx.request_repaint();
            }
            _ => {}
        }));

        Some(Self {
            _tray: tray,
            open_item,
            quit_item,
            action_rx,
            lang,
        })
    }

    pub fn sync_language(&mut self, lang: Language) {
        if self.lang != lang {
            self.lang = lang;
            let (open_lbl, quit_lbl) = tray_labels(lang);
            self.open_item.set_text(open_lbl);
            self.quit_item.set_text(quit_lbl);
        }
    }

    pub fn poll_action(&self) -> Option<TrayAction> {
        let mut latest = None;
        while let Ok(action) = self.action_rx.try_recv() {
            latest = Some(action);
            if action == TrayAction::QuitApp {
                break;
            }
        }
        latest
    }
}

#[cfg(not(target_os = "android"))]
impl Drop for TrayController {
    fn drop(&mut self) {
        MenuEvent::set_event_handler::<fn(MenuEvent)>(None);
        TrayIconEvent::set_event_handler::<fn(TrayIconEvent)>(None);
    }
}

#[cfg(target_os = "android")]
impl TrayController {
    pub fn try_new(_lang: Language, _egui_ctx: egui::Context) -> Option<Self> {
        None
    }

    pub fn sync_language(&mut self, _lang: Language) {}

    pub fn poll_action(&self) -> Option<TrayAction> {
        None
    }
}

pub fn restore_and_focus_viewport(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}
