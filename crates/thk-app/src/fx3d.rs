#[cfg(test)]
use crate::theme::ACCENT_ORANGE;
use crate::theme::{
    accent_palette, active_accent_preset, ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD,
    BG_CARD_DARK, BG_CARD_ELEVATED_DARK, BG_CARD_LIGHT, BG_SLOT_LIGHT, BORDER_GAME_LIP_LIGHT,
    BORDER_GAME_OUTER_LIGHT, TEXT_INK_LIGHT,
};
use crate::ui_assets::{paint_nine_slice_button, paint_tilted_ai_icon, UiAssetIcon, UiButtonPlate};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, Response, Sense, Shape,
    Stroke, StrokeKind, Ui, Vec2,
};
use glam::{Mat4, Vec3};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static LOW_RAM_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_low_ram_mode(enabled: bool) {
    LOW_RAM_MODE.store(enabled, Ordering::Relaxed);
}

pub fn is_low_ram_mode() -> bool {
    LOW_RAM_MODE.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon3dKind {
    BrandVoxelCore,
    HomePortal,
    ModpackCrate,
    ServerBladeTower,
    AccountCharacter,
    JavaReactor,
    LanRadar,
    SettingsGyro,
    LaunchPrism,
    #[cfg_attr(target_os = "android", allow(dead_code))]
    MultiWindow,
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    MobileGamepad,
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    ScreenRotate,
}

impl Icon3dKind {
    pub fn ai_asset(self) -> UiAssetIcon {
        match self {
            Self::BrandVoxelCore => UiAssetIcon::Brand,
            Self::HomePortal => UiAssetIcon::Home,
            Self::ModpackCrate => UiAssetIcon::Modpacks,
            Self::ServerBladeTower => UiAssetIcon::Servers,
            Self::AccountCharacter => UiAssetIcon::Accounts,
            Self::JavaReactor => UiAssetIcon::Java,
            Self::LanRadar => UiAssetIcon::Lan,
            Self::SettingsGyro => UiAssetIcon::Settings,
            Self::LaunchPrism => UiAssetIcon::Play,
            Self::MultiWindow => UiAssetIcon::MultiWindow,
            Self::MobileGamepad => UiAssetIcon::Gamepad,
            Self::ScreenRotate => UiAssetIcon::Rotate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorIcon {
    Play,
    Stop,
    Plus,
    Download,
    Upload,
    Folder,
    Search,
    Refresh,
    Trash,
    Save,
    Copy,
    Shield,
    Terminal,
    Globe,
    Key,
    Sparkles,
    ArrowLeft,
    ArrowRight,
    Wrench,
    Sliders,
    Chart,
    Cube,
    Grid,
    List,
    Image,
    Close,
    #[cfg_attr(target_os = "android", allow(dead_code))]
    MultiWindow,
}

impl VectorIcon {
    pub fn ai_asset(self) -> Option<UiAssetIcon> {
        match self {
            Self::Play => Some(UiAssetIcon::Play),
            Self::Download => Some(UiAssetIcon::Download),
            Self::Folder => Some(UiAssetIcon::Folder),
            Self::Refresh => Some(UiAssetIcon::Refresh),
            Self::Trash => Some(UiAssetIcon::Trash),
            Self::MultiWindow => Some(UiAssetIcon::MultiWindow),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle3d {
    PrimaryEmerald,
    HeroLaunch,
    HeroStop,
    AccentViolet,
    DangerRose,
    SecondaryGlass,
}

#[derive(Clone)]
struct ProjectedFace {
    pts: [Pos2; 4],
    depth: f32,
    fill: Color32,
    stroke: Stroke,
}

pub struct Poly3dBuffer {
    rect: Rect,
    cam_z: f32,
    fov_scale: f32,
    light_dir: Vec3,
    faces: Vec<ProjectedFace>,
}

impl Poly3dBuffer {
    pub fn new(rect: Rect, cam_z: f32, fov_scale: f32) -> Self {
        Self {
            rect,
            cam_z,
            fov_scale,
            light_dir: Vec3::new(0.46, 0.74, 0.52).normalize(),
            faces: Vec::with_capacity(48),
        }
    }

    pub fn project(&self, p: Vec3) -> (Pos2, f32) {
        let depth = (self.cam_z - p.z).max(0.45);
        let scale = (self.fov_scale / depth) * (self.rect.width().min(self.rect.height()) * 0.5);
        let c = self.rect.center();
        (pos2(c.x + p.x * scale, c.y - p.y * scale), depth)
    }

    pub fn push_cuboid(
        &mut self,
        xf: Mat4,
        local_center: Vec3,
        size: Vec3,
        base: Color32,
        edge_highlight: Color32,
    ) {
        let h = size * 0.5;
        let min = local_center - h;
        let max = local_center + h;

        let local_faces: [([Vec3; 4], Vec3); 6] = [
            (
                [
                    Vec3::new(min.x, max.y, max.z),
                    Vec3::new(max.x, max.y, max.z),
                    Vec3::new(max.x, min.y, max.z),
                    Vec3::new(min.x, min.y, max.z),
                ],
                Vec3::Z,
            ),
            (
                [
                    Vec3::new(max.x, max.y, min.z),
                    Vec3::new(min.x, max.y, min.z),
                    Vec3::new(min.x, min.y, min.z),
                    Vec3::new(max.x, min.y, min.z),
                ],
                Vec3::NEG_Z,
            ),
            (
                [
                    Vec3::new(min.x, max.y, min.z),
                    Vec3::new(min.x, max.y, max.z),
                    Vec3::new(min.x, min.y, max.z),
                    Vec3::new(min.x, min.y, min.z),
                ],
                Vec3::NEG_X,
            ),
            (
                [
                    Vec3::new(max.x, max.y, max.z),
                    Vec3::new(max.x, max.y, min.z),
                    Vec3::new(max.x, min.y, min.z),
                    Vec3::new(max.x, min.y, max.z),
                ],
                Vec3::X,
            ),
            (
                [
                    Vec3::new(min.x, max.y, min.z),
                    Vec3::new(max.x, max.y, min.z),
                    Vec3::new(max.x, max.y, max.z),
                    Vec3::new(min.x, max.y, max.z),
                ],
                Vec3::Y,
            ),
            (
                [
                    Vec3::new(min.x, min.y, max.z),
                    Vec3::new(max.x, min.y, max.z),
                    Vec3::new(max.x, min.y, min.z),
                    Vec3::new(min.x, min.y, min.z),
                ],
                Vec3::NEG_Y,
            ),
        ];

        let cam_pos = Vec3::new(0.0, 0.0, self.cam_z);
        for (corners, local_n) in local_faces {
            let world_pts = corners.map(|c| xf.transform_point3(c));
            let center = (world_pts[0] + world_pts[1] + world_pts[2] + world_pts[3]) * 0.25;
            let world_n = xf.transform_vector3(local_n).normalize_or_zero();
            let to_cam = (cam_pos - center).normalize_or_zero();
            if world_n.dot(to_cam) <= 0.02 {
                continue;
            }

            let diff = world_n.dot(self.light_dir).max(0.0);
            let hemi = world_n.y * 0.12 + 0.52;
            let rim = (1.0 - world_n.dot(to_cam).clamp(0.0, 1.0)).powi(2) * 0.25;
            let brightness = (hemi + diff * 0.54 + rim).clamp(0.32, 1.28);

            let fill = shade_color(base, brightness);
            let mut proj_pts = [Pos2::ZERO; 4];
            let mut depth_sum = 0.0;
            for (idx, wp) in world_pts.into_iter().enumerate() {
                let (sp, d) = self.project(wp);
                proj_pts[idx] = sp;
                depth_sum += d;
            }

            self.faces.push(ProjectedFace {
                pts: proj_pts,
                depth: depth_sum * 0.25,
                fill,
                stroke: Stroke::new(0.85, edge_highlight),
            });
        }
    }

    #[cfg(test)]
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    pub fn flush(mut self, painter: &egui::Painter) {
        self.faces.sort_by(|a, b| {
            b.depth
                .partial_cmp(&a.depth)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for f in self.faces {
            painter.add(Shape::convex_polygon(f.pts.to_vec(), f.fill, f.stroke));
        }
    }
}

#[cfg(test)]
pub fn build_icon_3d_scene(rect: Rect, kind: Icon3dKind, t: f32) -> Poly3dBuffer {
    let mut buf = Poly3dBuffer::new(rect, 4.4, 2.55);
    match kind {
        Icon3dKind::BrandVoxelCore => {
            let bob = (t * 2.2).sin() * 0.11;
            let core_xf = Mat4::from_translation(Vec3::new(0.0, bob - 0.06, 0.0))
                * Mat4::from_rotation_x(0.44 + (t * 1.1).sin() * 0.10)
                * Mat4::from_rotation_y(t * 0.95);
            buf.push_cuboid(
                core_xf,
                Vec3::new(0.0, -0.14, 0.0),
                Vec3::new(1.06, 0.78, 1.06),
                Color32::from_rgb(138, 92, 54),
                Color32::from_rgba_unmultiplied(255, 224, 152, 130),
            );

            let cap_lift = 0.38 + (t * 2.8).sin().abs() * 0.12;
            buf.push_cuboid(
                core_xf,
                Vec3::new(0.0, cap_lift, 0.0),
                Vec3::new(1.12, 0.30, 1.12),
                ACCENT_EMERALD,
                Color32::from_rgba_unmultiplied(180, 255, 225, 185),
            );

            for i in 0..3 {
                let a = t * 1.9 + i as f32 * (std::f32::consts::TAU / 3.0);
                let sat_xf = Mat4::from_rotation_z(0.32)
                    * Mat4::from_rotation_y(a)
                    * Mat4::from_translation(Vec3::new(0.0, bob, 1.18))
                    * Mat4::from_rotation_x(t * 2.4);
                let sat_col = match i {
                    0 => ACCENT_CYAN,
                    1 => ACCENT_AMBER,
                    _ => ACCENT_EMERALD,
                };
                buf.push_cuboid(
                    sat_xf,
                    Vec3::ZERO,
                    Vec3::splat(0.22),
                    sat_col,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 165),
                );
            }
        }
        Icon3dKind::HomePortal => {
            let bob = (t * 2.1).sin() * 0.10;
            let xf = Mat4::from_translation(Vec3::new(0.0, bob - 0.10, 0.0))
                * Mat4::from_rotation_x(0.40)
                * Mat4::from_rotation_y(0.66 + (t * 1.35).sin() * 0.32);
            buf.push_cuboid(
                xf,
                Vec3::new(0.0, -0.15, 0.0),
                Vec3::new(1.10, 0.84, 1.10),
                Color32::from_rgb(214, 178, 128),
                Color32::from_rgba_unmultiplied(56, 152, 255, 120),
            );
            let roof_lift = 0.42 + (t * 2.6).sin().abs() * 0.10;
            buf.push_cuboid(
                xf,
                Vec3::new(0.0, roof_lift, 0.0),
                Vec3::new(1.24, 0.28, 1.24),
                ACCENT_EMERALD,
                Color32::from_rgba_unmultiplied(167, 243, 208, 180),
            );
            buf.push_cuboid(
                xf,
                Vec3::new(0.0, -0.12, 0.56),
                Vec3::new(0.42, 0.54, 0.08),
                ACCENT_CYAN,
                Color32::WHITE,
            );
        }
        Icon3dKind::ModpackCrate => {
            let bob = (t * 2.0).sin() * 0.09;
            let xf = Mat4::from_translation(Vec3::new(0.0, bob - 0.14, 0.0))
                * Mat4::from_rotation_x(0.42)
                * Mat4::from_rotation_y(-0.62 + (t * 1.25).sin() * 0.28);
            buf.push_cuboid(
                xf,
                Vec3::new(0.0, -0.14, 0.0),
                Vec3::new(1.14, 0.82, 1.14),
                ACCENT_AMBER,
                Color32::from_rgba_unmultiplied(254, 240, 138, 155),
            );

            let open_amt = (t * 2.4).sin().abs();
            let gem_xf = xf
                * Mat4::from_translation(Vec3::new(0.0, 0.22 + open_amt * 0.26, 0.0))
                * Mat4::from_rotation_y(t * 2.5)
                * Mat4::from_rotation_z(0.6);
            buf.push_cuboid(
                gem_xf,
                Vec3::ZERO,
                Vec3::splat(0.42),
                ACCENT_CYAN,
                Color32::from_rgba_unmultiplied(224, 242, 254, 190),
            );

            let lid_xf = xf * Mat4::from_translation(Vec3::new(0.0, 0.44 + open_amt * 0.30, 0.0));
            buf.push_cuboid(
                lid_xf,
                Vec3::ZERO,
                Vec3::new(1.22, 0.22, 1.22),
                ACCENT_ORANGE,
                Color32::from_rgba_unmultiplied(255, 237, 213, 175),
            );
        }
        Icon3dKind::ServerBladeTower => {
            let base_xf = Mat4::from_rotation_x(0.38)
                * Mat4::from_rotation_y(-0.56 + (t * 1.05).sin() * 0.22);
            for blade in 0..3 {
                let phase = (t * 2.6 + blade as f32 * 0.95).sin() * 0.04;
                let y = -0.52 + blade as f32 * 0.50 + phase;
                let active_led = ((t * 3.2) as usize + blade).is_multiple_of(3);
                let col = if active_led {
                    Color32::from_rgb(56, 98, 148)
                } else {
                    Color32::from_rgb(42, 68, 108)
                };
                let rim = if active_led {
                    ACCENT_CYAN
                } else {
                    Color32::from_rgba_unmultiplied(125, 162, 210, 130)
                };
                buf.push_cuboid(
                    base_xf,
                    Vec3::new(0.0, y, 0.0),
                    Vec3::new(1.24, 0.34, 0.98),
                    col,
                    rim,
                );
                let led_col = if active_led {
                    ACCENT_EMERALD
                } else {
                    ACCENT_BLUE
                };
                buf.push_cuboid(
                    base_xf,
                    Vec3::new(0.36, y, 0.51),
                    Vec3::new(0.26, 0.11, 0.06),
                    led_col,
                    Color32::WHITE,
                );
            }
        }
        Icon3dKind::AccountCharacter => {
            let bob = (t * 2.1).sin() * 0.07;
            let body_xf = Mat4::from_translation(Vec3::new(0.0, bob, 0.0))
                * Mat4::from_rotation_x(0.24)
                * Mat4::from_rotation_y((t * 1.15).sin() * 0.26);
            buf.push_cuboid(
                body_xf,
                Vec3::new(0.0, -0.52, 0.0),
                Vec3::new(1.08, 0.68, 0.54),
                ACCENT_BLUE,
                Color32::from_rgba_unmultiplied(147, 197, 253, 150),
            );

            let head_xf = body_xf
                * Mat4::from_translation(Vec3::new(0.0, 0.22, 0.0))
                * Mat4::from_rotation_y((t * 1.85).sin() * 0.32)
                * Mat4::from_rotation_x((t * 1.4).cos() * 0.10);
            buf.push_cuboid(
                head_xf,
                Vec3::ZERO,
                Vec3::splat(0.76),
                Color32::from_rgb(232, 180, 142),
                Color32::from_rgba_unmultiplied(255, 255, 255, 130),
            );

            let crown_xf = head_xf
                * Mat4::from_translation(Vec3::new(0.0, 0.54, 0.0))
                * Mat4::from_rotation_y(t * 2.0);
            buf.push_cuboid(
                crown_xf,
                Vec3::ZERO,
                Vec3::new(0.56, 0.14, 0.56),
                ACCENT_AMBER,
                Color32::WHITE,
            );
        }
        Icon3dKind::JavaReactor => {
            let core_xf = Mat4::from_rotation_y(t * 1.8) * Mat4::from_rotation_x(t * 1.3);
            buf.push_cuboid(
                core_xf,
                Vec3::ZERO,
                Vec3::splat(0.58),
                ACCENT_ORANGE,
                Color32::from_rgba_unmultiplied(254, 215, 170, 190),
            );
            for ring in 0..2 {
                let tilt = if ring == 0 {
                    Mat4::from_rotation_z(0.65) * Mat4::from_rotation_x(0.45)
                } else {
                    Mat4::from_rotation_z(-0.65) * Mat4::from_rotation_x(0.45)
                };
                let dir = if ring == 0 { 1.0 } else { -1.0 };
                for s in 0..4 {
                    let a = t * 2.4 * dir + s as f32 * std::f32::consts::FRAC_PI_2;
                    let seg_xf = tilt
                        * Mat4::from_rotation_y(a)
                        * Mat4::from_translation(Vec3::new(0.0, 0.0, 1.08));
                    let c = if ring == 0 { ACCENT_CYAN } else { ACCENT_AMBER };
                    buf.push_cuboid(
                        seg_xf,
                        Vec3::ZERO,
                        Vec3::new(0.32, 0.14, 0.18),
                        c,
                        Color32::WHITE,
                    );
                }
            }
        }
        Icon3dKind::LanRadar => {
            let disc_xf = Mat4::from_translation(Vec3::new(0.0, -0.34, 0.0))
                * Mat4::from_rotation_x(0.52)
                * Mat4::from_rotation_y(t * 1.6);
            buf.push_cuboid(
                disc_xf,
                Vec3::ZERO,
                Vec3::new(1.26, 0.16, 1.26),
                Color32::from_rgb(38, 92, 138),
                ACCENT_CYAN,
            );

            let pin_jump = (t * 3.1).sin().abs() * 0.28;
            let pin_xf = Mat4::from_translation(Vec3::new(0.0, 0.22 + pin_jump, 0.0))
                * Mat4::from_rotation_y(-t * 2.0)
                * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_4);
            buf.push_cuboid(
                pin_xf,
                Vec3::ZERO,
                Vec3::splat(0.48),
                ACCENT_EMERALD,
                Color32::WHITE,
            );
        }
        Icon3dKind::SettingsGyro => {
            let outer = Mat4::from_rotation_x(t * 1.25) * Mat4::from_rotation_y(0.55);
            for i in 0..4 {
                let a = i as f32 * std::f32::consts::FRAC_PI_2;
                let xf = outer
                    * Mat4::from_rotation_z(a)
                    * Mat4::from_translation(Vec3::new(0.95, 0.0, 0.0));
                buf.push_cuboid(
                    xf,
                    Vec3::ZERO,
                    Vec3::new(0.24, 0.72, 0.22),
                    ACCENT_AMBER,
                    Color32::from_rgba_unmultiplied(254, 243, 199, 175),
                );
            }
            let inner = Mat4::from_rotation_y(-t * 1.9) * Mat4::from_rotation_x(0.65);
            buf.push_cuboid(
                inner,
                Vec3::ZERO,
                Vec3::splat(0.56),
                ACCENT_CYAN,
                Color32::WHITE,
            );
        }
        Icon3dKind::LaunchPrism => {
            let pulse = (t * 3.4).sin() * 0.08;
            let xf = Mat4::from_translation(Vec3::new(pulse, 0.0, 0.0))
                * Mat4::from_rotation_z(-0.22)
                * Mat4::from_rotation_x(t * 2.6)
                * Mat4::from_rotation_y(0.52);
            buf.push_cuboid(
                xf,
                Vec3::ZERO,
                Vec3::new(1.18, 0.56, 0.56),
                ACCENT_EMERALD,
                Color32::WHITE,
            );
            let trail_xf = Mat4::from_translation(Vec3::new(-0.68 - pulse, 0.0, -0.2))
                * Mat4::from_rotation_x(-t * 2.0);
            buf.push_cuboid(
                trail_xf,
                Vec3::ZERO,
                Vec3::new(0.32, 0.36, 0.36),
                ACCENT_CYAN,
                Color32::WHITE,
            );
        }
        Icon3dKind::MultiWindow => {
            let sway = (t * 1.8).sin() * 0.22;
            let back_xf = Mat4::from_translation(Vec3::new(-0.26, 0.16, -0.22))
                * Mat4::from_rotation_y(sway)
                * Mat4::from_rotation_x(0.22);
            buf.push_cuboid(
                back_xf,
                Vec3::ZERO,
                Vec3::new(1.06, 0.80, 0.16),
                ACCENT_BLUE,
                Color32::WHITE,
            );
            let front_xf = Mat4::from_translation(Vec3::new(0.24, -0.16, 0.24))
                * Mat4::from_rotation_y(sway * 1.15)
                * Mat4::from_rotation_x(0.22);
            buf.push_cuboid(
                front_xf,
                Vec3::ZERO,
                Vec3::new(1.06, 0.80, 0.18),
                ACCENT_CYAN,
                Color32::WHITE,
            );
        }
        Icon3dKind::MobileGamepad => {
            let tilt = Mat4::from_rotation_x(0.36 + (t * 2.0).sin() * 0.10)
                * Mat4::from_rotation_y((t * 1.4).cos() * 0.28);
            buf.push_cuboid(
                tilt,
                Vec3::ZERO,
                Vec3::new(1.36, 0.66, 0.28),
                ACCENT_BLUE,
                Color32::WHITE,
            );
            buf.push_cuboid(
                tilt,
                Vec3::new(-0.44, -0.26, 0.0),
                Vec3::new(0.34, 0.56, 0.28),
                ACCENT_CYAN,
                Color32::WHITE,
            );
            buf.push_cuboid(
                tilt,
                Vec3::new(0.44, -0.26, 0.0),
                Vec3::new(0.34, 0.56, 0.28),
                ACCENT_AMBER,
                Color32::WHITE,
            );
        }
        Icon3dKind::ScreenRotate => {
            let spin = Mat4::from_rotation_z((t * 1.6).sin() * 0.65)
                * Mat4::from_rotation_y(0.35)
                * Mat4::from_rotation_x(0.25);
            buf.push_cuboid(
                spin,
                Vec3::ZERO,
                Vec3::new(0.84, 1.24, 0.18),
                ACCENT_CYAN,
                Color32::WHITE,
            );
            buf.push_cuboid(
                spin,
                Vec3::new(0.0, 0.0, 0.12),
                Vec3::new(0.66, 0.96, 0.08),
                ACCENT_EMERALD,
                Color32::WHITE,
            );
        }
    }
    buf
}

pub fn paint_icon_3d(ui: &mut Ui, size: f32, kind: Icon3dKind, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return resp;
    }

    let low_ram = is_low_ram_mode();
    let t = if low_ram {
        0.35
    } else {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
        ui.input(|i| i.time) as f32
    };

    let painter = ui.painter();
    let dark = ui.visuals().dark_mode;
    let c = rect.center();
    let hovered = resp.hovered();

    // Grounded 3D contact shadow beneath floating icon
    let bob_px = if low_ram {
        0.0
    } else {
        let speed = if active || hovered { 2.7 } else { 1.7 };
        let amp = if active || hovered {
            size * 0.048
        } else {
            size * 0.026
        };
        (t * speed).sin() * amp
    };
    let shadow_scale = (1.0 - (bob_px / (size.max(1.0) * 0.28)).abs() * 0.22).clamp(0.7, 1.15);
    let shadow_alpha = if dark { 76 } else { 38 };
    painter.circle_filled(
        pos2(c.x, rect.bottom() - size * 0.07),
        size * 0.25 * shadow_scale,
        Color32::from_rgba_unmultiplied(15, 28, 48, shadow_alpha),
    );

    if active || hovered {
        let (accent, _) = accent_palette(active_accent_preset(ui.ctx()));
        let pulse = if low_ram {
            0.90
        } else {
            ((t * 3.0).sin() * 0.5 + 0.5) * 0.18 + 0.84
        };
        let aura_alpha = if active {
            if dark {
                52
            } else {
                38
            }
        } else if dark {
            28
        } else {
            22
        };
        painter.circle_filled(
            c,
            size * 0.46 * pulse,
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), aura_alpha),
        );
    }

    // Subtle real-time 3D orbital voxel satellites behind the Brand / Active hero icon
    if !low_ram && (kind == Icon3dKind::BrandVoxelCore || (active && size >= 34.0)) {
        let mut orbit_buf = Poly3dBuffer::new(rect.expand(size * 0.06), 4.6, 2.55);
        for i in 0..2 {
            let a = t * 2.1 + i as f32 * std::f32::consts::PI;
            let sat_xf = Mat4::from_rotation_z(0.28)
                * Mat4::from_rotation_y(a)
                * Mat4::from_translation(Vec3::new(0.0, 0.0, 1.24))
                * Mat4::from_rotation_x(t * 2.6);
            let sat_col = if i == 0 { ACCENT_EMERALD } else { ACCENT_AMBER };
            orbit_buf.push_cuboid(
                sat_xf,
                Vec3::ZERO,
                Vec3::splat(0.19),
                sat_col,
                Color32::from_rgba_unmultiplied(255, 255, 255, 180),
            );
        }
        orbit_buf.flush(painter);
    }

    // Radar expanding pulse rings for LanRadar
    if !low_ram && kind == Icon3dKind::LanRadar {
        for wave in 0..2 {
            let p = ((t * 0.72 + wave as f32 * 0.5).fract()).clamp(0.0, 1.0);
            let wr = size * (0.18 + p * 0.30);
            let alpha = ((1.0 - p) * (if dark { 145.0 } else { 115.0 })) as u8;
            painter.circle_stroke(
                c,
                wr,
                Stroke::new(1.25, Color32::from_rgba_unmultiplied(14, 165, 233, alpha)),
            );
        }
    }

    // Real-time 3D perspective tilt (tracks pointer hover + gentle organic 3D breathing)
    let (ptr_pitch, ptr_yaw) = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| rect.expand(8.0).contains(*p))
        .map(|p| {
            let nx = ((p.x - c.x) / (size * 0.5).max(1.0)).clamp(-1.0, 1.0);
            let ny = ((p.y - c.y) / (size * 0.5).max(1.0)).clamp(-1.0, 1.0);
            (-ny * 0.24, nx * 0.28)
        })
        .unwrap_or((0.0, 0.0));

    let idle_amp = if low_ram {
        0.0
    } else if active || hovered {
        0.16
    } else {
        0.085
    };
    let pitch = ptr_pitch + (t * 1.55).cos() * (idle_amp * 0.65);
    let yaw = ptr_yaw + (t * 1.25).sin() * idle_amp;

    let icon_pad = if size >= 34.0 { 1.0 } else { 0.5 };
    let icon_rect = rect.shrink(icon_pad);
    let tint = if active || hovered {
        Color32::WHITE
    } else if dark {
        Color32::from_rgb(242, 247, 255)
    } else {
        Color32::from_rgb(250, 252, 255)
    };

    paint_tilted_ai_icon(
        painter,
        icon_rect,
        kind.ai_asset(),
        pitch,
        yaw,
        bob_px,
        tint,
    );

    resp
}

pub fn paint_vector_icon(painter: &egui::Painter, rect: Rect, icon: VectorIcon, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let st = Stroke::new(1.75, color);

    match icon {
        VectorIcon::Play => {
            let pts = vec![
                pos2(c.x - r * 0.55, c.y - r * 0.78),
                pos2(c.x + r * 0.85, c.y),
                pos2(c.x - r * 0.55, c.y + r * 0.78),
            ];
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        VectorIcon::Stop => {
            let sq = Rect::from_center_size(c, Vec2::splat(r * 1.3));
            painter.rect_filled(sq, CornerRadius::same(2), color);
        }
        VectorIcon::Plus => {
            painter.line_segment([pos2(c.x - r, c.y), pos2(c.x + r, c.y)], st);
            painter.line_segment([pos2(c.x, c.y - r), pos2(c.x, c.y + r)], st);
        }
        VectorIcon::Download => {
            painter.line_segment([pos2(c.x, c.y - r * 0.9), pos2(c.x, c.y + r * 0.45)], st);
            painter.line_segment(
                [
                    pos2(c.x - r * 0.55, c.y - r * 0.05),
                    pos2(c.x, c.y + r * 0.5),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x + r * 0.55, c.y - r * 0.05),
                    pos2(c.x, c.y + r * 0.5),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x - r * 0.85, c.y + r * 0.85),
                    pos2(c.x + r * 0.85, c.y + r * 0.85),
                ],
                st,
            );
        }
        VectorIcon::Upload => {
            painter.line_segment([pos2(c.x, c.y + r * 0.6), pos2(c.x, c.y - r * 0.65)], st);
            painter.line_segment(
                [
                    pos2(c.x - r * 0.55, c.y - r * 0.1),
                    pos2(c.x, c.y - r * 0.68),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x + r * 0.55, c.y - r * 0.1),
                    pos2(c.x, c.y - r * 0.68),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x - r * 0.85, c.y + r * 0.88),
                    pos2(c.x + r * 0.85, c.y + r * 0.88),
                ],
                st,
            );
        }
        VectorIcon::Folder => {
            let body = Rect::from_min_max(
                pos2(c.x - r * 0.95, c.y - r * 0.45),
                pos2(c.x + r * 0.95, c.y + r * 0.78),
            );
            painter.rect_stroke(body, CornerRadius::same(2), st, StrokeKind::Inside);
            painter.line_segment(
                [
                    pos2(c.x - r * 0.9, c.y - r * 0.45),
                    pos2(c.x - r * 0.25, c.y - r * 0.78),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x - r * 0.25, c.y - r * 0.78),
                    pos2(c.x + r * 0.2, c.y - r * 0.45),
                ],
                st,
            );
        }
        VectorIcon::Search => {
            let gc = pos2(c.x - r * 0.18, c.y - r * 0.18);
            painter.circle_stroke(gc, r * 0.62, st);
            painter.line_segment(
                [
                    pos2(gc.x + r * 0.44, gc.y + r * 0.44),
                    pos2(c.x + r * 0.85, c.y + r * 0.85),
                ],
                st,
            );
        }
        VectorIcon::Refresh => {
            painter.circle_stroke(c, r * 0.75, st);
            painter.add(Shape::convex_polygon(
                vec![
                    pos2(c.x + r * 0.42, c.y - r * 0.95),
                    pos2(c.x + r * 0.98, c.y - r * 0.55),
                    pos2(c.x + r * 0.40, c.y - r * 0.35),
                ],
                color,
                Stroke::NONE,
            ));
        }
        VectorIcon::Trash => {
            painter.line_segment(
                [
                    pos2(c.x - r * 0.85, c.y - r * 0.55),
                    pos2(c.x + r * 0.85, c.y - r * 0.55),
                ],
                st,
            );
            let bin = Rect::from_min_max(
                pos2(c.x - r * 0.62, c.y - r * 0.55),
                pos2(c.x + r * 0.62, c.y + r * 0.85),
            );
            painter.rect_stroke(bin, CornerRadius::same(2), st, StrokeKind::Inside);
        }
        VectorIcon::Save => {
            let sq = Rect::from_center_size(c, Vec2::splat(r * 1.65));
            painter.rect_stroke(sq, CornerRadius::same(2), st, StrokeKind::Inside);
            let inner = Rect::from_min_max(
                pos2(c.x - r * 0.45, c.y - r * 0.82),
                pos2(c.x + r * 0.45, c.y - r * 0.22),
            );
            painter.rect_filled(inner, CornerRadius::same(1), color);
        }
        VectorIcon::Copy => {
            let back = Rect::from_min_max(
                pos2(c.x - r * 0.78, c.y - r * 0.78),
                pos2(c.x + r * 0.35, c.y + r * 0.35),
            );
            let front = Rect::from_min_max(
                pos2(c.x - r * 0.32, c.y - r * 0.32),
                pos2(c.x + r * 0.82, c.y + r * 0.82),
            );
            painter.rect_stroke(back, CornerRadius::same(2), st, StrokeKind::Inside);
            painter.rect_stroke(front, CornerRadius::same(2), st, StrokeKind::Inside);
        }
        VectorIcon::Shield => {
            let pts = vec![
                pos2(c.x, c.y - r * 0.92),
                pos2(c.x + r * 0.80, c.y - r * 0.50),
                pos2(c.x + r * 0.58, c.y + r * 0.42),
                pos2(c.x, c.y + r * 0.92),
                pos2(c.x - r * 0.58, c.y + r * 0.42),
                pos2(c.x - r * 0.80, c.y - r * 0.50),
            ];
            painter.add(Shape::closed_line(pts, st));
        }
        VectorIcon::Terminal => {
            painter.line_segment(
                [
                    pos2(c.x - r * 0.78, c.y - r * 0.48),
                    pos2(c.x - r * 0.18, c.y),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x - r * 0.18, c.y),
                    pos2(c.x - r * 0.78, c.y + r * 0.48),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x + r * 0.05, c.y + r * 0.52),
                    pos2(c.x + r * 0.82, c.y + r * 0.52),
                ],
                st,
            );
        }
        VectorIcon::Globe => {
            painter.circle_stroke(c, r * 0.86, st);
            painter.line_segment([pos2(c.x - r * 0.84, c.y), pos2(c.x + r * 0.84, c.y)], st);
            painter.line_segment([pos2(c.x, c.y - r * 0.84), pos2(c.x, c.y + r * 0.84)], st);
        }
        VectorIcon::Key => {
            let ring_c = pos2(c.x - r * 0.35, c.y);
            painter.circle_stroke(ring_c, r * 0.45, st);
            painter.line_segment(
                [pos2(ring_c.x + r * 0.45, c.y), pos2(c.x + r * 0.88, c.y)],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x + r * 0.62, c.y),
                    pos2(c.x + r * 0.62, c.y + r * 0.36),
                ],
                st,
            );
        }
        VectorIcon::Sparkles => {
            let pts = vec![
                pos2(c.x, c.y - r * 0.92),
                pos2(c.x + r * 0.26, c.y - r * 0.26),
                pos2(c.x + r * 0.92, c.y),
                pos2(c.x + r * 0.26, c.y + r * 0.26),
                pos2(c.x, c.y + r * 0.92),
                pos2(c.x - r * 0.26, c.y + r * 0.26),
                pos2(c.x - r * 0.92, c.y),
                pos2(c.x - r * 0.26, c.y - r * 0.26),
            ];
            painter.add(Shape::closed_line(pts, st));
        }
        VectorIcon::ArrowLeft => {
            painter.line_segment([pos2(c.x + r * 0.75, c.y), pos2(c.x - r * 0.68, c.y)], st);
            painter.line_segment(
                [
                    pos2(c.x - r * 0.18, c.y - r * 0.55),
                    pos2(c.x - r * 0.72, c.y),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x - r * 0.18, c.y + r * 0.55),
                    pos2(c.x - r * 0.72, c.y),
                ],
                st,
            );
        }
        VectorIcon::ArrowRight => {
            painter.line_segment([pos2(c.x - r * 0.75, c.y), pos2(c.x + r * 0.68, c.y)], st);
            painter.line_segment(
                [
                    pos2(c.x + r * 0.18, c.y - r * 0.55),
                    pos2(c.x + r * 0.72, c.y),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x + r * 0.18, c.y + r * 0.55),
                    pos2(c.x + r * 0.72, c.y),
                ],
                st,
            );
        }
        VectorIcon::Wrench => {
            painter.line_segment(
                [
                    pos2(c.x - r * 0.72, c.y + r * 0.72),
                    pos2(c.x + r * 0.35, c.y - r * 0.35),
                ],
                Stroke::new(2.0, color),
            );
            painter.circle_stroke(pos2(c.x + r * 0.45, c.y - r * 0.45), r * 0.42, st);
        }
        VectorIcon::Sliders => {
            for (idx, ox) in [-0.58f32, 0.0, 0.58].into_iter().enumerate() {
                let x = c.x + r * ox;
                painter.line_segment([pos2(x, c.y - r * 0.8), pos2(x, c.y + r * 0.8)], st);
                let ky = if idx == 1 {
                    c.y - r * 0.3
                } else {
                    c.y + r * 0.25
                };
                painter.circle_filled(pos2(x, ky), r * 0.24, color);
            }
        }
        VectorIcon::Chart => {
            for (ox, h) in [(-0.62f32, 0.45f32), (0.0, 0.85), (0.62, 0.68)] {
                let x = c.x + r * ox;
                let bar = Rect::from_min_max(
                    pos2(x - r * 0.18, c.y + r * 0.8 - r * 1.6 * h),
                    pos2(x + r * 0.18, c.y + r * 0.8),
                );
                painter.rect_filled(bar, CornerRadius::same(1), color);
            }
        }
        VectorIcon::Cube => {
            let top = vec![
                pos2(c.x, c.y - r * 0.90),
                pos2(c.x + r * 0.82, c.y - r * 0.42),
                pos2(c.x, c.y + r * 0.06),
                pos2(c.x - r * 0.82, c.y - r * 0.42),
            ];
            painter.add(Shape::closed_line(top, st));
            painter.line_segment([pos2(c.x, c.y + r * 0.06), pos2(c.x, c.y + r * 0.92)], st);
            painter.line_segment(
                [
                    pos2(c.x - r * 0.82, c.y - r * 0.42),
                    pos2(c.x - r * 0.82, c.y + r * 0.45),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x + r * 0.82, c.y - r * 0.42),
                    pos2(c.x + r * 0.82, c.y + r * 0.45),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x - r * 0.82, c.y + r * 0.45),
                    pos2(c.x, c.y + r * 0.92),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x + r * 0.82, c.y + r * 0.45),
                    pos2(c.x, c.y + r * 0.92),
                ],
                st,
            );
        }
        VectorIcon::Grid => {
            for gx in [-0.46f32, 0.46] {
                for gy in [-0.46f32, 0.46] {
                    let sq = Rect::from_center_size(
                        pos2(c.x + r * gx, c.y + r * gy),
                        Vec2::splat(r * 0.66),
                    );
                    painter.rect_stroke(sq, CornerRadius::same(1), st, StrokeKind::Inside);
                }
            }
        }
        VectorIcon::List => {
            for gy in [-0.56f32, 0.0, 0.56] {
                let y = c.y + r * gy;
                painter.circle_filled(pos2(c.x - r * 0.72, y), r * 0.16, color);
                painter.line_segment([pos2(c.x - r * 0.34, y), pos2(c.x + r * 0.85, y)], st);
            }
        }
        VectorIcon::Image => {
            let frame = Rect::from_min_max(
                pos2(c.x - r * 0.90, c.y - r * 0.72),
                pos2(c.x + r * 0.90, c.y + r * 0.72),
            );
            painter.rect_stroke(frame, CornerRadius::same(2), st, StrokeKind::Inside);
            painter.circle_filled(pos2(c.x - r * 0.40, c.y - r * 0.28), r * 0.18, color);
            let peaks = vec![
                pos2(c.x - r * 0.75, c.y + r * 0.55),
                pos2(c.x - r * 0.22, c.y - r * 0.02),
                pos2(c.x + r * 0.14, c.y + r * 0.32),
                pos2(c.x + r * 0.48, c.y - r * 0.12),
                pos2(c.x + r * 0.78, c.y + r * 0.55),
            ];
            painter.add(Shape::line(peaks, st));
        }
        VectorIcon::Close => {
            painter.line_segment(
                [
                    pos2(c.x - r * 0.68, c.y - r * 0.68),
                    pos2(c.x + r * 0.68, c.y + r * 0.68),
                ],
                st,
            );
            painter.line_segment(
                [
                    pos2(c.x + r * 0.68, c.y - r * 0.68),
                    pos2(c.x - r * 0.68, c.y + r * 0.68),
                ],
                st,
            );
        }
        VectorIcon::MultiWindow => {
            let back = Rect::from_min_max(
                pos2(c.x - r * 0.85, c.y - r * 0.75),
                pos2(c.x + r * 0.35, c.y + r * 0.25),
            );
            let front = Rect::from_min_max(
                pos2(c.x - r * 0.30, c.y - r * 0.25),
                pos2(c.x + r * 0.88, c.y + r * 0.78),
            );
            painter.rect_stroke(back, CornerRadius::same(2), st, StrokeKind::Inside);
            painter.rect_stroke(front, CornerRadius::same(2), st, StrokeKind::Inside);
        }
    }
}

