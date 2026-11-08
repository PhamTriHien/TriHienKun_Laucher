use egui::{
    epaint::Shadow, style::WidgetVisuals, Color32, CornerRadius, FontData, FontDefinitions,
    FontFamily, FontId, IconData, Id, Stroke, TextStyle, Visuals,
};
use image::{imageops::FilterType, Rgba, RgbaImage};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use thk_core::models::{sanitize_font_scale, AccentPreset, ThemeMode};

// Vibrant Game-Engine Semantic Palette (Daylight & Studio Balanced)
pub const ACCENT_EMERALD: Color32 = Color32::from_rgb(20, 184, 102);
pub const ACCENT_EMERALD_GLOW: Color32 = Color32::from_rgb(82, 232, 152);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(14, 165, 233);
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(37, 112, 235);
pub const ACCENT_VIOLET: Color32 = Color32::from_rgb(124, 58, 237);
pub const ACCENT_AMBER: Color32 = Color32::from_rgb(234, 138, 12);
pub const ACCENT_RED: Color32 = Color32::from_rgb(225, 45, 72);
pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(234, 98, 24);

// Bright Sunlit Game-Engine Surfaces (Light Mode - Default)
pub const BG_SKY_LIGHT: Color32 = Color32::from_rgb(230, 239, 250);
pub const BG_PANEL_LIGHT: Color32 = Color32::from_rgb(243, 248, 254);
pub const BG_CARD_LIGHT: Color32 = Color32::from_rgb(252, 254, 255);
pub const BG_SLOT_LIGHT: Color32 = Color32::from_rgb(222, 233, 246);
pub const BORDER_GAME_OUTER_LIGHT: Color32 = Color32::from_rgb(154, 177, 205);
pub const BORDER_GAME_LIP_LIGHT: Color32 = Color32::from_rgb(186, 205, 228);
pub const BORDER_DARK_CONTOUR: Color32 = Color32::from_rgb(27, 40, 56);
pub const TEXT_INK_LIGHT: Color32 = Color32::from_rgb(20, 33, 52);
pub const TEXT_MUTED_LIGHT: Color32 = Color32::from_rgb(74, 95, 122);

// Cobalt Dusk Game-Engine Studio Surfaces (Dark Mode)
pub const BG_VOID_DARK: Color32 = Color32::from_rgb(24, 34, 50);
pub const BG_PANEL_DARK: Color32 = Color32::from_rgb(29, 41, 60);
pub const BG_CARD_DARK: Color32 = Color32::from_rgb(36, 50, 72);
pub const BG_CARD_ELEVATED_DARK: Color32 = Color32::from_rgb(43, 59, 84);
pub const BORDER_SUBTLE_DARK: Color32 = Color32::from_rgb(66, 88, 120);

pub fn accent_palette(preset: AccentPreset) -> (Color32, Color32) {
    match preset {
        AccentPreset::Emerald => (ACCENT_EMERALD, ACCENT_EMERALD_GLOW),
        AccentPreset::Cyan => (ACCENT_CYAN, Color32::from_rgb(103, 218, 255)),
        AccentPreset::Blue => (ACCENT_BLUE, Color32::from_rgb(118, 172, 255)),
        AccentPreset::Violet => (ACCENT_VIOLET, Color32::from_rgb(176, 132, 255)),
        AccentPreset::Amber => (ACCENT_AMBER, Color32::from_rgb(252, 196, 76)),
        AccentPreset::Crimson => (ACCENT_RED, Color32::from_rgb(252, 118, 138)),
    }
}

pub fn active_accent_preset(ctx: &egui::Context) -> AccentPreset {
    ctx.data(|d| d.get_temp::<AccentPreset>(Id::unique("thk_active_accent_preset")))
        .unwrap_or(AccentPreset::Emerald)
}

/// Adjusts an accent hue for crisp WCAG-compliant foreground readability on bright or dark surfaces.
pub fn readable_accent_ink(color: Color32, dark_mode: bool) -> Color32 {
    if dark_mode {
        let r = color.r().saturating_add((255 - color.r()) / 4);
        let g = color.g().saturating_add((255 - color.g()) / 4);
        let b = color.b().saturating_add((255 - color.b()) / 4);
        Color32::from_rgba_unmultiplied(r, g, b, color.a())
    } else {
        let r = (color.r() as u16 * 56 / 100) as u8;
        let g = (color.g() as u16 * 54 / 100) as u8;
        let b = (color.b() as u16 * 60 / 100) as u8;
        Color32::from_rgba_unmultiplied(r, g, b, color.a())
    }
}

