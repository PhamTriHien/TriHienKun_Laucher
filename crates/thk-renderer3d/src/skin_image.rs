use image::{Rgba, RgbaImage};
use std::path::Path;
use thk_core::models::SkinVariant;
use thk_core::{ThkError, ThkResult};

pub fn load_and_normalize_skin_png(bytes: &[u8]) -> ThkResult<RgbaImage> {
    let dyn_img = image::load_from_memory(bytes)
        .map_err(|e| ThkError::InvalidInput(format!("Invalid skin PNG image: {e}")))?;
    let rgba = dyn_img.to_rgba8();
    normalize_skin_rgba(rgba)
}

pub fn load_and_normalize_skin_file(path: &Path) -> ThkResult<RgbaImage> {
    let bytes = std::fs::read(path)?;
    load_and_normalize_skin_png(&bytes)
}

pub fn normalize_skin_rgba(img: RgbaImage) -> ThkResult<RgbaImage> {
    let (w, h) = img.dimensions();
    if w < 64 || w % 64 != 0 {
        return Err(ThkError::InvalidInput(format!(
            "Skin width must be a multiple of 64px (got {w}x{h})"
        )));
    }
    if h == w {
        return Ok(strip_opaque_legacy_hat_black(img));
    }
    if h * 2 == w {
        return Ok(convert_legacy_64x32_to_square(&img));
    }
    Err(ThkError::InvalidInput(format!(
        "Unsupported skin aspect ratio {w}x{h} (expected 1:1 or 2:1)"
    )))
}

pub fn convert_legacy_64x32_to_square(src: &RgbaImage) -> RgbaImage {
    let w = src.width();
    let s = w / 64;
    let mut dst = RgbaImage::from_pixel(w, w, Rgba([0, 0, 0, 0]));

    for y in 0..src.height() {
        for x in 0..w {
            dst.put_pixel(x, y, *src.get_pixel(x, y));
        }
    }

    // Mirror Right Leg (0, 16) -> Left Leg (16, 48)
    copy_mirrored_box(&mut dst, s, (0, 16), (16, 48), (4, 12, 4));
    // Mirror Right Arm (40, 16) -> Left Arm (32, 48)
    copy_mirrored_box(&mut dst, s, (40, 16), (32, 48), (4, 12, 4));

    strip_opaque_legacy_hat_black(dst)
}

fn copy_mirrored_box(
    img: &mut RgbaImage,
    scale: u32,
    src_uv: (u32, u32),
    dst_uv: (u32, u32),
    dims: (u32, u32, u32),
) {
    let (su, sv) = (src_uv.0 * scale, src_uv.1 * scale);
    let (du, dv) = (dst_uv.0 * scale, dst_uv.1 * scale);
    let (dx, dy, dz) = (dims.0 * scale, dims.1 * scale, dims.2 * scale);

    // Top: (su + dz, sv, dx, dz) -> mirrored X at (du + dz, dv)
    blit_flipped_x(img, su + dz, sv, du + dz, dv, dx, dz);
    // Bottom: (su + dz + dx, sv, dx, dz) -> mirrored X at (du + dz + dx, dv)
    blit_flipped_x(img, su + dz + dx, sv, du + dz + dx, dv, dx, dz);
    // Front: (su + dz, sv + dz, dx, dy) -> mirrored X at (du + dz, dv + dz)
    blit_flipped_x(img, su + dz, sv + dz, du + dz, dv + dz, dx, dy);
    // Back: (su + 2*dz + dx, sv + dz, dx, dy) -> mirrored X at (du + 2*dz + dx, dv + dz)
    blit_flipped_x(
        img,
        su + 2 * dz + dx,
        sv + dz,
        du + 2 * dz + dx,
        dv + dz,
        dx,
        dy,
    );
    // Outer/Right face swaps with Inner/Left face when mirroring limb
    blit_flipped_x(img, su, sv + dz, du + dz + dx, dv + dz, dz, dy);
    blit_flipped_x(img, su + dz + dx, sv + dz, du, dv + dz, dz, dy);
}

fn blit_flipped_x(img: &mut RgbaImage, sx: u32, sy: u32, dx: u32, dy: u32, w: u32, h: u32) {
    let mut patch = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            patch.push(*img.get_pixel(sx + x, sy + y));
        }
    }
    for y in 0..h {
        for x in 0..w {
            let px = patch[(y * w + (w - 1 - x)) as usize];
            img.put_pixel(dx + x, dy + y, px);
        }
    }
}

