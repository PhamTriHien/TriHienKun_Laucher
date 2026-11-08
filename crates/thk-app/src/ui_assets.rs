use egui::{
    epaint::{Mesh, Vertex},
    pos2, vec2, Color32, ColorImage, Context, Id, Pos2, Rect, TextureHandle, TextureOptions,
};
use glam::{Mat3, Vec3};
use std::sync::{Arc, OnceLock};
use thk_core::models::AccentPreset;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiButtonPlate {
    Emerald,
    Sky,
    Amber,
    Ruby,
    Cerulean,
    Violet,
}

impl UiButtonPlate {
    pub fn for_accent(preset: AccentPreset) -> Self {
        match preset {
            AccentPreset::Emerald => Self::Emerald,
            AccentPreset::Cyan | AccentPreset::Blue => Self::Cerulean,
            AccentPreset::Violet => Self::Violet,
            AccentPreset::Amber => Self::Amber,
            AccentPreset::Crimson => Self::Ruby,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiAssetIcon {
    Brand,
    Home,
    Modpacks,
    Servers,
    Accounts,
    Java,
    Lan,
    Settings,
    Play,
    MultiWindow,
    Gamepad,
    Rotate,
    Folder,
    Download,
    Refresh,
    Trash,
}

struct DecodedUiSprites {
    btn_emerald: ColorImage,
    btn_sky: ColorImage,
    btn_amber: ColorImage,
    btn_ruby: ColorImage,
    btn_cerulean: ColorImage,
    btn_violet: ColorImage,
    icon_brand: ColorImage,
    icon_home: ColorImage,
    icon_modpacks: ColorImage,
    icon_servers: ColorImage,
    icon_accounts: ColorImage,
    icon_java: ColorImage,
    icon_lan: ColorImage,
    icon_settings: ColorImage,
    icon_play: ColorImage,
    icon_multiwindow: ColorImage,
    icon_gamepad: ColorImage,
    icon_rotate: ColorImage,
    icon_folder: ColorImage,
    icon_download: ColorImage,
    icon_refresh: ColorImage,
    icon_trash: ColorImage,
}

pub struct LoadedUiTextures {
    btn_emerald: TextureHandle,
    btn_sky: TextureHandle,
    btn_amber: TextureHandle,
    btn_ruby: TextureHandle,
    btn_cerulean: TextureHandle,
    btn_violet: TextureHandle,
    icon_brand: TextureHandle,
    icon_home: TextureHandle,
    icon_modpacks: TextureHandle,
    icon_servers: TextureHandle,
    icon_accounts: TextureHandle,
    icon_java: TextureHandle,
    icon_lan: TextureHandle,
    icon_settings: TextureHandle,
    icon_play: TextureHandle,
    icon_multiwindow: TextureHandle,
    icon_gamepad: TextureHandle,
    icon_rotate: TextureHandle,
    icon_folder: TextureHandle,
    icon_download: TextureHandle,
    icon_refresh: TextureHandle,
    icon_trash: TextureHandle,
}

impl LoadedUiTextures {
    pub fn button(&self, plate: UiButtonPlate) -> &TextureHandle {
        match plate {
            UiButtonPlate::Emerald => &self.btn_emerald,
            UiButtonPlate::Sky => &self.btn_sky,
            UiButtonPlate::Amber => &self.btn_amber,
            UiButtonPlate::Ruby => &self.btn_ruby,
            UiButtonPlate::Cerulean => &self.btn_cerulean,
            UiButtonPlate::Violet => &self.btn_violet,
        }
    }

    pub fn icon(&self, kind: UiAssetIcon) -> &TextureHandle {
        match kind {
            UiAssetIcon::Brand => &self.icon_brand,
            UiAssetIcon::Home => &self.icon_home,
            UiAssetIcon::Modpacks => &self.icon_modpacks,
            UiAssetIcon::Servers => &self.icon_servers,
            UiAssetIcon::Accounts => &self.icon_accounts,
            UiAssetIcon::Java => &self.icon_java,
            UiAssetIcon::Lan => &self.icon_lan,
            UiAssetIcon::Settings => &self.icon_settings,
            UiAssetIcon::Play => &self.icon_play,
            UiAssetIcon::MultiWindow => &self.icon_multiwindow,
            UiAssetIcon::Gamepad => &self.icon_gamepad,
            UiAssetIcon::Rotate => &self.icon_rotate,
            UiAssetIcon::Folder => &self.icon_folder,
            UiAssetIcon::Download => &self.icon_download,
            UiAssetIcon::Refresh => &self.icon_refresh,
            UiAssetIcon::Trash => &self.icon_trash,
        }
    }
}

fn decode_png_rgba(bytes: &[u8]) -> ColorImage {
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let dims = [rgba.width() as usize, rgba.height() as usize];
            ColorImage::from_rgba_unmultiplied(dims, rgba.as_raw())
        }
        Err(_) => ColorImage::filled([4, 4], Color32::TRANSPARENT),
    }
}

fn decoded_sprites() -> &'static DecodedUiSprites {
    static DECODED: OnceLock<DecodedUiSprites> = OnceLock::new();
    DECODED.get_or_init(|| DecodedUiSprites {
        btn_emerald: decode_png_rgba(include_bytes!("../../../assets/ui/btn_emerald.png")),
        btn_sky: decode_png_rgba(include_bytes!("../../../assets/ui/btn_sky.png")),
        btn_amber: decode_png_rgba(include_bytes!("../../../assets/ui/btn_amber.png")),
        btn_ruby: decode_png_rgba(include_bytes!("../../../assets/ui/btn_ruby.png")),
        btn_cerulean: decode_png_rgba(include_bytes!("../../../assets/ui/btn_cerulean.png")),
        btn_violet: decode_png_rgba(include_bytes!("../../../assets/ui/btn_violet.png")),
        icon_brand: decode_png_rgba(include_bytes!("../../../assets/ui/icon_brand.png")),
        icon_home: decode_png_rgba(include_bytes!("../../../assets/ui/icon_home.png")),
        icon_modpacks: decode_png_rgba(include_bytes!("../../../assets/ui/icon_modpacks.png")),
        icon_servers: decode_png_rgba(include_bytes!("../../../assets/ui/icon_servers.png")),
        icon_accounts: decode_png_rgba(include_bytes!("../../../assets/ui/icon_accounts.png")),
        icon_java: decode_png_rgba(include_bytes!("../../../assets/ui/icon_java.png")),
        icon_lan: decode_png_rgba(include_bytes!("../../../assets/ui/icon_lan.png")),
        icon_settings: decode_png_rgba(include_bytes!("../../../assets/ui/icon_settings.png")),
        icon_play: decode_png_rgba(include_bytes!("../../../assets/ui/icon_play.png")),
        icon_multiwindow: decode_png_rgba(include_bytes!(
            "../../../assets/ui/icon_multiwindow.png"
        )),
        icon_gamepad: decode_png_rgba(include_bytes!("../../../assets/ui/icon_gamepad.png")),
        icon_rotate: decode_png_rgba(include_bytes!("../../../assets/ui/icon_rotate.png")),
        icon_folder: decode_png_rgba(include_bytes!("../../../assets/ui/icon_folder.png")),
        icon_download: decode_png_rgba(include_bytes!("../../../assets/ui/icon_download.png")),
        icon_refresh: decode_png_rgba(include_bytes!("../../../assets/ui/icon_refresh.png")),
        icon_trash: decode_png_rgba(include_bytes!("../../../assets/ui/icon_trash.png")),
    })
}