pub fn install_unicode_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    let candidates: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Windows\Fonts\seguisb.ttf",
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\arial.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
        ]
    } else if cfg!(target_os = "android") {
        &[
            "/system/fonts/Roboto-Medium.ttf",
            "/system/fonts/Roboto-Regular.ttf",
            "/system/fonts/RobotoStatic-Regular.ttf",
            "/system/fonts/NotoSansCJK-Regular.ttc",
        ]
    } else {
        &[
            "/usr/share/fonts/truetype/noto/NotoSans-Medium.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
        ]
    };

    let mut installed_any = false;
    for path_str in candidates {
        let p = Path::new(path_str);
        if let Ok(bytes) = std::fs::read(p) {
            fonts.font_data.insert(
                "thk_gaming_ui".to_owned(),
                Arc::new(FontData::from_owned(bytes)),
            );
            if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
                prop.insert(0, "thk_gaming_ui".to_owned());
            }
            if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
                mono.push("thk_gaming_ui".to_owned());
            }
            installed_any = true;
            break;
        }
    }

    let mono_candidates: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Windows\Fonts\CascadiaMono.ttf",
            r"C:\Windows\Fonts\consola.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/SFNSMono.ttf",
            "/System/Library/Fonts/Menlo.ttc",
        ]
    } else if cfg!(target_os = "android") {
        &["/system/fonts/DroidSansMono.ttf"]
    } else {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        ]
    };

    for path_str in mono_candidates {
        if let Ok(bytes) = std::fs::read(Path::new(path_str)) {
            fonts.font_data.insert(
                "thk_gaming_mono".to_owned(),
                Arc::new(FontData::from_owned(bytes)),
            );
            if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
                mono.insert(0, "thk_gaming_mono".to_owned());
            }
            installed_any = true;
            break;
        }
    }

    if installed_any {
        ctx.set_fonts(fonts);
    }
}

pub fn base_text_styles() -> BTreeMap<TextStyle, FontId> {
    let mut text_styles = BTreeMap::new();
    text_styles.insert(
        TextStyle::Heading,
        FontId::new(19.5, FontFamily::Proportional),
    );
    text_styles.insert(
        TextStyle::Name("HeroTitle".into()),
        FontId::new(24.0, FontFamily::Proportional),
    );
    text_styles.insert(
        TextStyle::Name("Subheading".into()),
        FontId::new(15.2, FontFamily::Proportional),
    );
    text_styles.insert(TextStyle::Body, FontId::new(13.2, FontFamily::Proportional));
    text_styles.insert(
        TextStyle::Button,
        FontId::new(13.2, FontFamily::Proportional),
    );
    text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.0, FontFamily::Monospace),
    );
    text_styles.insert(
        TextStyle::Small,
        FontId::new(11.2, FontFamily::Proportional),
    );
    text_styles
}

pub fn apply_font_scale(ctx: &egui::Context, scale: f32) {
    let s = sanitize_font_scale(scale);
    ctx.data_mut(|d| d.insert_temp(Id::unique("thk_active_font_scale"), s));
    ctx.set_zoom_factor(s);
}

pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode, accent: AccentPreset) {
    let scale = ctx
        .data(|d| d.get_temp::<f32>(Id::unique("thk_active_font_scale")))
        .unwrap_or(1.0);
    apply_theme_with_scale(ctx, mode, accent, scale);
}