fn strip_opaque_legacy_hat_black(mut img: RgbaImage) -> RgbaImage {
    let s = (img.width() / 64).max(1);
    let x0 = 32 * s;
    let x1 = 64 * s;
    let y0 = 0;
    let y1 = 16 * s;

    let mut all_opaque = true;
    'check: for y in y0..y1 {
        for x in x0..x1 {
            if img.get_pixel(x, y)[3] < 255 {
                all_opaque = false;
                break 'check;
            }
        }
    }

    if all_opaque {
        let corner = *img.get_pixel(x0, y0);
        let uniform_bg = (y0..y1).all(|y| (x0..x1).all(|x| *img.get_pixel(x, y) == corner));
        if uniform_bg {
            for y in y0..y1 {
                for x in x0..x1 {
                    img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                }
            }
        }
    }
    img
}

pub fn load_and_normalize_cape_png(bytes: &[u8]) -> ThkResult<RgbaImage> {
    let dyn_img = image::load_from_memory(bytes)
        .map_err(|e| ThkError::InvalidInput(format!("Invalid cape/elytra PNG: {e}")))?;
    Ok(normalize_cape_rgba(dyn_img.to_rgba8()))
}

pub fn load_and_normalize_cape_file(path: &Path) -> ThkResult<RgbaImage> {
    let bytes = std::fs::read(path)?;
    load_and_normalize_cape_png(&bytes)
}

pub fn normalize_cape_rgba(src: RgbaImage) -> RgbaImage {
    let (w, mut h) = src.dimensions();
    if w == 0 || h == 0 {
        return RgbaImage::from_pixel(64, 32, Rgba([0, 0, 0, 0]));
    }

    // Crop first frame when given a vertical multi-frame animated cape strip
    if h > w {
        if w >= 64 && w % 64 == 0 {
            let frame_h = w / 2;
            if frame_h > 0 && h % frame_h == 0 {
                h = frame_h;
            }
        } else if w >= 22 && w % 22 == 0 {
            let frame_h = (w / 22) * 17;
            if frame_h > 0 && h % frame_h == 0 {
                h = frame_h;
            }
        }
    }

    let mut canvas = if w >= 64 && w % 64 == 0 && h * 2 == w && h == src.height() {
        src
    } else {
        let mut target_w = 64u32;
        while target_w < w || (target_w / 2) < h {
            target_w *= 2;
        }
        let target_h = target_w / 2;
        let mut out = RgbaImage::from_pixel(target_w, target_h, Rgba([0, 0, 0, 0]));
        for y in 0..h.min(target_h) {
            for x in 0..w.min(target_w) {
                out.put_pixel(x, y, *src.get_pixel(x, y));
            }
        }
        out
    };

    populate_missing_elytra_from_cape(&mut canvas);
    canvas
}

fn populate_missing_elytra_from_cape(canvas: &mut RgbaImage) {
    let s = (canvas.width() / 64).max(1);
    let cape_has_pixels =
        (0..(17 * s)).any(|y| (0..(22 * s)).any(|x| canvas.get_pixel(x, y)[3] > 0));
    if !cape_has_pixels {
        return;
    }

    let elytra_has_pixels =
        (0..(22 * s)).any(|y| ((22 * s)..(46 * s)).any(|x| canvas.get_pixel(x, y)[3] > 0));
    if elytra_has_pixels {
        return;
    }

    // Synthesize Elytra box UV (22..46, 0..22, dims 10x20x2) from Cape box UV (0..22, 0..17, dims 10x16x1)
    let edge_px = *canvas.get_pixel(s, s);
    fill_rect(canvas, 22 * s, 0, 24 * s, 22 * s, edge_px);

    let wing_w = 10 * s;
    let wing_h = 20 * s;
    let cape_h = 16 * s;
    for wy in 0..wing_h {
        let cy = (wy * cape_h) / wing_h;
        for wx in 0..wing_w {
            let outer_px = *canvas.get_pixel(s + wx, s + cy);
            canvas.put_pixel(24 * s + wx, 2 * s + wy, outer_px);

            let inner_src = *canvas.get_pixel(12 * s + wx, s + cy);
            let inner_px = if inner_src[3] > 0 {
                inner_src
            } else {
                outer_px
            };
            canvas.put_pixel(36 * s + wx, 2 * s + wy, inner_px);
        }
    }
}

pub fn detect_skin_variant(img: &RgbaImage) -> Option<SkinVariant> {
    let (w, h) = img.dimensions();
    if w < 64 || w % 64 != 0 || h != w {
        return None;
    }
    let s = w / 64;

    if rect_has_alpha(img, 47 * s, 16 * s, 3 * s, 4 * s)
        && rect_all_transparent(img, 50 * s, 16 * s, 2 * s, 4 * s)
    {
        return Some(SkinVariant::Slim);
    }

    if rect_has_alpha(img, 52 * s, 20 * s, 2 * s, 12 * s)
        && rect_all_transparent(img, 54 * s, 20 * s, 2 * s, 12 * s)
    {
        return Some(SkinVariant::Slim);
    }

    if (rect_has_alpha(img, 52 * s, 36 * s, 2 * s, 12 * s)
        || rect_has_alpha(img, 60 * s, 52 * s, 2 * s, 12 * s))
        && rect_all_transparent(img, 54 * s, 36 * s, 2 * s, 12 * s)
        && rect_all_transparent(img, 62 * s, 52 * s, 2 * s, 12 * s)
    {
        return Some(SkinVariant::Slim);
    }

    if rect_has_alpha(img, 50 * s, 16 * s, 2 * s, 4 * s)
        || rect_has_alpha(img, 54 * s, 20 * s, 2 * s, 12 * s)
    {
        return Some(SkinVariant::Wide);
    }

    None
}

