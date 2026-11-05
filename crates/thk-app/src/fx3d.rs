use crate::theme::{
    ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD, ACCENT_EMERALD_DIM, ACCENT_ORANGE,
    ACCENT_RED, ACCENT_VIOLET, ACCENT_VIOLET_DIM, BG_CARD_DARK, BG_CARD_HOVER_DARK,
};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, Response, Sense, Shape,
    Stroke, StrokeKind, Ui, Vec2,
};
use glam::{Mat4, Vec3};
use std::time::Duration;

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
            let center =
                (world_pts[0] + world_pts[1] + world_pts[2] + world_pts[3]) * 0.25;
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
        self.faces
            .sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));
        for f in self.faces {
            painter.add(Shape::convex_polygon(f.pts.to_vec(), f.fill, f.stroke));
        }
    }
}

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
                Color32::from_rgb(28, 39, 68),
                Color32::from_rgba_unmultiplied(148, 98, 255, 130),
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
                let sat_col = if i == 0 {
                    ACCENT_CYAN
                } else if i == 1 {
                    ACCENT_VIOLET
                } else {
                    ACCENT_EMERALD
                };
                buf.push_cuboid(
                    sat_xf,
                    Vec3::ZERO,
                    Vec3::splat(0.24),
                    sat_col,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 150),
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
                Color32::from_rgb(35, 48, 82),
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
                ACCENT_VIOLET,
                Color32::from_rgba_unmultiplied(233, 213, 255, 190),
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
                    Color32::from_rgb(32, 56, 92)
                } else {
                    Color32::from_rgb(24, 34, 58)
                };
                let rim = if active_led {
                    ACCENT_CYAN
                } else {
                    Color32::from_rgba_unmultiplied(100, 130, 180, 120)
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
                ACCENT_EMERALD,
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
                    let seg_xf =
                        tilt * Mat4::from_rotation_y(a) * Mat4::from_translation(Vec3::new(0.0, 0.0, 1.08));
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
                Color32::from_rgb(20, 48, 68),
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
                let xf =
                    outer * Mat4::from_rotation_z(a) * Mat4::from_translation(Vec3::new(0.95, 0.0, 0.0));
                buf.push_cuboid(
                    xf,
                    Vec3::ZERO,
                    Vec3::new(0.24, 0.72, 0.22),
                    ACCENT_VIOLET,
                    Color32::from_rgba_unmultiplied(221, 214, 254, 175),
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
                Color32::WHITE,
                ACCENT_CYAN,
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
    }
    buf
}

pub fn paint_icon_3d(ui: &mut Ui, size: f32, kind: Icon3dKind, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return resp;
    }

    let t = ui.input(|i| i.time) as f32;
    ui.ctx().request_repaint_after(Duration::from_millis(16));

    let painter = ui.painter();
    let c = rect.center();
    let r = size * 0.48;

    // Ground floating shadow + active aura
    let shadow_y = rect.bottom() - size * 0.10;
    painter.circle_filled(
        pos2(c.x, shadow_y),
        size * 0.24,
        Color32::from_rgba_unmultiplied(0, 0, 0, 72),
    );
    if active {
        let pulse = ((t * 3.0).sin() * 0.5 + 0.5) * 0.35 + 0.65;
        painter.circle_filled(
            c,
            r * pulse,
            Color32::from_rgba_unmultiplied(16, 219, 134, 34),
        );
    }

    // Radar expanding pulse rings for LanRadar
    if kind == Icon3dKind::LanRadar {
        for wave in 0..2 {
            let p = ((t * 0.75 + wave as f32 * 0.5).fract()).clamp(0.0, 1.0);
            let wr = size * (0.14 + p * 0.34);
            let alpha = ((1.0 - p) * 155.0) as u8;
            painter.circle_stroke(
                pos2(c.x, c.y + size * 0.14),
                wr,
                Stroke::new(1.2, Color32::from_rgba_unmultiplied(34, 211, 238, alpha)),
            );
        }
    }

    let scene = build_icon_3d_scene(rect, kind, t);
    scene.flush(painter);

    // Laser scan beam overlay for ModpackCrate
    if kind == Icon3dKind::ModpackCrate {
        let scan_y = c.y + (t * 2.8).sin() * (size * 0.20);
        painter.line_segment(
            [
                pos2(c.x - size * 0.28, scan_y),
                pos2(c.x + size * 0.28, scan_y),
            ],
            Stroke::new(1.4, Color32::from_rgba_unmultiplied(34, 211, 238, 195)),
        );
    }

    resp
}