pub fn paint_gaming_3d_backdrop(ui: &Ui, rect: Rect, dark_mode: bool, has_custom_bg: bool) {
    if !ui.is_rect_visible(rect) || is_low_ram_mode() {
        return;
    }

    let t = ui.input(|i| i.time) as f32;
    ui.ctx().request_repaint_after(Duration::from_millis(24));

    let pointer_norm = ui
        .input(|i| i.pointer.hover_pos())
        .map(|p| {
            let nx = ((p.x - rect.center().x) / rect.width().max(1.0)).clamp(-0.5, 0.5);
            let ny = ((p.y - rect.center().y) / rect.height().max(1.0)).clamp(-0.5, 0.5);
            vec2(nx, ny)
        })
        .unwrap_or(Vec2::ZERO);

    let painter = ui.painter_at(rect);
    let alpha_mul = if has_custom_bg { 0.38 } else { 1.0 };

    // Sunlit Sky / Cobalt Studio Ambient Lights (Parallax Layer 1)
    let (sun_col, breeze_col) = if dark_mode {
        (
            Color32::from_rgba_unmultiplied(56, 152, 255, (24.0 * alpha_mul) as u8),
            Color32::from_rgba_unmultiplied(20, 184, 102, (20.0 * alpha_mul) as u8),
        )
    } else {
        (
            Color32::from_rgba_unmultiplied(253, 214, 112, (38.0 * alpha_mul) as u8),
            Color32::from_rgba_unmultiplied(125, 211, 252, (42.0 * alpha_mul) as u8),
        )
    };
    painter.circle_filled(
        pos2(
            rect.right() - rect.width() * 0.14 - pointer_norm.x * 24.0,
            rect.top() + rect.height() * 0.14 - pointer_norm.y * 16.0,
        ),
        rect.width().max(rect.height()) * 0.28,
        sun_col,
    );
    painter.circle_filled(
        pos2(
            rect.left() + rect.width() * 0.18 - pointer_norm.x * 16.0,
            rect.bottom() - rect.height() * 0.14 - pointer_norm.y * 12.0,
        ),
        rect.width().max(rect.height()) * 0.26,
        breeze_col,
    );

    // 3D Perspective Game-Engine Horizon Blueprint Grid (Parallax Layer 2)
    let horizon_y = rect.bottom() - rect.height() * 0.27 + pointer_norm.y * 10.0;
    let vanish_x = rect.center().x - pointer_norm.x * 34.0;
    let grid_alpha = ((if dark_mode { 28.0 } else { 32.0 }) * alpha_mul) as u8;
    let grid_col = if dark_mode {
        Color32::from_rgba_unmultiplied(72, 168, 242, grid_alpha)
    } else {
        Color32::from_rgba_unmultiplied(74, 138, 212, grid_alpha)
    };

    for ix in -7..=7 {
        let top_x = vanish_x + ix as f32 * (rect.width() * 0.022);
        let bot_x = vanish_x + ix as f32 * (rect.width() * 0.125);
        painter.line_segment(
            [pos2(top_x, horizon_y), pos2(bot_x, rect.bottom())],
            Stroke::new(1.0, grid_col),
        );
    }

    let scroll = (t * 0.25).fract();
    for iz in 0..6 {
        let u = ((iz as f32 + scroll) / 6.0).powf(1.85);
        let y = horizon_y + (rect.bottom() - horizon_y) * u;
        let fade = ((u * (grid_alpha as f32)) as u8).max(4);
        let row_col =
            Color32::from_rgba_unmultiplied(grid_col.r(), grid_col.g(), grid_col.b(), fade);
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.0, row_col),
        );
    }

    // Floating 3D Voxel Motes (Parallax Layer 3)
    for i in 0..14usize {
        let seed = i as f32 * 1.425;
        let depth_layer = (i % 3) as f32 + 1.0;
        let speed = 0.018 + depth_layer * 0.008;
        let ux = ((seed * 0.37 + (t * 0.07 + seed).sin() * 0.04).fract() + 1.0).fract();
        let uy = (1.0 - (seed * 0.53 + t * speed).fract()).clamp(0.0, 1.0);
        let px = rect.left() + ux * rect.width() - pointer_norm.x * (depth_layer * 14.0);
        let py = rect.top() + uy * rect.height() - pointer_norm.y * (depth_layer * 10.0);
        let half = 1.7 + depth_layer * 1.1;
        let spin = t * 0.85 + seed;
        let c_a = (((1.0 - (uy - 0.5).abs() * 1.8).clamp(0.15, 1.0))
            * (if dark_mode { 58.0 } else { 52.0 })
            * alpha_mul) as u8;

        let mote_col = match i % 3 {
            0 => Color32::from_rgba_unmultiplied(20, 184, 102, c_a),
            1 => Color32::from_rgba_unmultiplied(234, 158, 24, c_a),
            _ => Color32::from_rgba_unmultiplied(14, 165, 233, c_a),
        };
        let dx = spin.cos() * half;
        let dy = spin.sin() * half;
        let diamond = vec![
            pos2(px + dx, py + dy),
            pos2(px - dy, py + dx),
            pos2(px - dx, py - dy),
            pos2(px + dy, py - dx),
        ];
        painter.add(Shape::convex_polygon(diamond, mote_col, Stroke::NONE));
    }
}