fn rect_has_alpha(img: &RgbaImage, x0: u32, y0: u32, w: u32, h: u32) -> bool {
    let x1 = (x0 + w).min(img.width());
    let y1 = (y0 + h).min(img.height());
    (y0..y1).any(|y| (x0..x1).any(|x| img.get_pixel(x, y)[3] > 0))
}

fn rect_all_transparent(img: &RgbaImage, x0: u32, y0: u32, w: u32, h: u32) -> bool {
    let x1 = (x0 + w).min(img.width());
    let y1 = (y0 + h).min(img.height());
    (y0..y1).all(|y| (x0..x1).all(|x| img.get_pixel(x, y)[3] == 0))
}

pub fn extract_vanilla_steve_skin(
    versions_dir: &Path,
    cache_dir: Option<&Path>,
    variant: SkinVariant,
) -> Option<RgbaImage> {
    let cache_name = match variant {
        SkinVariant::Wide => "vanilla_steve_wide.png",
        SkinVariant::Slim => "vanilla_steve_slim.png",
    };

    if let Some(dir) = cache_dir {
        let cached = dir.join(cache_name);
        if cached.is_file() {
            if let Ok(img) = load_and_normalize_skin_file(&cached) {
                return Some(img);
            }
        }
    }

    let entries: &[&str] = match variant {
        SkinVariant::Wide => &[
            "assets/minecraft/textures/entity/player/wide/steve.png",
            "assets/minecraft/textures/entity/steve.png",
        ],
        SkinVariant::Slim => &["assets/minecraft/textures/entity/player/slim/steve.png"],
    };

    let read_dir = std::fs::read_dir(versions_dir).ok()?;
    let mut dirs: Vec<_> = read_dir
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .collect();
    dirs.sort_by_key(|b| std::cmp::Reverse(b.file_name()));

    for entry in dirs {
        let ver_name = entry.file_name();
        let jar_path = entry
            .path()
            .join(format!("{}.jar", ver_name.to_string_lossy()));
        if !jar_path.is_file() {
            continue;
        }
        for &zip_entry in entries {
            if let Ok(Some(bytes)) = thk_core::archive::read_zip_entry_bytes(&jar_path, zip_entry) {
                if let Ok(img) = load_and_normalize_skin_png(&bytes) {
                    if let Some(dir) = cache_dir {
                        if std::fs::create_dir_all(dir).is_ok() {
                            let _ = std::fs::write(dir.join(cache_name), &bytes);
                        }
                    }
                    return Some(img);
                }
            }
        }
    }

    None
}

pub fn generate_default_steve_skin() -> RgbaImage {
    generate_default_steve_skin_for_variant(SkinVariant::Wide)
}

