use egui::epaint::Shadow;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, IconData, Stroke,
    TextStyle, Visuals,
};
use image::{Rgba, RgbaImage};
use std::path::Path;
use std::sync::Arc;
use thk_core::models::{AccentPreset, ThemeMode};

pub const ACCENT_EMERALD: Color32 = Color32::from_rgb(16, 219, 134);
pub const ACCENT_EMERALD_DIM: Color32 = Color32::from_rgb(13, 128, 82);
pub const ACCENT_VIOLET: Color32 = Color32::from_rgb(148, 98, 255);
pub const ACCENT_VIOLET_DIM: Color32 = Color32::from_rgb(76, 29, 149);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(34, 211, 238);
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(56, 152, 255);
pub const ACCENT_AMBER: Color32 = Color32::from_rgb(245, 165, 36);
pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(249, 115, 22);
pub const ACCENT_RED: Color32 = Color32::from_rgb(244, 63, 94);

pub fn accent_palette(preset: AccentPreset) -> (Color32, Color32) {
    match preset {
        AccentPreset::Emerald => (ACCENT_EMERALD, ACCENT_EMERALD_DIM),
        AccentPreset::Cyan => (ACCENT_CYAN, Color32::from_rgb(14, 116, 144)),
        AccentPreset::Blue => (ACCENT_BLUE, Color32::from_rgb(29, 78, 216)),
        AccentPreset::Violet => (ACCENT_VIOLET, ACCENT_VIOLET_DIM),
        AccentPreset::Amber => (ACCENT_AMBER, Color32::from_rgb(180, 83, 9)),
        AccentPreset::Crimson => (ACCENT_RED, Color32::from_rgb(159, 18, 57)),
    }
}

pub const BG_VOID_DARK: Color32 = Color32::from_rgb(9, 11, 20);
pub const BG_HUD_DARK: Color32 = Color32::from_rgb(14, 18, 32);
pub const BG_CARD_DARK: Color32 = Color32::from_rgb(21, 27, 46);
pub const BG_CARD_HOVER_DARK: Color32 = Color32::from_rgb(29, 38, 64);

pub fn install_unicode_fonts(ctx: &egui::Context) {
    let ui_candidates: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Windows\Fonts\seguisb.ttf",
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\arial.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
        ]
    } else {
        &[
            "/usr/share/fonts/truetype/noto/NotoSans-Medium.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ]
    };

    let mono_candidates: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Windows\Fonts\CascadiaMono.ttf",
            r"C:\Windows\Fonts\consola.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        &["/System/Library/Fonts/SFNSMono.ttf"]
    } else {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
        ]
    };

    let mut defs = FontDefinitions::default();
    let mut installed_any = false;

    for path_str in ui_candidates {
        let p = Path::new(path_str);
        if p.is_file() {
            if let Ok(bytes) = std::fs::read(p) {
                defs.font_data.insert(
                    "thk_gaming_ui".to_owned(),
                    Arc::new(FontData::from_owned(bytes)),
                );
                if let Some(prop) = defs.families.get_mut(&FontFamily::Proportional) {
                    prop.insert(0, "thk_gaming_ui".to_owned());
                }
                if let Some(mono) = defs.families.get_mut(&FontFamily::Monospace) {
                    mono.push("thk_gaming_ui".to_owned());
                }
                installed_any = true;
                break;
            }
        }
    }

    for path_str in mono_candidates {
        let p = Path::new(path_str);
        if p.is_file() {
            if let Ok(bytes) = std::fs::read(p) {
                defs.font_data.insert(
                    "thk_gaming_mono".to_owned(),
                    Arc::new(FontData::from_owned(bytes)),
                );
                if let Some(mono) = defs.families.get_mut(&FontFamily::Monospace) {
                    mono.insert(0, "thk_gaming_mono".to_owned());
                }
                installed_any = true;
                break;
            }
        }
    }

    if installed_any {
        ctx.set_fonts(defs);
    }
}

pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode, accent: AccentPreset) {
    let egui_theme = match mode {
        ThemeMode::Dark => egui::Theme::Dark,
        ThemeMode::Light => egui::Theme::Light,
    };
    ctx.set_theme(egui_theme);
    let (acc_bright, acc_dim) = accent_palette(accent);

    let mut style = (*ctx.style_of(egui_theme)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(13.0, 7.5);
    style.spacing.window_margin = egui::Margin::same(16);
    style.spacing.scroll.bar_width = 7.0;
    style.spacing.scroll.floating = true;
    style.animation_time = 0.16;

    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(19.5, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(13.2, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(13.2, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(11.2, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.0, FontFamily::Monospace),
    );

    let mut visuals = match mode {
        ThemeMode::Dark => Visuals::dark(),
        ThemeMode::Light => Visuals::light(),
    };

    let cr_sm = CornerRadius::same(8);
    let cr_md = CornerRadius::same(12);
    visuals.widgets.noninteractive.corner_radius = cr_md;
    visuals.widgets.inactive.corner_radius = cr_sm;
    visuals.widgets.hovered.corner_radius = cr_sm;
    visuals.widgets.active.corner_radius = cr_sm;
    visuals.widgets.open.corner_radius = cr_sm;
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);

    match mode {
        ThemeMode::Dark => {
            let border = Color32::from_rgb(46, 58, 92);
            let text_main = Color32::from_rgb(241, 245, 252);

            visuals.panel_fill = BG_VOID_DARK;
            visuals.window_fill = BG_HUD_DARK;
            visuals.extreme_bg_color = Color32::from_rgb(6, 8, 15);
            visuals.faint_bg_color = BG_CARD_DARK;
            visuals.override_text_color = Some(text_main);
            visuals.hyperlink_color = ACCENT_CYAN;

            visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(
                acc_bright.r(),
                acc_bright.g(),
                acc_bright.b(),
                64,
            );
            visuals.selection.stroke = Stroke::new(1.5, acc_bright);
            visuals.text_cursor.stroke = Stroke::new(2.0, acc_bright);

            visuals.window_shadow = Shadow {
                offset: [0, 12],
                blur: 32,
                spread: 2,
                color: Color32::from_rgba_unmultiplied(2, 4, 12, 195),
            };
            visuals.popup_shadow = Shadow {
                offset: [0, 8],
                blur: 22,
                spread: 0,
                color: Color32::from_rgba_unmultiplied(2, 4, 12, 175),
            };
            visuals.window_stroke = Stroke::new(
                1.2,
                Color32::from_rgba_unmultiplied(acc_bright.r(), acc_bright.g(), acc_bright.b(), 95),
            );

            visuals.widgets.noninteractive.bg_fill = BG_HUD_DARK;
            visuals.widgets.noninteractive.weak_bg_fill = BG_HUD_DARK;
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border);
            visuals.widgets.noninteractive.fg_stroke =
                Stroke::new(1.0, Color32::from_rgb(164, 178, 206));

            visuals.widgets.inactive.bg_fill = BG_CARD_DARK;
            visuals.widgets.inactive.weak_bg_fill = BG_CARD_DARK;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
            visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text_main);

            visuals.widgets.hovered.bg_fill = BG_CARD_HOVER_DARK;
            visuals.widgets.hovered.weak_bg_fill = BG_CARD_HOVER_DARK;
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.4, acc_bright);
            visuals.widgets.hovered.fg_stroke = Stroke::new(1.2, Color32::WHITE);
            visuals.widgets.hovered.expansion = 1.0;

            visuals.widgets.active.bg_fill = acc_dim;
            visuals.widgets.active.weak_bg_fill = acc_dim;
            visuals.widgets.active.bg_stroke = Stroke::new(1.6, acc_bright);
            visuals.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
            visuals.widgets.active.expansion = -0.8;
        }
        ThemeMode::Light => {
            let bg_main = Color32::from_rgb(241, 245, 251);
            let bg_panel = Color32::from_rgb(255, 255, 255);
            let bg_card = Color32::from_rgb(232, 238, 248);
            let bg_hover = Color32::from_rgb(220, 231, 246);
            let border = Color32::from_rgb(194, 207, 226);
            let text_dark = Color32::from_rgb(15, 23, 42);

            visuals.panel_fill = bg_main;
            visuals.window_fill = bg_panel;
            visuals.extreme_bg_color = Color32::from_rgb(248, 250, 253);
            visuals.faint_bg_color = bg_card;
            visuals.override_text_color = Some(text_dark);
            visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(
                acc_bright.r(),
                acc_bright.g(),
                acc_bright.b(),
                52,
            );
            visuals.selection.stroke = Stroke::new(1.5, acc_dim);
            visuals.text_cursor.stroke = Stroke::new(2.0, acc_dim);

            visuals.window_shadow = Shadow {
                offset: [0, 10],
                blur: 26,
                spread: 0,
                color: Color32::from_rgba_unmultiplied(15, 23, 42, 42),
            };
            visuals.popup_shadow = Shadow {
                offset: [0, 6],
                blur: 18,
                spread: 0,
                color: Color32::from_rgba_unmultiplied(15, 23, 42, 34),
            };

            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border);
            visuals.widgets.inactive.bg_fill = bg_card;
            visuals.widgets.inactive.weak_bg_fill = bg_card;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
            visuals.widgets.hovered.bg_fill = bg_hover;
            visuals.widgets.hovered.weak_bg_fill = bg_hover;
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.3, acc_dim);
            visuals.widgets.hovered.expansion = 1.0;
            visuals.widgets.active.expansion = -0.8;
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
                let mut r = (12.0 + grad * 14.0) as u8;
                let mut g = (16.0 + (1.0 - grad) * 20.0) as u8;
                let mut b = (30.0 + grad * 22.0) as u8;

                let nx = (fx - center) / radius;
                let ny = (fy - center) / radius;

                // Isometric 3D voxel diamond faces + glowing "T" emblem
                let iso_top = (-0.62..=-0.04).contains(&ny)
                    && (nx.abs() * 0.58 + (ny + 0.33).abs()) <= 0.30;
                let in_t_top = (-0.50..=-0.22).contains(&ny) && nx.abs() <= 0.52;
                let in_t_stem = (-0.22..=0.58).contains(&ny) && nx.abs() <= 0.19;
                let in_border = dist > -2.4;

                if in_t_top {
                    r = 52;
                    g = 235;
                    b = 162;
                } else if in_t_stem {
                    r = 16;
                    g = 219;
                    b = 134;
                } else if iso_top {
                    r = 139;
                    g = 92;
                    b = 246;
                } else if in_border {
                    r = 20;
                    g = 184;
                    b = 118;
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