pub fn gaming_card_3d<R>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    selected: bool,
    accent: Color32,
    inner_margin: egui::Margin,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (R, Response) {
    let card_id = ui.make_persistent_id(id_salt);
    let dark = ui.visuals().dark_mode;

    let hover_prev = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(card_id.with("hov")))
        .unwrap_or(false);
    let lift = ui
        .ctx()
        .animate_bool_with_time(card_id.with("lift"), hover_prev || selected, 0.16);

    let base_fill = if dark {
        if selected {
            lerp_color(BG_CARD_ELEVATED_DARK, accent, 0.22)
        } else if hover_prev {
            BG_CARD_ELEVATED_DARK
        } else {
            BG_CARD_DARK
        }
    } else if selected {
        lerp_color(BG_CARD_LIGHT, accent, 0.14)
    } else if hover_prev {
        Color32::from_rgb(245, 250, 255)
    } else {
        BG_CARD_LIGHT
    };

    let stroke_col = if selected {
        accent
    } else if hover_prev {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 195)
    } else if dark {
        ui.visuals().widgets.noninteractive.bg_stroke.color
    } else {
        BORDER_GAME_OUTER_LIGHT
    };

    let shadow_offset_y = (2.0 + lift * 3.0) as i8;
    let shadow_alpha = if dark {
        (80.0 + lift * 45.0) as u8
    } else {
        (20.0 + lift * 18.0) as u8
    };

    let cr = CornerRadius::same(12);
    let inner = egui::Frame::NONE
        .fill(base_fill)
        .stroke(Stroke::new(if selected { 1.8 } else { 1.2 }, stroke_col))
        .corner_radius(cr)
        .inner_margin(inner_margin)
        .shadow(egui::epaint::Shadow {
            offset: [0, shadow_offset_y],
            blur: (8.0 + lift * 6.0) as u8,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(20, 38, 64, shadow_alpha),
        })
        .show(ui, add_contents);

    let rect = inner.response.rect;
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let hovered = pointer.is_some_and(|p| rect.contains(p));
    ui.ctx()
        .data_mut(|d| d.insert_temp(card_id.with("hov"), hovered));

    let painter = ui.painter();

    // Tactile Game-Engine 3D Stepped Bottom Lip & Specular Top Rim
    let bottom_lip_col = if selected {
        shade_color(accent, 0.78)
    } else if dark {
        Color32::from_rgba_unmultiplied(14, 20, 34, 165)
    } else {
        BORDER_GAME_LIP_LIGHT
    };
    painter.line_segment(
        [
            pos2(rect.left() + 10.0, rect.bottom() - 1.2),
            pos2(rect.right() - 10.0, rect.bottom() - 1.2),
        ],
        Stroke::new(2.0, bottom_lip_col),
    );

    let bevel_alpha = if dark { 42 } else { 210 };
    painter.line_segment(
        [
            pos2(rect.left() + 12.0, rect.top() + 1.2),
            pos2(rect.right() - 12.0, rect.top() + 1.2),
        ],
        Stroke::new(
            1.2,
            Color32::from_rgba_unmultiplied(255, 255, 255, bevel_alpha),
        ),
    );

    if selected && rect.height() >= 24.0 {
        let pill_h = (rect.height() * 0.48).clamp(12.0, 28.0);
        let bar =
            Rect::from_center_size(pos2(rect.left() + 2.5, rect.center().y), vec2(3.5, pill_h));
        painter.rect_filled(bar, CornerRadius::same(2), accent);
    }

    if !is_low_ram_mode() {
        if let Some(p) = pointer.filter(|p| rect.contains(*p)) {
            let nx = ((p.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
            let smooth_nx = ui
                .ctx()
                .animate_value_with_time(card_id.with("sheen_x"), nx, 0.12);
            let sheen_cx = rect.left() + smooth_nx * rect.width();
            let half_w = (rect.width() * 0.16).clamp(24.0, 88.0);
            let x0 = (sheen_cx - half_w).max(rect.left() + 4.0);
            let x1 = (sheen_cx + half_w).min(rect.right() - 4.0);
            if x1 > x0 + 4.0 {
                let sheen_alpha = if dark { 18 } else { 30 };
                let poly = vec![
                    pos2(x0, rect.top() + 1.0),
                    pos2(x1, rect.top() + 1.0),
                    pos2((x1 - 14.0).max(rect.left() + 4.0), rect.bottom() - 1.0),
                    pos2((x0 - 14.0).max(rect.left() + 4.0), rect.bottom() - 1.0),
                ];
                painter.add(Shape::convex_polygon(
                    poly,
                    Color32::from_rgba_unmultiplied(
                        accent.r(),
                        accent.g(),
                        accent.b(),
                        sheen_alpha,
                    ),
                    Stroke::NONE,
                ));
                painter.line_segment(
                    [pos2(x0, rect.top() + 1.0), pos2(x1, rect.top() + 1.0)],
                    Stroke::new(
                        1.6,
                        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 195),
                    ),
                );
            }
        }
    }

    (inner.inner, inner.response)
}

