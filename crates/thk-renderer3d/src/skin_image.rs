use image::{Rgba, RgbaImage};
use std::path::Path;
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

fn blit_flipped_x(
    img: &mut RgbaImage,
    sx: u32,
    sy: u32,
    dx: u32,
    dy: u32,
    w: u32,
    h: u32,
) {
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
        let uniform_bg = (y0..y1)
            .all(|y| (x0..x1).all(|x| *img.get_pixel(x, y) == corner));
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
    let (w, h) = src.dimensions();
    if w % 64 == 0 && h * 2 == w {
        return src;
    }
    let mut target_w = 64u32;
    while target_w < w || (target_w / 2) < h {
        target_w *= 2;
    }
    let target_h = target_w / 2;
    let mut canvas = RgbaImage::from_pixel(target_w, target_h, Rgba([0, 0, 0, 0]));
    for y in 0..h.min(target_h) {
        for x in 0..w.min(target_w) {
            canvas.put_pixel(x, y, *src.get_pixel(x, y));
        }
    }
    canvas
}

pub fn generate_default_steve_skin() -> RgbaImage {
    let mut img = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));

    let skin = Rgba([198, 146, 108, 255]);
    let hair = Rgba([58, 38, 24, 255]);
    let shirt = Rgba([16, 152, 166, 255]);
    let pants = Rgba([52, 62, 138, 255]);
    let shoes = Rgba([78, 78, 82, 255]);

    // Head (0, 0, 8x8x8)
    fill_rect(&mut img, 0, 0, 32, 16, skin);
    fill_rect(&mut img, 8, 0, 8, 8, hair);
    fill_rect(&mut img, 8, 8, 8, 2, hair);
    fill_rect(&mut img, 0, 8, 8, 3, hair);
    fill_rect(&mut img, 16, 8, 16, 4, hair);
    // Eyes & mouth on front face (8..16, 8..16)
    img.put_pixel(9, 12, Rgba([255, 255, 255, 255]));
    img.put_pixel(10, 12, Rgba([62, 74, 158, 255]));
    img.put_pixel(13, 12, Rgba([62, 74, 158, 255]));
    img.put_pixel(14, 12, Rgba([255, 255, 255, 255]));
    fill_rect(&mut img, 11, 14, 2, 1, Rgba([130, 80, 58, 255]));

    // Torso (16, 16, 8x12x4)
    fill_rect(&mut img, 16, 16, 24, 16, shirt);

    // Right Arm (40, 16, 4x12x4) & Left Arm (32, 48, 4x12x4)
    fill_rect(&mut img, 40, 16, 16, 8, shirt);
    fill_rect(&mut img, 40, 24, 16, 8, skin);
    fill_rect(&mut img, 32, 48, 16, 8, shirt);
    fill_rect(&mut img, 32, 56, 16, 8, skin);

    // Right Leg (0, 16, 4x12x4) & Left Leg (16, 48, 4x12x4)
    fill_rect(&mut img, 0, 16, 16, 14, pants);
    fill_rect(&mut img, 0, 30, 16, 2, shoes);
    fill_rect(&mut img, 16, 48, 16, 14, pants);
    fill_rect(&mut img, 16, 62, 16, 2, shoes);

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
