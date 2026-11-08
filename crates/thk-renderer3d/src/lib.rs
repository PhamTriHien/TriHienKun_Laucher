#![deny(unsafe_code)]

pub mod geometry;
pub mod pipeline;
pub mod skin_image;

pub use geometry::{
    build_character_meshes, build_stage_pedestal_meshes, BackEquipmentMode, CharacterMeshBatch,
    SkinVertex, StageMeshBatch, StageVertex,
};
pub use pipeline::SkinPreviewViewport;
pub use skin_image::{
    convert_legacy_64x32_to_square, detect_skin_variant, extract_vanilla_steve_skin,
    generate_default_brand_cape, generate_default_steve_skin,
    generate_default_steve_skin_for_variant, load_and_normalize_cape_file,
    load_and_normalize_cape_png, load_and_normalize_skin_file, load_and_normalize_skin_png,
    normalize_cape_rgba, normalize_skin_rgba,
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
    fn detect_skin_variant_distinguishes_wide_slim_and_padded_slim_uvs() {
        let wide = generate_default_steve_skin_for_variant(SkinVariant::Wide);
        let slim = generate_default_steve_skin_for_variant(SkinVariant::Slim);
        assert_eq!(detect_skin_variant(&wide), Some(SkinVariant::Wide));
        assert_eq!(detect_skin_variant(&slim), Some(SkinVariant::Slim));

        // 128x128 HD Slim skin where inner right-arm side strip (54..56, 20..32) is padded opaque,
        // while top/bottom UV (50..52, 16..20) and outer sleeve (54..56, 36..48) follow 3px Slim UV.
        let mut hd_slim = RgbaImage::from_pixel(128, 128, Rgba([0, 0, 0, 0]));
        for y in 32..40 {
            for x in 88..100 {
                hd_slim.put_pixel(x, y, Rgba([250, 240, 242, 255]));
            }
        }
        for y in 40..64 {
            for x in 80..112 {
                hd_slim.put_pixel(x, y, Rgba([247, 207, 183, 255]));
            }
        }
        for y in 84..96 {
            for x in 102..108 {
                hd_slim.put_pixel(x, y, Rgba([130, 140, 180, 255]));
            }
        }
        assert_eq!(detect_skin_variant(&hd_slim), Some(SkinVariant::Slim));
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

    #[test]
    fn stage_pedestal_builder_produces_bounded_finite_geometry() {
        for t in [0.0f32, 1.25, 3.7] {
            let stage = build_stage_pedestal_meshes(t);
            assert!(!stage.vertices.is_empty());
            assert!(!stage.indices.is_empty());
            assert!(stage.vertices.len() <= 2048);
            assert!(stage.indices.len() <= 3072);
            for v in &stage.vertices {
                assert!(v.position.iter().all(|c| c.is_finite()));
                assert!(v.normal.iter().all(|c| c.is_finite()));
            }
            for &idx in &stage.indices {
                assert!((idx as usize) < stage.vertices.len());
            }
        }
    }

    #[test]
    fn cape_and_elytra_stay_behind_character_and_wings_are_symmetric() {
        for phase in [0.0f32, 1.1, 2.7] {
            let cape =
                build_character_meshes(SkinVariant::Wide, true, BackEquipmentMode::Cape, phase);
            assert_eq!(cape.back_vertices.len(), 24);
            for v in &cape.back_vertices {
                assert!(
                    v.position[2] <= -2.25,
                    "cape vertex clipped into torso: z={}",
                    v.position[2]
                );
            }

            let elytra =
                build_character_meshes(SkinVariant::Slim, true, BackEquipmentMode::Elytra, phase);
            assert_eq!(elytra.back_vertices.len(), 48);
            for v in &elytra.back_vertices {
                assert!(
                    v.position[2] <= -2.2,
                    "elytra vertex clipped into torso: z={}",
                    v.position[2]
                );
            }
            let left_mean_x: f32 = elytra.back_vertices[..24]
                .iter()
                .map(|v| v.position[0])
                .sum::<f32>()
                / 24.0;
            let right_mean_x: f32 = elytra.back_vertices[24..]
                .iter()
                .map(|v| v.position[0])
                .sum::<f32>()
                / 24.0;
            assert!(
                left_mean_x > 4.0,
                "left wing should sit on +X: {left_mean_x}"
            );
            assert!(
                right_mean_x < -4.0,
                "right wing should sit on -X: {right_mean_x}"
            );
            assert!((left_mean_x + right_mean_x).abs() < 1e-3);
        }
    }

    #[test]
    fn cape_normalizer_crops_animated_strips_and_synthesizes_elytra_uvs() {
        let mut strip = RgbaImage::from_pixel(64, 256, Rgba([0, 0, 0, 0]));
        strip.put_pixel(2, 2, Rgba([240, 80, 40, 255]));
        strip.put_pixel(2, 40, Rgba([10, 20, 250, 255]));
        let norm_strip = normalize_cape_rgba(strip);
        assert_eq!(norm_strip.dimensions(), (64, 32));
        assert_eq!(*norm_strip.get_pixel(2, 2), Rgba([240, 80, 40, 255]));
        assert!(norm_strip.get_pixel(25, 4)[3] > 0);

        let raw_22x17 = RgbaImage::from_pixel(22, 17, Rgba([30, 180, 90, 255]));
        let norm_22 = normalize_cape_rgba(raw_22x17);
        assert_eq!(norm_22.dimensions(), (64, 32));
        assert_eq!(*norm_22.get_pixel(26, 8), Rgba([30, 180, 90, 255]));
    }

    #[test]
    fn default_steve_variants_and_jar_extraction_preserve_authentic_uvs() {
        let wide = generate_default_steve_skin_for_variant(SkinVariant::Wide);
        let slim = generate_default_steve_skin_for_variant(SkinVariant::Slim);
        assert_eq!(wide.dimensions(), (64, 64));
        assert_eq!(slim.dimensions(), (64, 64));
        // Wide right-arm bottom-of-hand at (48..52, 16..20) is skin-colored, not cyan shirt
        let hand_px = wide.get_pixel(49, 17);
        assert!(hand_px[0] > hand_px[1] && hand_px[3] == 255);
        // Slim arm strip ends at x = 40 + 8 + 6 = 54, leaving x = 55 transparent
        assert_eq!(slim.get_pixel(55, 22)[3], 0);
        assert_eq!(wide.get_pixel(55, 22)[3], 255);

        let temp = std::env::temp_dir().join(format!("thk-steve-jar-{}", uuid::Uuid::new_v4()));
        let ver_dir = temp.join("versions").join("1.21.4");
        let cache_dir = temp.join("cache");
        std::fs::create_dir_all(&ver_dir).unwrap();

        let jar_path = ver_dir.join("1.21.4.jar");
        {
            let file = std::fs::File::create(&jar_path).unwrap();
            let mut zw = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            let mut png_bytes = Vec::new();
            let mut custom_jar_steve = wide.clone();
            custom_jar_steve.put_pixel(10, 10, Rgba([212, 155, 114, 255]));
            image::DynamicImage::ImageRgba8(custom_jar_steve)
                .write_to(
                    &mut std::io::Cursor::new(&mut png_bytes),
                    image::ImageFormat::Png,
                )
                .unwrap();
            zw.start_file(
                "assets/minecraft/textures/entity/player/wide/steve.png",
                opts,
            )
            .unwrap();
            std::io::Write::write_all(&mut zw, &png_bytes).unwrap();
            zw.finish().unwrap();
        }

        let extracted =
            extract_vanilla_steve_skin(&temp.join("versions"), Some(&cache_dir), SkinVariant::Wide)
                .expect("extract steve from client jar");
        assert_eq!(*extracted.get_pixel(10, 10), Rgba([212, 155, 114, 255]));
        assert!(cache_dir.join("vanilla_steve_wide.png").is_file());

        let _ = std::fs::remove_dir_all(&temp);
    }
}