pub fn tactile_button_3d(
    ui: &mut Ui,
    icon: Option<VectorIcon>,
    label: impl Into<String>,
    style: ButtonStyle3d,
    min_size: Vec2,
) -> Response {
    let text = label.into();
    let font_size = match style {
        ButtonStyle3d::HeroLaunch | ButtonStyle3d::HeroStop => 15.5,
        _ => 13.0,
    };
    let font_id = FontId::proportional(font_size);
    let galley = ui
        .painter()
        .layout_no_wrap(text.clone(), font_id.clone(), Color32::WHITE);

    let has_text = !text.is_empty();
    let icon_space = match (icon.is_some(), has_text) {
        (true, true) => 20.0,
        (true, false) => 15.0,
        (false, _) => 0.0,
    };
    let pad = match style {
        ButtonStyle3d::HeroLaunch | ButtonStyle3d::HeroStop => vec2(22.0, 12.0),
        _ if !has_text => vec2(9.0, 7.5),
        _ => vec2(14.0, 7.5),
    };
    let text_w = if has_text { galley.size().x } else { 0.0 };
    let mut desired = vec2(
        (text_w + icon_space + pad.x * 2.0).max(min_size.x),
        (galley.size().y + pad.y * 2.0).max(min_size.y),
    );
    let cap_w = if ui.layout().main_wrap() {
        ui.max_rect().width()
    } else {
        ui.available_width().min(ui.max_rect().width())
    };
    if cap_w.is_finite() && cap_w >= 44.0 && desired.x > cap_w {
        desired.x = cap_w;
    }

    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
    let a11y_fallback = match icon {
        Some(VectorIcon::List) => "List",
        Some(VectorIcon::Grid) => "Grid",
        Some(VectorIcon::Close) => "Close",
        Some(VectorIcon::Search) => "Search",
        Some(VectorIcon::Refresh) => "Refresh",
        Some(VectorIcon::Plus) => "Add",
        _ => "Button",
    };
    resp.widget_info(|| {
        let a11y = if text.is_empty() {
            a11y_fallback
        } else {
            text.as_str()
        };
        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), a11y)
    });
    if !ui.is_rect_visible(rect) {
        return resp;
    }

    let enabled = ui.is_enabled();
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(resp.id.with("hov"), enabled && resp.hovered(), 0.14);
    let press_t = ui.ctx().animate_bool_with_time(
        resp.id.with("prs"),
        enabled && resp.is_pointer_button_down_on(),
        0.09,
    );

    let scale = 1.0 - press_t * 0.032 + hover_t * 0.012;
    let scaled_size = rect.size() * scale;
    let press_dy = press_t * 2.2;
    let draw_rect = Rect::from_center_size(
        pos2(rect.center().x, rect.center().y + press_dy),
        scaled_size,
    );

    let dark = ui.visuals().dark_mode;
    let active_preset = active_accent_preset(ui.ctx());
    let (plate, plate_tint, text_col, shadow_col) = match style {
        ButtonStyle3d::PrimaryEmerald | ButtonStyle3d::HeroLaunch => (
            UiButtonPlate::for_accent(active_preset),
            lerp_color(Color32::from_rgb(242, 248, 252), Color32::WHITE, hover_t),
            Color32::WHITE,
            Some(Color32::from_rgba_unmultiplied(12, 28, 44, 185)),
        ),
        ButtonStyle3d::HeroStop | ButtonStyle3d::DangerRose => (
            UiButtonPlate::Ruby,
            lerp_color(Color32::from_rgb(244, 244, 248), Color32::WHITE, hover_t),
            Color32::WHITE,
            Some(Color32::from_rgba_unmultiplied(52, 10, 22, 190)),
        ),
        ButtonStyle3d::AccentViolet => (
            UiButtonPlate::Amber,
            lerp_color(Color32::from_rgb(246, 246, 248), Color32::WHITE, hover_t),
            Color32::WHITE,
            Some(Color32::from_rgba_unmultiplied(54, 28, 8, 185)),
        ),
        ButtonStyle3d::SecondaryGlass => {
            if dark {
                (
                    UiButtonPlate::Sky,
                    lerp_color(
                        Color32::from_rgb(84, 108, 144),
                        Color32::from_rgb(104, 134, 178),
                        hover_t,
                    ),
                    Color32::from_rgb(240, 246, 255),
                    Some(Color32::from_rgba_unmultiplied(10, 18, 30, 165)),
                )
            } else {
                (
                    UiButtonPlate::Sky,
                    lerp_color(Color32::from_rgb(245, 250, 255), Color32::WHITE, hover_t),
                    TEXT_INK_LIGHT,
                    Some(Color32::from_rgba_unmultiplied(255, 255, 255, 165)),
                )
            }
        }
    };

    let effective_tint = if enabled {
        plate_tint
    } else {
        Color32::from_rgba_unmultiplied(
            plate_tint.r() / 2 + 64,
            plate_tint.g() / 2 + 64,
            plate_tint.b() / 2 + 64,
            165,
        )
    };

    let painter = ui.painter();
    paint_nine_slice_button(painter, draw_rect, plate, effective_tint);

    // Animated sunlight sweep across HeroLaunch button
    if style == ButtonStyle3d::HeroLaunch && enabled && !is_low_ram_mode() {
        let t = ui.input(|i| i.time) as f32;
        ui.ctx().request_repaint_after(Duration::from_millis(20));
        let sweep = ((t * 0.55).fract() * 1.4) - 0.2;
        let sx = draw_rect.left() + sweep * draw_rect.width();
        let x0 = (sx - 15.0).clamp(draw_rect.left() + 6.0, draw_rect.right() - 6.0);
        let x1 = (sx + 15.0).clamp(draw_rect.left() + 6.0, draw_rect.right() - 6.0);
        if x1 > x0 + 2.0 {
            let poly = vec![
                pos2(x0, draw_rect.top() + 3.0),
                pos2(x1, draw_rect.top() + 3.0),
                pos2(
                    (x1 - 10.0).max(draw_rect.left() + 6.0),
                    draw_rect.bottom() - 5.0,
                ),
                pos2(
                    (x0 - 10.0).max(draw_rect.left() + 6.0),
                    draw_rect.bottom() - 5.0,
                ),
            ];
            painter.add(Shape::convex_polygon(
                poly,
                Color32::from_rgba_unmultiplied(255, 255, 255, 42),
                Stroke::NONE,
            ));
        }
    }

    if resp.has_focus() {
        let cr = match style {
            ButtonStyle3d::HeroLaunch | ButtonStyle3d::HeroStop => CornerRadius::same(11),
            _ => CornerRadius::same(8),
        };
        painter.rect_stroke(
            draw_rect.expand(2.0),
            cr,
            Stroke::new(2.0, ACCENT_CYAN),
            StrokeKind::Outside,
        );
    }

    // Center content visually inside the top button face (slightly above bottom 3D bevel lip)
    let content_cy = draw_rect.center().y - (1.0 - press_t) * 0.8;
    let content_w = text_w + icon_space;
    let inner_cap = (draw_rect.width() - 12.0).max(16.0);
    let clipped_content = content_w > inner_cap;
    let start_x = if clipped_content {
        draw_rect.left() + 7.0
    } else {
        draw_rect.center().x - content_w * 0.5
    };
    let content_painter = if clipped_content {
        painter.with_clip_rect(ui.clip_rect().intersect(draw_rect.shrink2(vec2(5.0, 1.0))))
    } else {
        painter.clone()
    };
    let text_x = if let Some(ic) = icon {
        let ic_center_x = if has_text {
            start_x + 8.0
        } else {
            draw_rect.center().x
        };
        if let Some(ai_ic) = ic.ai_asset() {
            let sz = match style {
                ButtonStyle3d::HeroLaunch | ButtonStyle3d::HeroStop => 20.0,
                _ => 17.0,
            };
            let ic_rect = Rect::from_center_size(pos2(ic_center_x, content_cy), Vec2::splat(sz));
            let yaw = hover_t * 0.16;
            paint_tilted_ai_icon(
                &content_painter,
                ic_rect,
                ai_ic,
                -hover_t * 0.08,
                yaw,
                0.0,
                Color32::WHITE,
            );
        } else {
            let ic_rect = Rect::from_center_size(pos2(ic_center_x, content_cy), Vec2::splat(15.0));
            if let Some(sc) = shadow_col {
                paint_vector_icon(&content_painter, ic_rect.translate(vec2(0.0, 1.0)), ic, sc);
            }
            paint_vector_icon(&content_painter, ic_rect, ic, text_col);
        }
        start_x + icon_space
    } else {
        start_x
    };

    if has_text {
        if let Some(sc) = shadow_col {
            content_painter.text(
                pos2(text_x, content_cy + 1.0),
                Align2::LEFT_CENTER,
                &text,
                font_id.clone(),
                sc,
            );
        }
        content_painter.text(
            pos2(text_x, content_cy),
            Align2::LEFT_CENTER,
            text,
            font_id,
            text_col,
        );
    }

    resp
}

