use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};
use thk_core::models::SkinVariant;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SkinVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct StageVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Default)]
pub struct StageMeshBatch {
    pub vertices: Vec<StageVertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackEquipmentMode {
    None,
    Cape,
    Elytra,
}

#[derive(Debug, Clone, Default)]
pub struct CharacterMeshBatch {
    pub skin_vertices: Vec<SkinVertex>,
    pub skin_indices: Vec<u32>,
    pub back_vertices: Vec<SkinVertex>,
    pub back_indices: Vec<u32>,
}

pub fn build_character_meshes(
    variant: SkinVariant,
    show_outer_layer: bool,
    back_mode: BackEquipmentMode,
    anim_phase_rad: f32,
) -> CharacterMeshBatch {
    let mut batch = CharacterMeshBatch::default();

    let arm_w = match variant {
        SkinVariant::Wide => 4.0f32,
        SkinVariant::Slim => 3.0f32,
    };
    let arm_u_dx = match variant {
        SkinVariant::Wide => 4.0f32,
        SkinVariant::Slim => 3.0f32,
    };

    let swing = anim_phase_rad.sin() * 0.32;
    let head_nod = (anim_phase_rad * 0.5).sin() * 0.05;
    let breath = (anim_phase_rad * 0.5).cos() * 0.03;

    // 1. Head & Hat (pivot at neck y = 24)
    let head_xf = Mat4::from_translation(Vec3::new(0.0, 24.0, 0.0))
        * Mat4::from_rotation_y(head_nod * 1.6)
        * Mat4::from_rotation_x(head_nod);
    append_mc_cuboid(
        &mut batch.skin_vertices,
        &mut batch.skin_indices,
        head_xf,
        Vec3::new(0.0, 4.0, 0.0),
        Vec3::new(8.0, 8.0, 8.0),
        0.0,
        (0.0, 0.0),
        (8.0, 8.0, 8.0),
        (64.0, 64.0),
        false,
    );

    // 2. Torso
    let body_xf = Mat4::IDENTITY;
    append_mc_cuboid(
        &mut batch.skin_vertices,
        &mut batch.skin_indices,
        body_xf,
        Vec3::new(0.0, 18.0, 0.0),
        Vec3::new(8.0, 12.0, 4.0),
        0.0,
        (16.0, 16.0),
        (8.0, 12.0, 4.0),
        (64.0, 64.0),
        false,
    );

    // 3. Right Arm (character's right = -X)
    let r_arm_x = -(4.0 + arm_w * 0.5);
    let r_arm_xf = Mat4::from_translation(Vec3::new(r_arm_x, 22.0, 0.0))
        * Mat4::from_rotation_x(swing)
        * Mat4::from_rotation_z(-0.04 - breath);
    append_mc_cuboid(
        &mut batch.skin_vertices,
        &mut batch.skin_indices,
        r_arm_xf,
        Vec3::new(0.0, -4.0, 0.0),
        Vec3::new(arm_w, 12.0, 4.0),
        0.0,
        (40.0, 16.0),
        (arm_u_dx, 12.0, 4.0),
        (64.0, 64.0),
        false,
    );

    // 4. Left Arm (character's left = +X)
    let l_arm_x = 4.0 + arm_w * 0.5;
    let l_arm_xf = Mat4::from_translation(Vec3::new(l_arm_x, 22.0, 0.0))
        * Mat4::from_rotation_x(-swing)
        * Mat4::from_rotation_z(0.04 + breath);
    append_mc_cuboid(
        &mut batch.skin_vertices,
        &mut batch.skin_indices,
        l_arm_xf,
        Vec3::new(0.0, -4.0, 0.0),
        Vec3::new(arm_w, 12.0, 4.0),
        0.0,
        (32.0, 48.0),
        (arm_u_dx, 12.0, 4.0),
        (64.0, 64.0),
        false,
    );

    // 5. Right Leg (-X) & Left Leg (+X), pivot at hip y = 12
    let r_leg_xf =
        Mat4::from_translation(Vec3::new(-2.0, 12.0, 0.0)) * Mat4::from_rotation_x(-swing * 0.85);
    append_mc_cuboid(
        &mut batch.skin_vertices,
        &mut batch.skin_indices,
        r_leg_xf,
        Vec3::new(0.0, -6.0, 0.0),
        Vec3::new(4.0, 12.0, 4.0),
        0.0,
        (0.0, 16.0),
        (4.0, 12.0, 4.0),
        (64.0, 64.0),
        false,
    );

    let l_leg_xf =
        Mat4::from_translation(Vec3::new(2.0, 12.0, 0.0)) * Mat4::from_rotation_x(swing * 0.85);
    append_mc_cuboid(
        &mut batch.skin_vertices,
        &mut batch.skin_indices,
        l_leg_xf,
        Vec3::new(0.0, -6.0, 0.0),
        Vec3::new(4.0, 12.0, 4.0),
        0.0,
        (16.0, 48.0),
        (4.0, 12.0, 4.0),
        (64.0, 64.0),
        false,
    );

    // Outer 2nd Layer (appended after inner layer for clean painter's / alpha blending)
    if show_outer_layer {
        append_mc_cuboid(
            &mut batch.skin_vertices,
            &mut batch.skin_indices,
            head_xf,
            Vec3::new(0.0, 4.0, 0.0),
            Vec3::new(8.0, 8.0, 8.0),
            0.5,
            (32.0, 0.0),
            (8.0, 8.0, 8.0),
            (64.0, 64.0),
            false,
        );
        append_mc_cuboid(
            &mut batch.skin_vertices,
            &mut batch.skin_indices,
            body_xf,
            Vec3::new(0.0, 18.0, 0.0),
            Vec3::new(8.0, 12.0, 4.0),
            0.25,
            (16.0, 32.0),
            (8.0, 12.0, 4.0),
            (64.0, 64.0),
            false,
        );
        append_mc_cuboid(
            &mut batch.skin_vertices,
            &mut batch.skin_indices,
            r_arm_xf,
            Vec3::new(0.0, -4.0, 0.0),
            Vec3::new(arm_w, 12.0, 4.0),
            0.25,
            (40.0, 32.0),
            (arm_u_dx, 12.0, 4.0),
            (64.0, 64.0),
            false,
        );
        append_mc_cuboid(
            &mut batch.skin_vertices,
            &mut batch.skin_indices,
            l_arm_xf,
            Vec3::new(0.0, -4.0, 0.0),
            Vec3::new(arm_w, 12.0, 4.0),
            0.25,
            (48.0, 48.0),
            (arm_u_dx, 12.0, 4.0),
            (64.0, 64.0),
            false,
        );
        append_mc_cuboid(
            &mut batch.skin_vertices,
            &mut batch.skin_indices,
            r_leg_xf,
            Vec3::new(0.0, -6.0, 0.0),
            Vec3::new(4.0, 12.0, 4.0),
            0.25,
            (0.0, 32.0),
            (4.0, 12.0, 4.0),
            (64.0, 64.0),
            false,
        );
        append_mc_cuboid(
            &mut batch.skin_vertices,
            &mut batch.skin_indices,
            l_leg_xf,
            Vec3::new(0.0, -6.0, 0.0),
            Vec3::new(4.0, 12.0, 4.0),
            0.25,
            (0.0, 48.0),
            (4.0, 12.0, 4.0),
            (64.0, 64.0),
            false,
        );
    }

    match back_mode {
        BackEquipmentMode::None => {}
        BackEquipmentMode::Cape => {
            let cape_wave = 0.19 + (anim_phase_rad * 0.5).sin().abs() * 0.08;
            let cape_xf = Mat4::from_translation(Vec3::new(0.0, 24.0, -2.30))
                * Mat4::from_rotation_x(cape_wave)
                * Mat4::from_rotation_y(std::f32::consts::PI);
            append_mc_cuboid(
                &mut batch.back_vertices,
                &mut batch.back_indices,
                cape_xf,
                Vec3::new(0.0, -8.0, 0.5),
                Vec3::new(10.0, 16.0, 1.0),
                0.0,
                (0.0, 0.0),
                (10.0, 16.0, 1.0),
                (64.0, 32.0),
                false,
            );
        }
        BackEquipmentMode::Elytra => {
            let wing_flare = 0.26 + (anim_phase_rad * 0.5).sin().abs() * 0.05;
            // Left Elytra Wing (+X)
            let left_wing_xf = Mat4::from_translation(Vec3::new(0.5, 24.0, -2.35))
                * Mat4::from_rotation_x(0.22)
                * Mat4::from_rotation_z(wing_flare)
                * Mat4::from_rotation_y(std::f32::consts::PI + 0.12);
            append_mc_cuboid(
                &mut batch.back_vertices,
                &mut batch.back_indices,
                left_wing_xf,
                Vec3::new(-4.5, -10.0, 1.0),
                Vec3::new(10.0, 20.0, 2.0),
                0.0,
                (22.0, 0.0),
                (10.0, 20.0, 2.0),
                (64.0, 32.0),
                false,
            );

            // Right Elytra Wing (-X, mirrored UVs)
            let right_wing_xf = Mat4::from_translation(Vec3::new(-0.5, 24.0, -2.35))
                * Mat4::from_rotation_x(0.22)
                * Mat4::from_rotation_z(-wing_flare)
                * Mat4::from_rotation_y(std::f32::consts::PI - 0.12);
            append_mc_cuboid(
                &mut batch.back_vertices,
                &mut batch.back_indices,
                right_wing_xf,
                Vec3::new(4.5, -10.0, 1.0),
                Vec3::new(10.0, 20.0, 2.0),
                0.0,
                (22.0, 0.0),
                (10.0, 20.0, 2.0),
                (64.0, 32.0),
                true,
            );
        }
    }

    batch
}

#[allow(clippy::too_many_arguments)]
fn append_mc_cuboid(
    vertices: &mut Vec<SkinVertex>,
    indices: &mut Vec<u32>,
    transform: Mat4,
    local_center: Vec3,
    size: Vec3,
    inflate: f32,
    uv_origin: (f32, f32),
    uv_dims: (f32, f32, f32),
    tex_size: (f32, f32),
    mirror_u: bool,
) {
    let half = size * 0.5 + Vec3::splat(inflate);
    let min = local_center - half;
    let max = local_center + half;

    let (u0, v0) = uv_origin;
    let (dx, dy, dz) = uv_dims;
    let (tw, th) = tex_size;

    let norm_uv = |u_min: f32, v_min: f32, u_max: f32, v_max: f32| -> (Vec2, Vec2) {
        let eps_u = 0.015 / tw;
        let eps_v = 0.015 / th;
        let a = Vec2::new(u_min / tw + eps_u, v_min / th + eps_v);
        let b = Vec2::new(u_max / tw - eps_u, v_max / th - eps_v);
        if mirror_u {
            (Vec2::new(b.x, a.y), Vec2::new(a.x, b.y))
        } else {
            (a, b)
        }
    };

    // Front (+Z)
    let (f_uv0, f_uv1) = norm_uv(u0 + dz, v0 + dz, u0 + dz + dx, v0 + dz + dy);
    push_quad(
        vertices,
        indices,
        transform,
        [
            Vec3::new(min.x, max.y, max.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(min.x, min.y, max.z),
        ],
        Vec3::Z,
        f_uv0,
        f_uv1,
    );

    // Back (-Z)
    let (b_uv0, b_uv1) = norm_uv(
        u0 + 2.0 * dz + dx,
        v0 + dz,
        u0 + 2.0 * dz + 2.0 * dx,
        v0 + dz + dy,
    );
    push_quad(
        vertices,
        indices,
        transform,
        [
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(max.x, min.y, min.z),
        ],
        Vec3::NEG_Z,
        b_uv0,
        b_uv1,
    );

    // Character Right (-X)
    let (r_uv0, r_uv1) = norm_uv(u0, v0 + dz, u0 + dz, v0 + dz + dy);
    push_quad(
        vertices,
        indices,
        transform,
        [
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(min.x, max.y, max.z),
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(min.x, min.y, min.z),
        ],
        Vec3::NEG_X,
        r_uv0,
        r_uv1,
    );

    // Character Left (+X)
    let (l_uv0, l_uv1) = norm_uv(u0 + dz + dx, v0 + dz, u0 + 2.0 * dz + dx, v0 + dz + dy);
    push_quad(
        vertices,
        indices,
        transform,
        [
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(max.x, min.y, max.z),
        ],
        Vec3::X,
        l_uv0,
        l_uv1,
    );

    // Top (+Y)
    let (t_uv0, t_uv1) = norm_uv(u0 + dz, v0, u0 + dz + dx, v0 + dz);
    push_quad(
        vertices,
        indices,
        transform,
        [
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(min.x, max.y, max.z),
        ],
        Vec3::Y,
        t_uv0,
        t_uv1,
    );

    // Bottom (-Y)
    let (bt_uv0, bt_uv1) = norm_uv(u0 + dz + dx, v0, u0 + dz + 2.0 * dx, v0 + dz);
    push_quad(
        vertices,
        indices,
        transform,
        [
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(min.x, min.y, min.z),
        ],
        Vec3::NEG_Y,
        Vec2::new(bt_uv0.x, bt_uv1.y),
        Vec2::new(bt_uv1.x, bt_uv0.y),
    );
}

fn push_quad(
    vertices: &mut Vec<SkinVertex>,
    indices: &mut Vec<u32>,
    transform: Mat4,
    corners: [Vec3; 4],
    local_normal: Vec3,
    uv_min: Vec2,
    uv_max: Vec2,
) {
    let base = vertices.len() as u32;
    let world_normal = transform
        .transform_vector3(local_normal)
        .normalize_or_zero();
    let uvs = [
        [uv_min.x, uv_min.y],
        [uv_max.x, uv_min.y],
        [uv_max.x, uv_max.y],
        [uv_min.x, uv_max.y],
    ];

    for (corner, uv) in corners.into_iter().zip(uvs) {
        let p = transform.transform_point3(corner);
        vertices.push(SkinVertex {
            position: p.to_array(),
            normal: world_normal.to_array(),
            uv,
        });
    }

    // Two triangles (CCW front-facing AND double-sided rendered for overlay hats/capes)
    indices.extend_from_slice(&[base, base + 3, base + 2, base, base + 2, base + 1]);
}

pub fn build_stage_pedestal_meshes(_anim_time: f32) -> StageMeshBatch {
    let mut batch = StageMeshBatch::default();

    append_regular_prism(
        &mut batch,
        8,
        10.8,
        -1.6,
        -0.55,
        0.0,
        [0.09, 0.11, 0.16, 1.0],
        [0.06, 0.08, 0.12, 1.0],
    );

    append_regular_prism(
        &mut batch,
        8,
        8.4,
        -0.55,
        0.0,
        std::f32::consts::FRAC_PI_8,
        [0.14, 0.17, 0.23, 1.0],
        [0.10, 0.12, 0.18, 1.0],
    );

    batch
}

#[allow(clippy::too_many_arguments)]
fn append_regular_prism(
    batch: &mut StageMeshBatch,
    sides: usize,
    radius: f32,
    y_bottom: f32,
    y_top: f32,
    yaw_offset: f32,
    top_color: [f32; 4],
    side_color: [f32; 4],
) {
    let n = sides.max(3);
    let top_center_idx = batch.vertices.len() as u32;
    batch.vertices.push(StageVertex {
        position: [0.0, y_top, 0.0],
        normal: [0.0, 1.0, 0.0],
        color: top_color,
    });

    let ring_start = batch.vertices.len() as u32;
    for i in 0..n {
        let a = yaw_offset + (i as f32 / n as f32) * std::f32::consts::TAU;
        batch.vertices.push(StageVertex {
            position: [a.sin() * radius, y_top, a.cos() * radius],
            normal: [0.0, 1.0, 0.0],
            color: top_color,
        });
    }
    for i in 0..n {
        let cur = ring_start + i as u32;
        let next = ring_start + ((i + 1) % n) as u32;
        batch
            .indices
            .extend_from_slice(&[top_center_idx, cur, next]);
    }

    for i in 0..n {
        let a0 = yaw_offset + (i as f32 / n as f32) * std::f32::consts::TAU;
        let a1 = yaw_offset + ((i + 1) as f32 / n as f32) * std::f32::consts::TAU;
        let p0_top = Vec3::new(a0.sin() * radius, y_top, a0.cos() * radius);
        let p1_top = Vec3::new(a1.sin() * radius, y_top, a1.cos() * radius);
        let p1_bot = Vec3::new(a1.sin() * radius, y_bottom, a1.cos() * radius);
        let p0_bot = Vec3::new(a0.sin() * radius, y_bottom, a0.cos() * radius);
        let mid_a = 0.5 * (a0 + a1);
        let normal = Vec3::new(mid_a.sin(), 0.0, mid_a.cos()).normalize_or_zero();
        push_stage_quad(batch, [p0_top, p1_top, p1_bot, p0_bot], normal, side_color);
    }
}

fn push_stage_quad(batch: &mut StageMeshBatch, corners: [Vec3; 4], normal: Vec3, color: [f32; 4]) {
    let base = batch.vertices.len() as u32;
    for p in corners {
        batch.vertices.push(StageVertex {
            position: p.to_array(),
            normal: normal.to_array(),
            color,
        });
    }
    batch
        .indices
        .extend_from_slice(&[base, base + 3, base + 2, base, base + 2, base + 1]);
}
