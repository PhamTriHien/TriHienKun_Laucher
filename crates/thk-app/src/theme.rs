use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, IconData, Stroke, Visuals};
use image::{Rgba, RgbaImage};
use std::path::Path;
use std::sync::Arc;
use thk_core::models::ThemeMode;

pub const ACCENT_EMERALD: Color32 = Color32::from_rgb(34, 197, 94);
pub const ACCENT_EMERALD_DIM: Color32 = Color32::from_rgb(21, 128, 61);
pub const ACCENT_AMBER: Color32 = Color32::from_rgb(245, 158, 11);
pub const ACCENT_RED: Color32 = Color32::from_rgb(239, 68, 68);
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(59, 130, 246);

pub fn install_unicode_fonts(ctx: &egui::Context) {
    let candidates: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\arial.ttf",
            r"C:\Windows\Fonts\tahoma.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
        ]
    } else {
        &[
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ]
    };

    let mut font_bytes: Option<Vec<u8>> = None;
    for path_str in candidates {
        let p = Path::new(path_str);
        if p.is_file() {
            if let Ok(bytes) = std::fs::read(p) {
                font_bytes = Some(bytes);
                break;
            }
        }
    }

    let Some(bytes) = font_bytes else {
        return;
    };

    let mut defs = FontDefinitions::default();
    defs.font_data.insert(
        "system_ui_vn".to_owned(),
        Arc::new(FontData::from_owned(bytes)),
    );
    if let Some(prop) = defs.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "system_ui_vn".to_owned());
    }
    if let Some(mono) = defs.families.get_mut(&FontFamily::Monospace) {
        mono.push("system_ui_vn".to_owned());
    }
    ctx.set_fonts(defs);
}

pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode) {
    let egui_theme = match mode {
        ThemeMode::Dark => egui::Theme::Dark,
        ThemeMode::Light => egui::Theme::Light,
    };
    ctx.set_theme(egui_theme);

    let mut style = (*ctx.style_of(egui_theme)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.window_margin = egui::Margin::same(14);

    let mut visuals = match mode {
        ThemeMode::Dark => Visuals::dark(),
        ThemeMode::Light => Visuals::light(),
    };

    let cr_sm = CornerRadius::same(7);
    let cr_md = CornerRadius::same(10);
    visuals.widgets.noninteractive.corner_radius = cr_md;
    visuals.widgets.inactive.corner_radius = cr_sm;
    visuals.widgets.hovered.corner_radius = cr_sm;
    visuals.widgets.active.corner_radius = cr_sm;
    visuals.widgets.open.corner_radius = cr_sm;
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = cr_sm;

    match mode {
        ThemeMode::Dark => {
            let bg_deep = Color32::from_rgb(13, 17, 23);
            let bg_panel = Color32::from_rgb(20, 26, 35);
            let bg_card = Color32::from_rgb(27, 35, 48);
            let bg_hover = Color32::from_rgb(36, 47, 64);
            let border = Color32::from_rgb(44, 58, 78);
            let text_main = Color32::from_rgb(236, 242, 248);

            visuals.panel_fill = bg_deep;
            visuals.window_fill = bg_panel;
            visuals.extreme_bg_color = Color32::from_rgb(9, 12, 17);
            visuals.faint_bg_color = bg_panel;
            visuals.override_text_color = Some(text_main);
            visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(34, 197, 94, 70);
            visuals.selection.stroke = Stroke::new(1.5, ACCENT_EMERALD);

            visuals.widgets.noninteractive.bg_fill = bg_panel;
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border);
            visuals.widgets.inactive.bg_fill = bg_card;
            visuals.widgets.inactive.weak_bg_fill = bg_card;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
            visuals.widgets.hovered.bg_fill = bg_hover;
            visuals.widgets.hovered.weak_bg_fill = bg_hover;
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_EMERALD);
            visuals.widgets.active.bg_fill = ACCENT_EMERALD_DIM;
            visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT_EMERALD);
        }
        ThemeMode::Light => {
            let bg_main = Color32::from_rgb(244, 247, 251);
            let bg_panel = Color32::from_rgb(255, 255, 255);
            let bg_card = Color32::from_rgb(235, 241, 248);
            let border = Color32::from_rgb(203, 213, 225);

            visuals.panel_fill = bg_main;
            visuals.window_fill = bg_panel;
            visuals.faint_bg_color = bg_card;
            visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(22, 163, 74, 55);
            visuals.selection.stroke = Stroke::new(1.5, Color32::from_rgb(22, 163, 74));
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border);
            visuals.widgets.inactive.bg_fill = bg_card;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
        }
    }

    style.visuals = visuals;
    ctx.set_style_of(egui_theme, style);
}

pub fn generate_brand_icon_image(size: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 0]));
    let s = size as f32;
    let center = s * 0.5;
    let radius = s * 0.45;

    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            let dx = (fx - center).abs();
            let dy = (fy - center).abs();
            let corner_r = s * 0.22;
            let qx = (dx - (radius - corner_r)).max(0.0);
            let qy = (dy - (radius - corner_r)).max(0.0);
            let dist = (qx * qx + qy * qy).sqrt() - corner_r;

            if dist <= 0.8 {
                let alpha = ((0.8 - dist).clamp(0.0, 1.0) * 255.0) as u8;
                let grad = (fy / s).clamp(0.0, 1.0);
                let mut r = (16.0 + grad * 10.0) as u8;
                let mut g = (26.0 + (1.0 - grad) * 24.0) as u8;
                let mut b = (36.0 + grad * 14.0) as u8;

                // Emerald top grass band + stylized "T" voxel emblem
                let nx = (fx - center) / radius;
                let ny = (fy - center) / radius;

                let in_t_top = (-0.52..=-0.22).contains(&ny) && nx.abs() <= 0.54;
                let in_t_stem = (-0.22..=0.58).contains(&ny) && nx.abs() <= 0.20;
                let in_border = dist > -2.4;

                if in_t_top {
                    r = 52;
                    g = 211;
                    b = 153;
                } else if in_t_stem {
                    r = 34;
                    g = 197;
                    b = 94;
                } else if in_border {
                    r = 22;
                    g = 163;
                    b = 74;
                }

                img.put_pixel(x, y, Rgba([r, g, b, alpha]));
            }
        }
    }
    img
}

pub fn build_window_icon_data() -> IconData {
    let img = generate_brand_icon_image(64);
    let (width, height) = img.dimensions();
    IconData {
        rgba: img.into_raw(),
        width,
        height,
    }
}

pub fn write_icon_assets_if_missing(icons_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(icons_dir)?;
    let png_path = icons_dir.join("icon.png");
    let ico_path = icons_dir.join("icon.ico");
    if !png_path.exists() || !ico_path.exists() {
        let hi_res = generate_brand_icon_image(256);
        let _ = hi_res.save(&png_path);
        let ico_img = generate_brand_icon_image(64);
        let _ = ico_img.save(&ico_path);
    }
    Ok(())
}