pub fn apply_theme_with_scale(
    ctx: &egui::Context,
    mode: ThemeMode,
    accent: AccentPreset,
    font_scale: f32,
) {
    let clean_scale = sanitize_font_scale(font_scale);
    ctx.data_mut(|d| {
        d.insert_temp(Id::unique("thk_active_accent_preset"), accent);
        d.insert_temp(Id::unique("thk_active_font_scale"), clean_scale);
    });
    ctx.set_zoom_factor(clean_scale);

    let egui_theme = match mode {
        ThemeMode::Dark => egui::Theme::Dark,
        ThemeMode::Light => egui::Theme::Light,
    };
    ctx.set_theme(egui_theme);

    let (primary_accent, glow_accent) = accent_palette(accent);
    let mut style = (*ctx.style_of(egui_theme)).clone();
    style.text_styles = base_text_styles();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.window_margin = egui::Margin::same(16);
    style.spacing.indent = 16.0;
    style.spacing.scroll.bar_width = 7.0;
    style.spacing.scroll.floating = true;
    style.interaction.resize_grab_radius_side = 8.5;
    style.interaction.resize_grab_radius_corner = 10.0;
    style.animation_time = 0.16;

    let cr_small = CornerRadius::same(8);
    let cr_med = CornerRadius::same(10);
    let cr_card = CornerRadius::same(12);

    let mut visuals = match mode {
        ThemeMode::Dark => {
            let mut v = Visuals::dark();
            v.override_text_color = Some(Color32::from_rgb(236, 243, 252));
            v.panel_fill = BG_PANEL_DARK;
            v.window_fill = BG_CARD_DARK;
            v.extreme_bg_color = Color32::from_rgb(19, 27, 41);
            v.faint_bg_color = Color32::from_rgb(33, 46, 67);
            v.code_bg_color = Color32::from_rgb(20, 29, 44);
            v.window_corner_radius = cr_card;
            v.menu_corner_radius = cr_med;
            v.window_stroke = Stroke::new(1.5, Color32::from_rgb(76, 102, 138));
            v.window_shadow = Shadow {
                offset: [0, 6],
                blur: 16,
                spread: 0,
                color: Color32::from_black_alpha(110),
            };
            v.popup_shadow = Shadow {
                offset: [0, 4],
                blur: 10,
                spread: 0,
                color: Color32::from_black_alpha(95),
            };

            v.selection.bg_fill = Color32::from_rgba_unmultiplied(
                primary_accent.r(),
                primary_accent.g(),
                primary_accent.b(),
                72,
            );
            v.selection.stroke = Stroke::new(1.6, glow_accent);
            v.text_cursor.stroke = Stroke::new(2.0, glow_accent);
            v.hyperlink_color = ACCENT_CYAN;
            v.warn_fg_color = ACCENT_AMBER;
            v.error_fg_color = ACCENT_RED;

            v.widgets.noninteractive = WidgetVisuals {
                bg_fill: BG_CARD_DARK,
                weak_bg_fill: BG_PANEL_DARK,
                bg_stroke: Stroke::new(1.2, BORDER_SUBTLE_DARK),
                corner_radius: cr_med,
                fg_stroke: Stroke::new(1.0, Color32::from_rgb(184, 198, 222)),
                expansion: 0.0,
            };
            v.widgets.inactive = WidgetVisuals {
                bg_fill: BG_CARD_ELEVATED_DARK,
                weak_bg_fill: Color32::from_rgb(38, 53, 76),
                bg_stroke: Stroke::new(1.2, Color32::from_rgb(74, 98, 132)),
                corner_radius: cr_small,
                fg_stroke: Stroke::new(1.0, Color32::from_rgb(236, 243, 252)),
                expansion: 0.0,
            };
            v.widgets.hovered = WidgetVisuals {
                bg_fill: Color32::from_rgb(50, 68, 96),
                weak_bg_fill: Color32::from_rgb(45, 62, 88),
                bg_stroke: Stroke::new(1.6, primary_accent),
                corner_radius: cr_small,
                fg_stroke: Stroke::new(1.2, Color32::WHITE),
                expansion: 1.0,
            };
            v.widgets.active = WidgetVisuals {
                bg_fill: primary_accent,
                weak_bg_fill: primary_accent,
                bg_stroke: Stroke::new(1.6, glow_accent),
                corner_radius: cr_small,
                fg_stroke: Stroke::new(1.5, Color32::WHITE),
                expansion: -0.5,
            };
            v.widgets.open = WidgetVisuals {
                bg_fill: Color32::from_rgb(44, 61, 88),
                weak_bg_fill: Color32::from_rgb(40, 56, 80),
                bg_stroke: Stroke::new(1.4, ACCENT_CYAN),
                corner_radius: cr_small,
                fg_stroke: Stroke::new(1.2, Color32::WHITE),
                expansion: 0.0,
            };
            v
        }
        ThemeMode::Light => {
            let mut v = Visuals::light();
            v.override_text_color = Some(TEXT_INK_LIGHT);
            v.panel_fill = BG_PANEL_LIGHT;
            v.window_fill = BG_CARD_LIGHT;
            v.extreme_bg_color = BG_SLOT_LIGHT;
            v.faint_bg_color = Color32::from_rgb(232, 241, 251);
            v.code_bg_color = Color32::from_rgb(225, 236, 248);
            v.window_corner_radius = cr_card;
            v.menu_corner_radius = cr_med;
            v.window_stroke = Stroke::new(1.6, Color32::from_rgb(122, 149, 182));
            v.window_shadow = Shadow {
                offset: [0, 4],
                blur: 10,
                spread: 0,
                color: Color32::from_rgba_unmultiplied(24, 42, 68, 34),
            };
            v.popup_shadow = Shadow {
                offset: [0, 3],
                blur: 8,
                spread: 0,
                color: Color32::from_rgba_unmultiplied(24, 42, 68, 28),
            };

            let ink_accent = readable_accent_ink(primary_accent, false);
            v.selection.bg_fill = Color32::from_rgba_unmultiplied(
                primary_accent.r(),
                primary_accent.g(),
                primary_accent.b(),
                48,
            );
            v.selection.stroke = Stroke::new(1.6, ink_accent);
            v.text_cursor.stroke = Stroke::new(2.0, ink_accent);
            v.hyperlink_color = Color32::from_rgb(3, 105, 161);
            v.warn_fg_color = Color32::from_rgb(180, 83, 9);
            v.error_fg_color = Color32::from_rgb(185, 28, 28);

            v.widgets.noninteractive = WidgetVisuals {
                bg_fill: BG_CARD_LIGHT,
                weak_bg_fill: BG_PANEL_LIGHT,
                bg_stroke: Stroke::new(1.2, BORDER_GAME_LIP_LIGHT),
                corner_radius: cr_med,
                fg_stroke: Stroke::new(1.0, TEXT_MUTED_LIGHT),
                expansion: 0.0,
            };
            v.widgets.inactive = WidgetVisuals {
                bg_fill: Color32::from_rgb(235, 243, 252),
                weak_bg_fill: Color32::from_rgb(228, 238, 249),
                bg_stroke: Stroke::new(1.3, BORDER_GAME_OUTER_LIGHT),
                corner_radius: cr_small,
                fg_stroke: Stroke::new(1.1, TEXT_INK_LIGHT),
                expansion: 0.0,
            };
            v.widgets.hovered = WidgetVisuals {
                bg_fill: Color32::from_rgb(222, 236, 252),
                weak_bg_fill: Color32::from_rgb(216, 232, 250),
                bg_stroke: Stroke::new(1.6, primary_accent),
                corner_radius: cr_small,
                fg_stroke: Stroke::new(1.3, Color32::from_rgb(12, 22, 38)),
                expansion: 1.0,
            };
            v.widgets.active = WidgetVisuals {
                bg_fill: Color32::from_rgb(196, 222, 250),
                weak_bg_fill: Color32::from_rgb(204, 228, 252),
                bg_stroke: Stroke::new(1.6, ink_accent),
                corner_radius: cr_small,
                fg_stroke: Stroke::new(1.4, Color32::from_rgb(12, 22, 38)),
                expansion: -0.5,
            };
            v.widgets.open = WidgetVisuals {
                bg_fill: Color32::from_rgb(219, 234, 250),
                weak_bg_fill: Color32::from_rgb(214, 230, 248),
                bg_stroke: Stroke::new(1.5, primary_accent),
                corner_radius: cr_small,
                fg_stroke: Stroke::new(1.2, TEXT_INK_LIGHT),
                expansion: 0.0,
            };
            v
        }
    };

    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    style.visuals = visuals;
    ctx.set_style_of(egui_theme, style);
}

pub fn generate_brand_icon_image(size: u32) -> RgbaImage {
    if let Ok(decoded) =
        image::load_from_memory(include_bytes!("../../../assets/ui/icon_brand.png"))
    {
        return image::imageops::resize(&decoded.to_rgba8(), size, size, FilterType::Lanczos3);
    }

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
                img.put_pixel(x, y, Rgba([20, 184, 102, alpha]));
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