pub fn sliding_pill_tabs<T: Copy + PartialEq>(
    ui: &mut Ui,
    bar_id_salt: impl std::hash::Hash + std::fmt::Debug,
    tabs: &[(T, VectorIcon, String)],
    current: T,
) -> Option<T> {
    let bar_id = ui.make_persistent_id(bar_id_salt);
    let dark = ui.visuals().dark_mode;
    let mut clicked_tab = None;

    egui::Frame::NONE
        .fill(if dark {
            Color32::from_rgb(22, 31, 47)
        } else {
            BG_SLOT_LIGHT
        })
        .stroke(Stroke::new(
            1.2,
            if dark {
                ui.visuals().widgets.noninteractive.bg_stroke.color
            } else {
                BORDER_GAME_OUTER_LIGHT
            },
        ))
        .corner_radius(CornerRadius::same(11))
        .inner_margin(egui::Margin::symmetric(6, 5))
        .show(ui, |ui| {
            let bar_origin = ui.min_rect().min;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                let mut active_rect: Option<Rect> = None;
                let mut tab_visuals = Vec::with_capacity(tabs.len());

                for (val, icon, title) in tabs {
                    let is_sel = *val == current;
                    let font = FontId::proportional(12.6);
                    let g =
                        ui.painter()
                            .layout_no_wrap(title.clone(), font.clone(), Color32::WHITE);
                    let max_tab_w = ui.max_rect().width().max(48.0);
                    let sz = vec2((g.size().x + 34.0).min(max_tab_w), 30.0);
                    let (rect, resp) = ui.allocate_exact_size(sz, Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Tab, ui.is_enabled(), is_sel, title)
                    });
                    if is_sel {
                        active_rect = Some(rect);
                    }
                    if resp.clicked() && !is_sel {
                        clicked_tab = Some(*val);
                    }
                    tab_visuals.push((rect, resp.hovered(), is_sel, *icon, title.clone(), font));
                }

                if let Some(target) = active_rect {
                    let rel_x = target.min.x - bar_origin.x;
                    let rel_y = target.min.y - bar_origin.y;
                    let ax = bar_origin.x
                        + ui.ctx()
                            .animate_value_with_time(bar_id.with("pill_x"), rel_x, 0.17);
                    let ay = bar_origin.y
                        + ui.ctx()
                            .animate_value_with_time(bar_id.with("pill_y"), rel_y, 0.17);
                    let aw = ui.ctx().animate_value_with_time(
                        bar_id.with("pill_w"),
                        target.width(),
                        0.17,
                    );
                    let pill = Rect::from_min_size(pos2(ax, ay), vec2(aw, target.height()));
                    paint_nine_slice_button(
                        ui.painter(),
                        pill,
                        UiButtonPlate::for_accent(active_accent_preset(ui.ctx())),
                        Color32::WHITE,
                    );
                }

                for (rect, hovered, is_sel, icon, title, font) in tab_visuals {
                    if hovered && !is_sel {
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same(8),
                            if dark {
                                Color32::from_rgba_unmultiplied(148, 174, 212, 34)
                            } else {
                                Color32::from_rgba_unmultiplied(255, 255, 255, 165)
                            },
                        );
                    }
                    let cy = if is_sel {
                        rect.center().y - 0.6
                    } else {
                        rect.center().y
                    };
                    let fg = if is_sel {
                        Color32::WHITE
                    } else if hovered {
                        ui.visuals().strong_text_color()
                    } else {
                        ui.visuals().text_color()
                    };
                    let ic_rect =
                        Rect::from_center_size(pos2(rect.left() + 14.0, cy), Vec2::splat(13.5));
                    if let Some(ai_ic) = icon.ai_asset() {
                        paint_tilted_ai_icon(
                            ui.painter(),
                            ic_rect.expand(1.5),
                            ai_ic,
                            0.0,
                            0.0,
                            0.0,
                            Color32::WHITE,
                        );
                    } else {
                        if is_sel {
                            paint_vector_icon(
                                ui.painter(),
                                ic_rect.translate(vec2(0.0, 1.0)),
                                icon,
                                Color32::from_rgba_unmultiplied(12, 28, 44, 165),
                            );
                        }
                        paint_vector_icon(ui.painter(), ic_rect, icon, fg);
                    }
                    let txt_painter = ui
                        .painter()
                        .with_clip_rect(ui.clip_rect().intersect(rect.shrink2(vec2(4.0, 0.0))));
                    if is_sel {
                        txt_painter.text(
                            pos2(rect.left() + 25.0, cy + 1.0),
                            Align2::LEFT_CENTER,
                            &title,
                            font.clone(),
                            Color32::from_rgba_unmultiplied(12, 28, 44, 175),
                        );
                    }
                    txt_painter.text(
                        pos2(rect.left() + 25.0, cy),
                        Align2::LEFT_CENTER,
                        title,
                        font,
                        fg,
                    );
                }
            });
        });

    clicked_tab
}

