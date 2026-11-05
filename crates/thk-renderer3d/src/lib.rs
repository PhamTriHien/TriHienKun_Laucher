#![deny(unsafe_code)]

pub mod geometry;
pub mod pipeline;
pub mod skin_image;

pub use geometry::{build_character_meshes, BackEquipmentMode, CharacterMeshBatch, SkinVertex};
pub use pipeline::SkinPreviewViewport;
pub use skin_image::{
    convert_legacy_64x32_to_square, generate_default_brand_cape, generate_default_steve_skin,
    load_and_normalize_cape_file, load_and_normalize_cape_png, load_and_normalize_skin_file,
    load_and_normalize_skin_png, normalize_cape_rgba, normalize_skin_rgba,
};

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use thk_core::models::SkinVariant;

    #[test]
    fn legacy_64x32_skin_converts_to_64x64_with_mirrored_limbs() {
        let mut legacy = RgbaImage::from_pixel(64, 32, Rgba([0, 0, 0, 0]));
        legacy.put_pixel(4, 20, Rgba([220, 40, 40, 255]));
        legacy.put_pixel(44, 20, Rgba([40, 220, 80, 255]));

        let converted = normalize_skin_rgba(legacy).expect("valid 64x32 skin");
        assert_eq!(converted.dimensions(), (64, 64));
        assert_eq!(*converted.get_pixel(23, 52), Rgba([220, 40, 40, 255]));
        assert_eq!(*converted.get_pixel(39, 52), Rgba([40, 220, 80, 255]));
    }

    #[test]
    fn character_mesh_builder_produces_valid_wide_slim_cape_and_elytra_batches() {
        let wide_cape =
            build_character_meshes(SkinVariant::Wide, true, BackEquipmentMode::Cape, 0.5);
        assert_eq!(wide_cape.skin_vertices.len(), 12 * 24);
        assert_eq!(wide_cape.skin_indices.len(), 12 * 36);
        assert_eq!(wide_cape.back_vertices.len(), 24);
        assert_eq!(wide_cape.back_indices.len(), 36);

        let slim_elytra =
            build_character_meshes(SkinVariant::Slim, false, BackEquipmentMode::Elytra, 0.0);
        assert_eq!(slim_elytra.skin_vertices.len(), 6 * 24);
        assert_eq!(slim_elytra.back_vertices.len(), 2 * 24);
        assert_eq!(slim_elytra.back_indices.len(), 2 * 36);
    }

    #[test]
    fn wgpu_perspective_and_look_at_map_near_to_zero_and_far_to_one() {
        let proj = pipeline::perspective_wgpu_rh(45.0f32.to_radians(), 1.5, 2.0, 200.0);
        let view = pipeline::look_at_rh(
            glam::Vec3::new(0.0, 16.0, 50.0),
            glam::Vec3::new(0.0, 16.0, 0.0),
            glam::Vec3::Y,
        );
        let near_ndc = proj.project_point3(glam::Vec3::new(0.0, 0.0, -2.0));
        let far_ndc = proj.project_point3(glam::Vec3::new(0.0, 0.0, -200.0));
        assert!((near_ndc.z - 0.0).abs() < 1e-5);
        assert!((far_ndc.z - 1.0).abs() < 1e-5);

        let target_in_view = view.transform_point3(glam::Vec3::new(0.0, 16.0, 0.0));
        assert!((target_in_view.z + 50.0).abs() < 1e-4);
    }
}
