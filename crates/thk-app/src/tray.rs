use crate::theme::generate_brand_icon_image;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    RestoreWindow,
    QuitApp,
}

pub struct TrayController {
    _tray: TrayIcon,
    open_id: MenuId,
    quit_id: MenuId,
}

impl TrayController {
    pub fn try_new() -> Option<Self> {
        let rgba_img = generate_brand_icon_image(64);
        let (width, height) = rgba_img.dimensions();
        let icon = Icon::from_rgba(rgba_img.into_raw(), width, height).ok()?;

        let menu = Menu::new();
        let open_item = MenuItem::new("Mở TriHienKun Laucher", true, None);
        let quit_item = MenuItem::new("Thoát hoàn toàn", true, None);
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

        Some(Self {
            _tray: tray,
            open_id,
            quit_id,
        })
    }

    pub fn poll_action(&self) -> Option<TrayAction> {
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == self.open_id {
                return Some(TrayAction::RestoreWindow);
            }
            if ev.id == self.quit_id {
                return Some(TrayAction::QuitApp);
            }
        }

        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            match ev {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => {
                    return Some(TrayAction::RestoreWindow);
                }
                _ => {}
            }
        }

        None
    }
}

pub fn restore_and_focus_viewport(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}