pub fn section_enter_offset(ctx: &egui::Context, section_key: u64) -> (f32, f32) {
    if is_low_ram_mode() {
        return (1.0, 0.0);
    }
    let id = Id::unique(("thk_sec_anim", section_key));
    let last_key_id = Id::unique("thk_last_sec_key");
    let prev = ctx.data(|d| d.get_temp::<u64>(last_key_id));
    if prev != Some(section_key) {
        ctx.data_mut(|d| {
            d.insert_temp(last_key_id, section_key);
            d.insert_temp(id, 0.0f32);
        });
        let _ = ctx.animate_value_with_time(id, 0.0, 0.0);
    }
    let progress = ctx.animate_value_with_time(id, 1.0, 0.18).clamp(0.0, 1.0);
    let y_offset = (1.0 - progress) * 6.0;
    (progress, y_offset)
}

pub fn paint_3d_isometric_bars(
    ui: &mut Ui,
    desired_height: f32,
    bars: &[(String, u64, String)],
    accent: Color32,
) {
    let desired = vec2(ui.available_width(), desired_height);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    if !ui.is_rect_visible(rect) || bars.is_empty() {
        return;
    }

    let dark = ui.visuals().dark_mode;
    let painter = ui.painter_at(rect);
    let max_val = bars.iter().map(|(_, v, _)| *v).max().unwrap_or(1).max(60) as f32;

    let pointer = ui.input(|i| i.pointer.hover_pos());
    let n = bars.len().max(1) as f32;
    let slot_w = (rect.width() - 16.0) / n;
    let bar_w = (slot_w * 0.48).clamp(10.0, 34.0);
    let iso_dx = (bar_w * 0.34).clamp(4.0, 10.0);
    let iso_dy = (bar_w * 0.22).clamp(3.0, 7.0);
    let floor_y = rect.bottom() - 24.0;
    let max_bar_h = (rect.height() - 46.0).max(32.0);

    let guide_col = if dark {
        Color32::from_rgba_unmultiplied(148, 163, 184, 30)
    } else {
        Color32::from_rgba_unmultiplied(100, 130, 168, 48)
    };
    for step in 0..=3 {
        let gy = floor_y - max_bar_h * (step as f32 / 3.0);
        painter.line_segment(
            [pos2(rect.left() + 4.0, gy), pos2(rect.right() - 4.0, gy)],
            Stroke::new(1.0, guide_col),
        );
    }

    let mut hovered_tooltip: Option<(Pos2, String)> = None;

    for (i, (short_label, val, full_tooltip)) in bars.iter().enumerate() {
        let ratio = (*val as f32 / max_val).clamp(0.04, 1.0);
        let anim_r = ui.ctx().animate_value_with_time(
            Id::unique(("thk_bar3d", i, short_label.as_str())),
            ratio,
            0.26,
        );
        let h = max_bar_h * anim_r;
        let cx = rect.left() + 8.0 + slot_w * (i as f32 + 0.5);
        let x0 = cx - bar_w * 0.5;
        let x1 = cx + bar_w * 0.5;
        let y_top = floor_y - h;

        let front_rect = Rect::from_min_max(pos2(x0, y_top), pos2(x1, floor_y));
        let is_hov = pointer.is_some_and(|p| front_rect.expand(4.0).contains(p));
        let front_col = if is_hov {
            shade_color(accent, 1.15)
        } else {
            accent
        };
        let side_col = shade_color(front_col, 0.68);
        let top_col = shade_color(front_col, 1.28);
        let contour = Stroke::new(
            0.8,
            if dark {
                Color32::from_rgba_unmultiplied(15, 23, 42, 140)
            } else {
                Color32::from_rgba_unmultiplied(27, 40, 56, 110)
            },
        );

        painter.add(Shape::convex_polygon(
            vec![
                pos2(x0, y_top),
                pos2(x1, y_top),
                pos2(x1, floor_y),
                pos2(x0, floor_y),
            ],
            front_col,
            contour,
        ));
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x1, y_top),
                pos2(x1 + iso_dx, y_top - iso_dy),
                pos2(x1 + iso_dx, floor_y - iso_dy),
                pos2(x1, floor_y),
            ],
            side_col,
            contour,
        ));
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x0 + iso_dx, y_top - iso_dy),
                pos2(x1 + iso_dx, y_top - iso_dy),
                pos2(x1, y_top),
                pos2(x0, y_top),
            ],
            top_col,
            contour,
        ));

        painter.text(
            pos2(cx, rect.bottom() - 9.0),
            Align2::CENTER_CENTER,
            short_label,
            FontId::proportional(10.5),
            ui.visuals().text_color(),
        );

        if is_hov {
            hovered_tooltip = Some((pos2(cx, y_top - iso_dy - 12.0), full_tooltip.clone()));
        }
    }

    if let Some((tip_pos, tip_txt)) = hovered_tooltip {
        let g = painter.layout_no_wrap(tip_txt.clone(), FontId::proportional(11.5), Color32::WHITE);
        let box_r = Rect::from_center_size(tip_pos, g.size() + vec2(12.0, 6.0));
        painter.rect_filled(
            box_r,
            CornerRadius::same(6),
            Color32::from_rgba_unmultiplied(20, 32, 50, 242),
        );
        painter.rect_stroke(
            box_r,
            CornerRadius::same(6),
            Stroke::new(1.2, accent),
            StrokeKind::Inside,
        );
        painter.text(
            box_r.center(),
            Align2::CENTER_CENTER,
            tip_txt,
            FontId::proportional(11.5),
            Color32::WHITE,
        );
    }
}