pub fn generate_default_steve_skin_for_variant(variant: SkinVariant) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));

    let skin = Rgba([186, 134, 99, 255]);
    let skin_light = Rgba([198, 146, 109, 255]);
    let skin_shade = Rgba([164, 115, 84, 255]);
    let hair = Rgba([47, 32, 21, 255]);
    let hair_dark = Rgba([36, 24, 15, 255]);
    let shirt = Rgba([0, 170, 170, 255]);
    let shirt_shade = Rgba([0, 142, 146, 255]);
    let pants = Rgba([58, 64, 148, 255]);
    let pants_shade = Rgba([46, 51, 124, 255]);
    let shoes = Rgba([102, 102, 106, 255]);
    let shoes_sole = Rgba([72, 72, 76, 255]);

    // Head (0, 0, 8x8x8)
    fill_rect(&mut img, 8, 0, 8, 8, hair);
    fill_rect(&mut img, 10, 2, 4, 4, hair_dark);
    fill_rect(&mut img, 16, 0, 8, 8, skin_shade);
    fill_rect(&mut img, 0, 8, 32, 8, skin);
    fill_rect(&mut img, 9, 10, 6, 3, skin_light);

    // Hairline, sideburns, and back of head
    fill_rect(&mut img, 8, 8, 8, 2, hair);
    img.put_pixel(8, 10, hair);
    img.put_pixel(15, 10, hair);
    fill_rect(&mut img, 0, 8, 8, 3, hair);
    fill_rect(&mut img, 0, 11, 4, 3, hair_dark);
    fill_rect(&mut img, 6, 11, 2, 2, hair);
    fill_rect(&mut img, 16, 8, 8, 3, hair);
    fill_rect(&mut img, 16, 11, 2, 2, hair);
    fill_rect(&mut img, 20, 11, 4, 3, hair_dark);
    fill_rect(&mut img, 24, 8, 8, 7, hair_dark);

    // Eyes, nose, and classic goatee
    img.put_pixel(9, 12, Rgba([255, 255, 255, 255]));
    img.put_pixel(10, 12, Rgba([73, 59, 127, 255]));
    img.put_pixel(13, 12, Rgba([73, 59, 127, 255]));
    img.put_pixel(14, 12, Rgba([255, 255, 255, 255]));
    fill_rect(&mut img, 11, 13, 2, 1, Rgba([156, 102, 76, 255]));
    fill_rect(&mut img, 10, 14, 4, 2, Rgba([98, 61, 43, 255]));
    fill_rect(&mut img, 11, 14, 2, 1, Rgba([136, 84, 62, 255]));

    // Torso (16, 16, 8x12x4)
    fill_rect(&mut img, 20, 16, 8, 4, shirt);
    fill_rect(&mut img, 22, 17, 4, 2, skin_shade);
    fill_rect(&mut img, 28, 16, 8, 4, shirt_shade);
    fill_rect(&mut img, 16, 20, 24, 12, shirt);
    fill_rect(&mut img, 16, 30, 24, 2, shirt_shade);
    fill_rect(&mut img, 22, 20, 4, 1, skin);
    fill_rect(&mut img, 23, 21, 2, 2, skin_shade);

    // Right Arm (40, 16) & Left Arm (32, 48) with Wide (4px) or Slim (3px) UV width
    let arm_dx = match variant {
        SkinVariant::Wide => 4,
        SkinVariant::Slim => 3,
    };
    for (u0, v0) in [(40, 16), (32, 48)] {
        let strip_w = 8 + 2 * arm_dx;
        fill_rect(&mut img, u0 + 4, v0, arm_dx, 4, shirt);
        fill_rect(&mut img, u0 + 4 + arm_dx, v0, arm_dx, 4, skin_shade);
        fill_rect(&mut img, u0, v0 + 4, strip_w, 4, shirt);
        fill_rect(&mut img, u0, v0 + 7, strip_w, 1, shirt_shade);
        fill_rect(&mut img, u0, v0 + 8, strip_w, 8, skin);
        fill_rect(&mut img, u0, v0 + 15, strip_w, 1, skin_shade);
    }

    // Right Leg (0, 16, 4x12x4) & Left Leg (16, 48, 4x12x4)
    for (u0, v0) in [(0, 16), (16, 48)] {
        fill_rect(&mut img, u0 + 4, v0, 4, 4, pants_shade);
        fill_rect(&mut img, u0 + 8, v0, 4, 4, shoes_sole);
        fill_rect(&mut img, u0, v0 + 4, 16, 10, pants);
        fill_rect(&mut img, u0, v0 + 13, 16, 1, pants_shade);
        fill_rect(&mut img, u0, v0 + 14, 16, 1, shoes);
        fill_rect(&mut img, u0, v0 + 15, 16, 1, shoes_sole);
    }

    img
}

pub fn generate_default_brand_cape() -> RgbaImage {
    let mut img = RgbaImage::from_pixel(64, 32, Rgba([0, 0, 0, 0]));
    let dark_emerald = Rgba([14, 48, 38, 255]);
    let bright_emerald = Rgba([34, 197, 94, 255]);
    let gold = Rgba([245, 190, 65, 255]);

    // Cape cuboid (0, 0) dims (10, 16, 1)
    fill_rect(&mut img, 0, 0, 22, 17, dark_emerald);
    // Border & "T" emblem on outer cape face (1..11, 1..17)
    fill_rect(&mut img, 1, 1, 10, 1, gold);
    fill_rect(&mut img, 1, 16, 10, 1, gold);
    fill_rect(&mut img, 3, 4, 6, 2, bright_emerald);
    fill_rect(&mut img, 5, 6, 2, 7, bright_emerald);

    // Elytra wing UV (22, 0) dims (10, 20, 2)
    fill_rect(&mut img, 22, 0, 24, 22, Rgba([22, 64, 52, 255]));
    fill_rect(&mut img, 24, 3, 8, 16, bright_emerald);
    fill_rect(&mut img, 26, 5, 4, 12, dark_emerald);

    img
}

fn fill_rect(img: &mut RgbaImage, x0: u32, y0: u32, w: u32, h: u32, color: Rgba<u8>) {
    for y in y0..(y0 + h).min(img.height()) {
        for x in x0..(x0 + w).min(img.width()) {
            img.put_pixel(x, y, color);
        }
    }
}