pub fn ui_textures(ctx: &Context) -> Arc<LoadedUiTextures> {
    let cache_id = Id::unique("thk_loaded_ui_textures_v1");
    if let Some(existing) = ctx.data_mut(|d| d.get_temp::<Arc<LoadedUiTextures>>(cache_id)) {
        return existing;
    }

    let s = decoded_sprites();
    let opts = TextureOptions::LINEAR;
    let upload = |name: &str, img: &ColorImage| ctx.load_texture(name, img.clone(), opts);

    let loaded = Arc::new(LoadedUiTextures {
        btn_emerald: upload("thk_ui_btn_emerald", &s.btn_emerald),
        btn_sky: upload("thk_ui_btn_sky", &s.btn_sky),
        btn_amber: upload("thk_ui_btn_amber", &s.btn_amber),
        btn_ruby: upload("thk_ui_btn_ruby", &s.btn_ruby),
        btn_cerulean: upload("thk_ui_btn_cerulean", &s.btn_cerulean),
        btn_violet: upload("thk_ui_btn_violet", &s.btn_violet),
        icon_brand: upload("thk_ui_icon_brand", &s.icon_brand),
        icon_home: upload("thk_ui_icon_home", &s.icon_home),
        icon_modpacks: upload("thk_ui_icon_modpacks", &s.icon_modpacks),
        icon_servers: upload("thk_ui_icon_servers", &s.icon_servers),
        icon_accounts: upload("thk_ui_icon_accounts", &s.icon_accounts),
        icon_java: upload("thk_ui_icon_java", &s.icon_java),
        icon_lan: upload("thk_ui_icon_lan", &s.icon_lan),
        icon_settings: upload("thk_ui_icon_settings", &s.icon_settings),
        icon_play: upload("thk_ui_icon_play", &s.icon_play),
        icon_multiwindow: upload("thk_ui_icon_multiwindow", &s.icon_multiwindow),
        icon_gamepad: upload("thk_ui_icon_gamepad", &s.icon_gamepad),
        icon_rotate: upload("thk_ui_icon_rotate", &s.icon_rotate),
        icon_folder: upload("thk_ui_icon_folder", &s.icon_folder),
        icon_download: upload("thk_ui_icon_download", &s.icon_download),
        icon_refresh: upload("thk_ui_icon_refresh", &s.icon_refresh),
        icon_trash: upload("thk_ui_icon_trash", &s.icon_trash),
    });

    ctx.data_mut(|d| d.insert_temp(cache_id, loaded.clone()));
    loaded
}