pub fn paint_3d_tunnel_conduit(
    ui: &mut Ui,
    active: bool,
    connecting: bool,
    local_label: &str,
    remote_label: &str,
) {
    let desired = vec2(ui.available_width(), 68.0);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let t = ui.input(|i| i.time) as f32;
    if active || connecting {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    let dark = ui.visuals().dark_mode;
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        CornerRadius::same(10),
        if dark {
            Color32::from_rgb(20, 29, 44)
        } else {
            BG_SLOT_LIGHT
        },
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(
            1.3,
            if active {
                ACCENT_EMERALD
            } else if connecting {
                ACCENT_AMBER
            } else if dark {
                Color32::from_rgba_unmultiplied(100, 116, 139, 110)
            } else {
                BORDER_GAME_OUTER_LIGHT
            },
        ),
        StrokeKind::Inside,
    );

    let node_w = if rect.width() < 236.0 {
        ((rect.width() - 28.0) * 0.45).clamp(56.0, 96.0)
    } else {
        (rect.width() * 0.34).clamp(96.0, 146.0)
    };
    let half_nw = node_w * 0.5;
    let side_pad = if rect.width() < 236.0 { 8.0 } else { 12.0 };
    let left_node = pos2(rect.left() + half_nw + side_pad, rect.center().y);
    let right_node = pos2(rect.right() - half_nw - side_pad, rect.center().y);
    let line_col = if active {
        Color32::from_rgba_unmultiplied(20, 184, 102, 130)
    } else if connecting {
        Color32::from_rgba_unmultiplied(234, 138, 12, 130)
    } else {
        Color32::from_rgba_unmultiplied(100, 130, 168, 90)
    };

    let x_start = left_node.x + half_nw + 4.0;
    let x_end = right_node.x - half_nw - 4.0;
    if x_end > x_start + 4.0 {
        painter.line_segment(
            [pos2(x_start, left_node.y), pos2(x_end, right_node.y)],
            Stroke::new(2.4, line_col),
        );
    }

    if (active || connecting) && x_end > x_start + 12.0 {
        for pkt in 0..4 {
            let u = (t * 0.65 + pkt as f32 * 0.25).fract();
            let px = x_start + (x_end - x_start) * u;
            let col = if active { ACCENT_CYAN } else { ACCENT_AMBER };
            painter.circle_filled(pos2(px, left_node.y), 4.2, col);
            painter.circle_filled(
                pos2(px, left_node.y),
                8.0,
                Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 52),
            );
        }
    }

    let node_bg = if dark { BG_CARD_DARK } else { BG_CARD_LIGHT };
    let node_fg = if dark { Color32::WHITE } else { TEXT_INK_LIGHT };

    for (nc, lbl, col) in [
        (left_node, local_label, ACCENT_BLUE),
        (
            right_node,
            remote_label,
            if active {
                ACCENT_EMERALD
            } else {
                BORDER_GAME_OUTER_LIGHT
            },
        ),
    ] {
        let nr = Rect::from_center_size(nc, vec2(node_w, 36.0));
        painter.rect_filled(nr, CornerRadius::same(8), node_bg);
        painter.rect_stroke(
            nr,
            CornerRadius::same(8),
            Stroke::new(1.4, col),
            StrokeKind::Inside,
        );
        let node_painter =
            painter.with_clip_rect(ui.clip_rect().intersect(nr.shrink2(vec2(3.0, 0.0))));
        node_painter.text(
            nr.center(),
            Align2::CENTER_CENTER,
            lbl,
            FontId::monospace(11.2),
            node_fg,
        );
    }
}