pub fn paint_vector_icon(painter: &egui::Painter, rect: Rect, icon: VectorIcon, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let st = Stroke::new(1.65, color);

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
                [pos2(c.x - r * 0.55, c.y - r * 0.05), pos2(c.x, c.y + r * 0.5)],
                st,
            );
            painter.line_segment(
                [pos2(c.x + r * 0.55, c.y - r * 0.05), pos2(c.x, c.y + r * 0.5)],
                st,
            );
            painter.line_segment(
                [pos2(c.x - r * 0.85, c.y + r * 0.85), pos2(c.x + r * 0.85, c.y + r * 0.85)],
                st,
            );
        }
        VectorIcon::Upload => {
            painter.line_segment([pos2(c.x, c.y + r * 0.6), pos2(c.x, c.y - r * 0.65)], st);
            painter.line_segment(
                [pos2(c.x - r * 0.55, c.y - r * 0.1), pos2(c.x, c.y - r * 0.68)],
                st,
            );
            painter.line_segment(
                [pos2(c.x + r * 0.55, c.y - r * 0.1), pos2(c.x, c.y - r * 0.68)],
                st,
            );
            painter.line_segment(
                [pos2(c.x - r * 0.85, c.y + r * 0.88), pos2(c.x + r * 0.85, c.y + r * 0.88)],
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
                [pos2(c.x - r * 0.9, c.y - r * 0.45), pos2(c.x - r * 0.25, c.y - r * 0.78)],
                st,
            );
            painter.line_segment(
                [pos2(c.x - r * 0.25, c.y - r * 0.78), pos2(c.x + r * 0.2, c.y - r * 0.45)],
                st,
            );
        }
        VectorIcon::Search => {
            let gc = pos2(c.x - r * 0.18, c.y - r * 0.18);
            painter.circle_stroke(gc, r * 0.62, st);
            painter.line_segment(
                [pos2(gc.x + r * 0.44, gc.y + r * 0.44), pos2(c.x + r * 0.85, c.y + r * 0.85)],
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
                [pos2(c.x - r * 0.85, c.y - r * 0.55), pos2(c.x + r * 0.85, c.y - r * 0.55)],
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
                [pos2(c.x - r * 0.78, c.y - r * 0.48), pos2(c.x - r * 0.18, c.y)],
                st,
            );
            painter.line_segment(
                [pos2(c.x - r * 0.18, c.y), pos2(c.x - r * 0.78, c.y + r * 0.48)],
                st,
            );
            painter.line_segment(
                [pos2(c.x + r * 0.05, c.y + r * 0.52), pos2(c.x + r * 0.82, c.y + r * 0.52)],
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
            painter.line_segment([pos2(ring_c.x + r * 0.45, c.y), pos2(c.x + r * 0.88, c.y)], st);
            painter.line_segment(
                [pos2(c.x + r * 0.62, c.y), pos2(c.x + r * 0.62, c.y + r * 0.36)],
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
                [pos2(c.x - r * 0.18, c.y - r * 0.55), pos2(c.x - r * 0.72, c.y)],
                st,
            );
            painter.line_segment(
                [pos2(c.x - r * 0.18, c.y + r * 0.55), pos2(c.x - r * 0.72, c.y)],
                st,
            );
        }
        VectorIcon::ArrowRight => {
            painter.line_segment([pos2(c.x - r * 0.75, c.y), pos2(c.x + r * 0.68, c.y)], st);
            painter.line_segment(
                [pos2(c.x + r * 0.18, c.y - r * 0.55), pos2(c.x + r * 0.72, c.y)],
                st,
            );
            painter.line_segment(
                [pos2(c.x + r * 0.18, c.y + r * 0.55), pos2(c.x + r * 0.72, c.y)],
                st,
            );
        }
        VectorIcon::Wrench => {
            painter.line_segment(
                [pos2(c.x - r * 0.72, c.y + r * 0.72), pos2(c.x + r * 0.35, c.y - r * 0.35)],
                Stroke::new(2.0, color),
            );
            painter.circle_stroke(pos2(c.x + r * 0.45, c.y - r * 0.45), r * 0.42, st);
        }
        VectorIcon::Sliders => {
            for (idx, ox) in [-0.58f32, 0.0, 0.58].into_iter().enumerate() {
                let x = c.x + r * ox;
                painter.line_segment([pos2(x, c.y - r * 0.8), pos2(x, c.y + r * 0.8)], st);
                let ky = if idx == 1 { c.y - r * 0.3 } else { c.y + r * 0.25 };
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
                [pos2(c.x - r * 0.82, c.y - r * 0.42), pos2(c.x - r * 0.82, c.y + r * 0.45)],
                st,
            );
            painter.line_segment(
                [pos2(c.x + r * 0.82, c.y - r * 0.42), pos2(c.x + r * 0.82, c.y + r * 0.45)],
                st,
            );
            painter.line_segment(
                [pos2(c.x - r * 0.82, c.y + r * 0.45), pos2(c.x, c.y + r * 0.92)],
                st,
            );
            painter.line_segment(
                [pos2(c.x + r * 0.82, c.y + r * 0.45), pos2(c.x, c.y + r * 0.92)],
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
                [pos2(c.x - r * 0.68, c.y - r * 0.68), pos2(c.x + r * 0.68, c.y + r * 0.68)],
                st,
            );
            painter.line_segment(
                [pos2(c.x + r * 0.68, c.y - r * 0.68), pos2(c.x - r * 0.68, c.y + r * 0.68)],
                st,
            );
        }
    }
}