/// Renders a 9-slice scaled tactile game-engine button plate (`240x64` source) with zero corner distortion.
pub fn paint_nine_slice_button(
    painter: &egui::Painter,
    rect: Rect,
    plate: UiButtonPlate,
    tint: Color32,
) {
    if rect.width() < 4.0 || rect.height() < 4.0 {
        return;
    }
    let atlas = ui_textures(painter.ctx());
    let tex = atlas.button(plate);

    // Source 240x64: left/right corner bevel = 28px, top rim = 20px, bottom 3D lip = 22px
    let u = [0.0_f32, 28.0 / 240.0, (240.0 - 28.0) / 240.0, 1.0];
    let v = [0.0_f32, 20.0 / 64.0, (64.0 - 22.0) / 64.0, 1.0];

    let scale = (rect.height() / 44.0).clamp(0.42, 1.32);
    let cap_x = (20.0 * scale).min(rect.width() * 0.45);
    let cap_top = (13.5 * scale).min(rect.height() * 0.42);
    let cap_bot = (15.0 * scale).min(rect.height() * 0.45);

    let xs = [
        rect.left(),
        rect.left() + cap_x,
        rect.right() - cap_x,
        rect.right(),
    ];
    let ys = [
        rect.top(),
        rect.top() + cap_top,
        rect.bottom() - cap_bot,
        rect.bottom(),
    ];

    let mut mesh = Mesh::with_texture(tex.id());
    mesh.vertices.reserve(16);
    mesh.indices.reserve(54);

    for ry in 0..4 {
        for rx in 0..4 {
            mesh.vertices.push(Vertex {
                pos: pos2(xs[rx], ys[ry]),
                uv: pos2(u[rx], v[ry]),
                color: tint,
            });
        }
    }

    for ry in 0..3_u32 {
        for rx in 0..3_u32 {
            let i00 = ry * 4 + rx;
            let i10 = i00 + 1;
            let i01 = i00 + 4;
            let i11 = i01 + 1;
            mesh.indices
                .extend_from_slice(&[i00, i10, i11, i00, i11, i01]);
        }
    }

    painter.add(egui::Shape::mesh(mesh));
}

/// Projects an AI-crafted stylized 3D icon onto a real-time 3D perspective-tilted quad with floating motion.
pub fn paint_tilted_ai_icon(
    painter: &egui::Painter,
    rect: Rect,
    icon: UiAssetIcon,
    pitch_rad: f32,
    yaw_rad: f32,
    bob_px: f32,
    tint: Color32,
) {
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let atlas = ui_textures(painter.ctx());
    let tex = atlas.icon(icon);

    let center = rect.center() + vec2(0.0, bob_px);
    let half_w = rect.width() * 0.5;
    let half_h = rect.height() * 0.5;

    let rot = Mat3::from_euler(glam::EulerRot::YXZ, yaw_rad, pitch_rad, 0.0);
    let cam_dist = 3.6_f32;

    let project_corner = |lx: f32, ly: f32| -> Pos2 {
        let p = rot * Vec3::new(lx, ly, 0.0);
        let persp = cam_dist / (cam_dist + p.z).max(1.2);
        pos2(
            center.x + p.x * half_w * persp,
            center.y + p.y * half_h * persp,
        )
    };

    let p00 = project_corner(-1.0, -1.0);
    let p10 = project_corner(1.0, -1.0);
    let p11 = project_corner(1.0, 1.0);
    let p01 = project_corner(-1.0, 1.0);

    let mut mesh = Mesh::with_texture(tex.id());
    mesh.vertices.extend_from_slice(&[
        Vertex {
            pos: p00,
            uv: pos2(0.0, 0.0),
            color: tint,
        },
        Vertex {
            pos: p10,
            uv: pos2(1.0, 0.0),
            color: tint,
        },
        Vertex {
            pos: p11,
            uv: pos2(1.0, 1.0),
            color: tint,
        },
        Vertex {
            pos: p01,
            uv: pos2(0.0, 1.0),
            color: tint,
        },
    ]);
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}