pub fn paint_glowing_progress_bar(ui: &mut Ui, fraction: f32, width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let t = ui.input(|i| i.time) as f32;
    ui.ctx().request_repaint_after(Duration::from_millis(16));

    let dark = ui.visuals().dark_mode;
    let painter = ui.painter();
    let cr = CornerRadius::same((height * 0.5) as u8);
    painter.rect_filled(
        rect,
        cr,
        if dark {
            Color32::from_rgb(18, 26, 40)
        } else {
            Color32::from_rgb(208, 222, 240)
        },
    );
    painter.rect_stroke(
        rect,
        cr,
        Stroke::new(
            1.2,
            if dark {
                Color32::from_rgba_unmultiplied(74, 144, 226, 110)
            } else {
                BORDER_GAME_OUTER_LIGHT
            },
        ),
        StrokeKind::Inside,
    );

    let clamped = fraction.clamp(0.0, 1.0);
    let fill_w = (rect.width() * clamped).max(0.0);
    if fill_w > 2.0 {
        let fill_rect = Rect::from_min_size(rect.min, vec2(fill_w, rect.height()));
        painter.rect_filled(fill_rect, cr, ACCENT_EMERALD);

        let pulse_x = fill_rect.left() + (t * 0.9).fract() * fill_w;
        let glow_w = 24.0f32.min(fill_w * 0.5);
        let gx0 = (pulse_x - glow_w).max(fill_rect.left());
        let gx1 = (pulse_x + glow_w).min(fill_rect.right());
        if gx1 > gx0 + 2.0 {
            let glow_rect = Rect::from_min_max(
                pos2(gx0, fill_rect.top() + 1.0),
                pos2(gx1, fill_rect.bottom() - 1.0),
            );
            painter.rect_filled(
                glow_rect,
                cr,
                Color32::from_rgba_unmultiplied(200, 255, 234, 125),
            );
        }
    }

    let pct_txt = format!("{:.0}%", clamped * 100.0);
    let fg = if dark || clamped >= 0.48 {
        painter.text(
            rect.center() + vec2(0.0, 1.0),
            Align2::CENTER_CENTER,
            &pct_txt,
            FontId::proportional(11.0),
            Color32::from_rgba_unmultiplied(12, 26, 42, 165),
        );
        Color32::WHITE
    } else {
        TEXT_INK_LIGHT
    };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        pct_txt,
        FontId::proportional(11.0),
        fg,
    );
}

pub fn shade_color(c: Color32, factor: f32) -> Color32 {
    let r = (c.r() as f32 * factor).clamp(0.0, 255.0) as u8;
    let g = (c.g() as f32 * factor).clamp(0.0, 255.0) as u8;
    let b = (c.b() as f32 * factor).clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, c.a())
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let u = t.clamp(0.0, 1.0);
    let r = (a.r() as f32 + (b.r() as f32 - a.r() as f32) * u) as u8;
    let g = (a.g() as f32 + (b.g() as f32 - a.g() as f32) * u) as u8;
    let bl = (a.b() as f32 + (b.b() as f32 - a.b() as f32) * u) as u8;
    let al = (a.a() as f32 + (b.a() as f32 - a.a() as f32) * u) as u8;
    Color32::from_rgba_unmultiplied(r, g, bl, al)
}