pub fn paint_gaming_3d_backdrop(
    ui: &Ui,
    rect: Rect,
    dark_mode: bool,
    has_custom_bg: bool,
) {
    if !ui.is_rect_visible(rect) {
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
    let alpha_mul = if has_custom_bg { 0.42 } else { 1.0 };

    // Ambient volumetric corner nebulae (parallax layer 1)
    let neb_a = ((if dark_mode { 22.0 } else { 14.0 }) * alpha_mul) as u8;
    painter.circle_filled(
        pos2(
            rect.right() - rect.width() * 0.16 - pointer_norm.x * 26.0,
            rect.top() + rect.height() * 0.18 - pointer_norm.y * 18.0,
        ),
        rect.width() * 0.32,
        Color32::from_rgba_unmultiplied(148, 98, 255, neb_a),
    );
    painter.circle_filled(
        pos2(
            rect.left() + rect.width() * 0.20 - pointer_norm.x * 18.0,
            rect.bottom() - rect.height() * 0.15 - pointer_norm.y * 14.0,
        ),
        rect.width() * 0.28,
        Color32::from_rgba_unmultiplied(16, 219, 134, neb_a),
    );

    // 3D perspective voxel horizon grid along lower viewport (parallax layer 2)
    let horizon_y = rect.bottom() - rect.height() * 0.28 + pointer_norm.y * 12.0;
    let vanish_x = rect.center().x - pointer_norm.x * 38.0;
    let grid_alpha = ((if dark_mode { 28.0 } else { 20.0 }) * alpha_mul) as u8;
    let grid_col = if dark_mode {
        Color32::from_rgba_unmultiplied(56, 189, 248, grid_alpha)
    } else {
        Color32::from_rgba_unmultiplied(79, 70, 229, grid_alpha)
    };

    for ix in -7..=7 {
        let top_x = vanish_x + ix as f32 * (rect.width() * 0.022);
        let bot_x = vanish_x + ix as f32 * (rect.width() * 0.125);
        painter.line_segment(
            [pos2(top_x, horizon_y), pos2(bot_x, rect.bottom())],
            Stroke::new(1.0, grid_col),
        );
    }

    let scroll = (t * 0.28).fract();
    for iz in 0..6 {
        let u = ((iz as f32 + scroll) / 6.0).powf(1.85);
        let y = horizon_y + (rect.bottom() - horizon_y) * u;
        let fade = ((u * (grid_alpha as f32)) as u8).max(4);
        let row_col = Color32::from_rgba_unmultiplied(grid_col.r(), grid_col.g(), grid_col.b(), fade);
        painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, row_col));
    }

    // Floating 3D depth-sorted voxel motes (parallax layer 3)
    for i in 0..16usize {
        let seed = i as f32 * 1.375;
        let depth_layer = (i % 3) as f32 + 1.0;
        let speed = 0.020 + depth_layer * 0.009;
        let ux = ((seed * 0.37 + (t * 0.07 + seed).sin() * 0.04).fract() + 1.0).fract();
        let uy = (1.0 - (seed * 0.53 + t * speed).fract()).clamp(0.0, 1.0);
        let px = rect.left() + ux * rect.width() - pointer_norm.x * (depth_layer * 14.0);
        let py = rect.top() + uy * rect.height() - pointer_norm.y * (depth_layer * 10.0);
        let half = 1.6 + depth_layer * 1.15;
        let spin = t * 0.9 + seed;
        let c_a = (((1.0 - (uy - 0.5).abs() * 1.8).clamp(0.15, 1.0))
            * (if dark_mode { 62.0 } else { 44.0 })
            * alpha_mul) as u8;

        let mote_col = match i % 3 {
            0 => Color32::from_rgba_unmultiplied(16, 219, 134, c_a),
            1 => Color32::from_rgba_unmultiplied(148, 98, 255, c_a),
            _ => Color32::from_rgba_unmultiplied(34, 211, 238, c_a),
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
            Color32::from_rgba_unmultiplied(20, 36, 54, 242)
        } else if hover_prev {
            BG_CARD_HOVER_DARK
        } else {
            BG_CARD_DARK
        }
    } else if selected {
        Color32::from_rgb(232, 248, 240)
    } else if hover_prev {
        Color32::from_rgb(248, 251, 255)
    } else {
        Color32::WHITE
    };

    let stroke_col = if selected {
        accent
    } else if hover_prev {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 175)
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke.color
    };

    let shadow_offset_y = (3.0 + lift * 5.0) as i8;
    let shadow_alpha = if dark {
        (95.0 + lift * 65.0) as u8
    } else {
        (22.0 + lift * 24.0) as u8
    };

    let inner = egui::Frame::NONE
        .fill(base_fill)
        .stroke(Stroke::new(if selected { 1.6 } else { 1.0 }, stroke_col))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(inner_margin)
        .shadow(egui::epaint::Shadow {
            offset: [0, shadow_offset_y],
            blur: (12.0 + lift * 10.0) as u8,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(2, 5, 14, shadow_alpha),
        })
        .show(ui, add_contents);

    let rect = inner.response.rect;
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let hovered = pointer.is_some_and(|p| rect.contains(p));
    ui.ctx()
        .data_mut(|d| d.insert_temp(card_id.with("hov"), hovered));

    // Specular top bevel + interactive cursor-tracked 3D perspective sheen
    let painter = ui.painter();
    let bevel_alpha = if dark { 38 } else { 140 };
    painter.line_segment(
        [
            pos2(rect.left() + 12.0, rect.top() + 1.0),
            pos2(rect.right() - 12.0, rect.top() + 1.0),
        ],
        Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, bevel_alpha),
        ),
    );

    if let Some(p) = pointer.filter(|p| rect.contains(*p)) {
        let nx = ((p.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
        let smooth_nx = ui
            .ctx()
            .animate_value_with_time(card_id.with("sheen_x"), nx, 0.12);
        let sheen_cx = rect.left() + smooth_nx * rect.width();
        let half_w = (rect.width() * 0.18).clamp(28.0, 95.0);
        let x0 = (sheen_cx - half_w).max(rect.left() + 4.0);
        let x1 = (sheen_cx + half_w).min(rect.right() - 4.0);
        if x1 > x0 + 4.0 {
            let sheen_alpha = if dark { 20 } else { 45 };
            let poly = vec![
                pos2(x0, rect.top() + 1.0),
                pos2(x1, rect.top() + 1.0),
                pos2((x1 - 14.0).max(rect.left() + 4.0), rect.bottom() - 1.0),
                pos2((x0 - 14.0).max(rect.left() + 4.0), rect.bottom() - 1.0),
            ];
            painter.add(Shape::convex_polygon(
                poly,
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), sheen_alpha),
                Stroke::NONE,
            ));
            painter.line_segment(
                [pos2(x0, rect.top() + 1.0), pos2(x1, rect.top() + 1.0)],
                Stroke::new(
                    1.6,
                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 190),
                ),
            );
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
    let desired = vec2(
        (text_w + icon_space + pad.x * 2.0).max(min_size.x),
        (galley.size().y + pad.y * 2.0).max(min_size.y),
    );

    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }

    let hover_t = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("hov"), resp.hovered(), 0.14);
    let press_t = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("prs"), resp.is_pointer_button_down_on(), 0.10);

    // Subtle 0.97 active scale compression + 3D bottom lip compression
    let scale = 1.0 - press_t * 0.03 + hover_t * 0.012;
    let scaled_size = rect.size() * scale;
    let press_dy = press_t * 2.0;
    let draw_rect = Rect::from_center_size(
        pos2(rect.center().x, rect.center().y + press_dy),
        scaled_size,
    );

    let dark = ui.visuals().dark_mode;
    let (face_col, lip_col, rim_col, text_col) = match style {
        ButtonStyle3d::PrimaryEmerald => (
            lerp_color(Color32::from_rgb(15, 148, 94), Color32::from_rgb(20, 184, 118), hover_t),
            Color32::from_rgb(8, 86, 56),
            ACCENT_EMERALD,
            Color32::WHITE,
        ),
        ButtonStyle3d::HeroLaunch => (
            lerp_color(Color32::from_rgb(14, 165, 104), Color32::from_rgb(22, 204, 130), hover_t),
            Color32::from_rgb(7, 92, 58),
            Color32::from_rgb(110, 255, 198),
            Color32::WHITE,
        ),
        ButtonStyle3d::HeroStop | ButtonStyle3d::DangerRose => (
            lerp_color(Color32::from_rgb(185, 28, 56), Color32::from_rgb(225, 42, 74), hover_t),
            Color32::from_rgb(112, 14, 34),
            ACCENT_RED,
            Color32::WHITE,
        ),
        ButtonStyle3d::AccentViolet => (
            lerp_color(ACCENT_VIOLET_DIM, Color32::from_rgb(109, 40, 217), hover_t),
            Color32::from_rgb(46, 16, 101),
            ACCENT_VIOLET,
            Color32::WHITE,
        ),
        ButtonStyle3d::SecondaryGlass => {
            if dark {
                (
                    lerp_color(BG_CARD_DARK, BG_CARD_HOVER_DARK, hover_t),
                    Color32::from_rgb(10, 14, 26),
                    lerp_color(Color32::from_rgb(52, 68, 108), ACCENT_CYAN, hover_t),
                    Color32::from_rgb(236, 242, 252),
                )
            } else {
                (
                    lerp_color(
                        Color32::from_rgb(236, 242, 250),
                        Color32::from_rgb(224, 235, 248),
                        hover_t,
                    ),
                    Color32::from_rgb(188, 202, 224),
                    lerp_color(
                        Color32::from_rgb(180, 196, 220),
                        Color32::from_rgb(13, 148, 96),
                        hover_t,
                    ),
                    Color32::from_rgb(15, 23, 42),
                )
            }
        }
    };

    let cr = match style {
        ButtonStyle3d::HeroLaunch | ButtonStyle3d::HeroStop => CornerRadius::same(11),
        _ => CornerRadius::same(8),
    };
    let painter = ui.painter();

    // 3D base lip (extruded depth underneath button face)
    let lip_depth = (3.0 - press_t * 2.0).max(0.8);
    let lip_rect = draw_rect.translate(vec2(0.0, lip_depth));
    painter.rect_filled(lip_rect, cr, lip_col);

    // Main button face
    painter.rect_filled(draw_rect, cr, face_col);
    painter.rect_stroke(draw_rect, cr, Stroke::new(1.2, rim_col), StrokeKind::Inside);

    // Top specular glass highlight line
    painter.line_segment(
        [
            pos2(draw_rect.left() + 8.0, draw_rect.top() + 1.2),
            pos2(draw_rect.right() - 8.0, draw_rect.top() + 1.2),
        ],
        Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, (55.0 + hover_t * 55.0) as u8),
        ),
    );

    // Animated light sweep across HeroLaunch button
    if style == ButtonStyle3d::HeroLaunch {
        let t = ui.input(|i| i.time) as f32;
        ui.ctx().request_repaint_after(Duration::from_millis(20));
        let sweep = ((t * 0.55).fract() * 1.4) - 0.2;
        let sx = draw_rect.left() + sweep * draw_rect.width();
        let x0 = (sx - 16.0).clamp(draw_rect.left() + 3.0, draw_rect.right() - 3.0);
        let x1 = (sx + 16.0).clamp(draw_rect.left() + 3.0, draw_rect.right() - 3.0);
        if x1 > x0 + 2.0 {
            let poly = vec![
                pos2(x0, draw_rect.top() + 1.0),
                pos2(x1, draw_rect.top() + 1.0),
                pos2((x1 - 10.0).max(draw_rect.left() + 3.0), draw_rect.bottom() - 1.0),
                pos2((x0 - 10.0).max(draw_rect.left() + 3.0), draw_rect.bottom() - 1.0),
            ];
            painter.add(Shape::convex_polygon(
                poly,
                Color32::from_rgba_unmultiplied(255, 255, 255, 34),
                Stroke::NONE,
            ));
        }
    }

    if resp.has_focus() {
        painter.rect_stroke(
            draw_rect.expand(2.0),
            cr,
            Stroke::new(2.0, ACCENT_CYAN),
            StrokeKind::Outside,
        );
    }

    let content_w = text_w + icon_space;
    let start_x = draw_rect.center().x - content_w * 0.5;
    let text_x = if let Some(ic) = icon {
        let ic_center_x = if has_text {
            start_x + 7.5
        } else {
            draw_rect.center().x
        };
        let ic_rect = Rect::from_center_size(
            pos2(ic_center_x, draw_rect.center().y),
            Vec2::splat(15.0),
        );
        paint_vector_icon(painter, ic_rect, ic, text_col);
        start_x + icon_space
    } else {
        start_x
    };

    if has_text {
        painter.text(
            pos2(text_x, draw_rect.center().y),
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
            Color32::from_rgba_unmultiplied(12, 16, 29, 225)
        } else {
            Color32::from_rgb(228, 235, 246)
        })
        .stroke(Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(6, 5))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                let mut active_rect: Option<Rect> = None;
                let mut tab_visuals = Vec::with_capacity(tabs.len());

                for (val, icon, title) in tabs {
                    let is_sel = *val == current;
                    let font = FontId::proportional(12.6);
                    let g = ui
                        .painter()
                        .layout_no_wrap(title.clone(), font.clone(), Color32::WHITE);
                    let sz = vec2(g.size().x + 34.0, 29.0);
                    let (rect, resp) = ui.allocate_exact_size(sz, Sense::click());
                    if is_sel {
                        active_rect = Some(rect);
                    }
                    if resp.clicked() && !is_sel {
                        clicked_tab = Some(*val);
                    }
                    tab_visuals.push((rect, resp.hovered(), is_sel, *icon, title.clone(), font));
                }

                if let Some(target) = active_rect {
                    let ax = ui.ctx().animate_value_with_time(
                        bar_id.with("pill_x"),
                        target.min.x,
                        0.17,
                    );
                    let ay = ui.ctx().animate_value_with_time(
                        bar_id.with("pill_y"),
                        target.min.y,
                        0.17,
                    );
                    let aw = ui.ctx().animate_value_with_time(
                        bar_id.with("pill_w"),
                        target.width(),
                        0.17,
                    );
                    let pill = Rect::from_min_size(pos2(ax, ay), vec2(aw, target.height()));
                    ui.painter().rect_filled(
                        pill,
                        CornerRadius::same(8),
                        if dark {
                            ACCENT_EMERALD_DIM
                        } else {
                            Color32::from_rgb(16, 185, 129)
                        },
                    );
                    ui.painter().rect_stroke(
                        pill,
                        CornerRadius::same(8),
                        Stroke::new(1.2, ACCENT_EMERALD),
                        StrokeKind::Inside,
                    );
                }

                for (rect, hovered, is_sel, icon, title, font) in tab_visuals {
                    if hovered && !is_sel {
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same(8),
                            Color32::from_rgba_unmultiplied(148, 163, 184, 26),
                        );
                    }
                    let fg = if is_sel {
                        Color32::WHITE
                    } else if hovered {
                        ui.visuals().strong_text_color()
                    } else {
                        ui.visuals().text_color()
                    };
                    let ic_rect = Rect::from_center_size(
                        pos2(rect.left() + 14.0, rect.center().y),
                        Vec2::splat(13.0),
                    );
                    paint_vector_icon(ui.painter(), ic_rect, icon, fg);
                    ui.painter().text(
                        pos2(rect.left() + 25.0, rect.center().y),
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

    let painter = ui.painter_at(rect);
    let max_val = bars
        .iter()
        .map(|(_, v, _)| *v)
        .max()
        .unwrap_or(1)
        .max(60) as f32;

    let pointer = ui.input(|i| i.pointer.hover_pos());
    let n = bars.len().max(1) as f32;
    let slot_w = (rect.width() - 16.0) / n;
    let bar_w = (slot_w * 0.48).clamp(10.0, 34.0);
    let iso_dx = (bar_w * 0.34).clamp(4.0, 10.0);
    let iso_dy = (bar_w * 0.22).clamp(3.0, 7.0);
    let floor_y = rect.bottom() - 24.0;
    let max_bar_h = (rect.height() - 46.0).max(32.0);

    // Subtle horizontal guide lines
    for step in 0..=3 {
        let gy = floor_y - max_bar_h * (step as f32 / 3.0);
        painter.line_segment(
            [pos2(rect.left() + 4.0, gy), pos2(rect.right() - 4.0, gy)],
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(148, 163, 184, 24),
            ),
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
            shade_color(accent, 1.18)
        } else {
            accent
        };
        let side_col = shade_color(front_col, 0.62);
        let top_col = shade_color(front_col, 1.32);

        // Front face
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x0, y_top),
                pos2(x1, y_top),
                pos2(x1, floor_y),
                pos2(x0, floor_y),
            ],
            front_col,
            Stroke::NONE,
        ));
        // Extruded 3D right side face
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x1, y_top),
                pos2(x1 + iso_dx, y_top - iso_dy),
                pos2(x1 + iso_dx, floor_y - iso_dy),
                pos2(x1, floor_y),
            ],
            side_col,
            Stroke::NONE,
        ));
        // Glowing 3D top cap
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x0 + iso_dx, y_top - iso_dy),
                pos2(x1 + iso_dx, y_top - iso_dy),
                pos2(x1, y_top),
                pos2(x0, y_top),
            ],
            top_col,
            Stroke::NONE,
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
            Color32::from_rgba_unmultiplied(10, 14, 26, 238),
        );
        painter.rect_stroke(
            box_r,
            CornerRadius::same(6),
            Stroke::new(1.0, accent),
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

    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        CornerRadius::same(10),
        Color32::from_rgba_unmultiplied(10, 14, 26, 210),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(
            1.0,
            if active {
                ACCENT_EMERALD
            } else if connecting {
                ACCENT_AMBER
            } else {
                Color32::from_rgba_unmultiplied(100, 116, 139, 90)
            },
        ),
        StrokeKind::Inside,
    );

    let left_node = pos2(rect.left() + 86.0, rect.center().y);
    let right_node = pos2(rect.right() - 106.0, rect.center().y);
    let line_col = if active {
        Color32::from_rgba_unmultiplied(16, 219, 134, 95)
    } else if connecting {
        Color32::from_rgba_unmultiplied(245, 165, 36, 95)
    } else {
        Color32::from_rgba_unmultiplied(100, 116, 139, 55)
    };

    painter.line_segment(
        [pos2(left_node.x + 48.0, left_node.y), pos2(right_node.x - 58.0, right_node.y)],
        Stroke::new(2.2, line_col),
    );

    if active || connecting {
        let x_start = left_node.x + 50.0;
        let x_end = right_node.x - 60.0;
        if x_end > x_start + 12.0 {
            for pkt in 0..4 {
                let u = (t * 0.65 + pkt as f32 * 0.25).fract();
                let px = x_start + (x_end - x_start) * u;
                let col = if active { ACCENT_CYAN } else { ACCENT_AMBER };
                painter.circle_filled(pos2(px, left_node.y), 4.2, col);
                painter.circle_filled(
                    pos2(px, left_node.y),
                    8.0,
                    Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 42),
                );
            }
        }
    }

    for (nc, lbl, col) in [
        (left_node, local_label, ACCENT_BLUE),
        (
            right_node,
            remote_label,
            if active { ACCENT_EMERALD } else { Color32::GRAY },
        ),
    ] {
        let nr = Rect::from_center_size(nc, vec2(138.0, 36.0));
        painter.rect_filled(nr, CornerRadius::same(8), Color32::from_rgb(17, 24, 42));
        painter.rect_stroke(nr, CornerRadius::same(8), Stroke::new(1.2, col), StrokeKind::Inside);
        painter.text(
            nr.center(),
            Align2::CENTER_CENTER,
            lbl,
            FontId::monospace(11.5),
            Color32::WHITE,
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

    let painter = ui.painter();
    let cr = CornerRadius::same((height * 0.5) as u8);
    painter.rect_filled(rect, cr, Color32::from_rgb(12, 17, 30));
    painter.rect_stroke(
        rect,
        cr,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(56, 152, 255, 90)),
        StrokeKind::Inside,
    );

    let fill_w = (rect.width() * fraction.clamp(0.0, 1.0)).max(0.0);
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
                Color32::from_rgba_unmultiplied(190, 255, 232, 110),
            );
        }
    }

    let pct_txt = format!("{:.0}%", fraction.clamp(0.0, 1.0) * 100.0);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        pct_txt,
        FontId::proportional(11.0),
        Color32::WHITE,
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
