use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::Utc;
use image::imageops::FilterType;
use image::{ImageFormat, Rgba, RgbaImage};
use num_bigint::{BigInt, BigUint, Sign};
use num_integer::Integer;
use num_traits::{One, Zero};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use sha1::{Digest as _, Sha1};
use sha2::Sha256;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use thk_core::models::{Account, SkinVariant};
use thk_core::{AppPaths, DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub const AUTHLIB_INJECTOR_URL: &str =
    "https://github.com/yushijinhun/authlib-injector/releases/download/v1.2.8/authlib-injector-1.2.8.jar";
pub const AUTHLIB_INJECTOR_SHA256: &str =
    "9c7f4343e6c82034958ffb48c14a2cb0c85928be7283103ce17da00c6d5a7b10";

const RSA_KEY_BYTES: usize = 256;
const SHA1_DIGEST_INFO_PREFIX: [u8; 15] = [
    0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04, 0x14,
];
const SMALL_PRIMES: [u32; 30] = [
    3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97,
    101, 103, 107, 109, 113, 127,
];

#[derive(Clone)]
pub struct YggdrasilRsaKeypair {
    n: BigUint,
    e: BigUint,
    d: BigUint,
    public_key_pem: String,
}

static LOCAL_RSA_KEYPAIR: OnceLock<YggdrasilRsaKeypair> = OnceLock::new();

pub fn local_yggdrasil_keypair() -> &'static YggdrasilRsaKeypair {
    LOCAL_RSA_KEYPAIR.get_or_init(YggdrasilRsaKeypair::generate_2048)
}

impl YggdrasilRsaKeypair {
    fn generate_2048() -> Self {
        let e = BigUint::from(65_537_u32);
        loop {
            let p = generate_prime_1024(&e);
            let q = generate_prime_1024(&e);
            if p == q {
                continue;
            }
            let n = &p * &q;
            if n.bits() != 2048 {
                continue;
            }
            let phi = (&p - 1u32) * (&q - 1u32);
            let Some(d) = mod_inverse(&e, &phi) else {
                continue;
            };
            let public_key_pem = encode_spki_public_key_pem(&n, &e);
            let kp = Self {
                n,
                e: e.clone(),
                d,
                public_key_pem,
            };
            let probe = b"thk-yggdrasil-selftest";
            let sig = kp.sign_sha1_pkcs1_v15_base64(probe);
            if kp.verify_sha1_pkcs1_v15_base64(probe, &sig) {
                return kp;
            }
        }
    }

    pub fn public_key_pem(&self) -> &str {
        &self.public_key_pem
    }

    pub fn sign_sha1_pkcs1_v15_base64(&self, message: &[u8]) -> String {
        let em = build_pkcs1_v15_sha1_em(message);
        let m = BigUint::from_bytes_be(&em);
        let s = m.modpow(&self.d, &self.n);
        let raw = s.to_bytes_be();
        let mut padded = [0u8; RSA_KEY_BYTES];
        let offset = RSA_KEY_BYTES.saturating_sub(raw.len());
        padded[offset..].copy_from_slice(&raw[raw.len().saturating_sub(RSA_KEY_BYTES)..]);
        BASE64.encode(padded)
    }

    pub fn verify_sha1_pkcs1_v15_base64(&self, message: &[u8], signature_b64: &str) -> bool {
        let Ok(sig_bytes) = BASE64.decode(signature_b64) else {
            return false;
        };
        if sig_bytes.len() != RSA_KEY_BYTES {
            return false;
        }
        let s = BigUint::from_bytes_be(&sig_bytes);
        if s >= self.n {
            return false;
        }
        let m = s.modpow(&self.e, &self.n);
        let raw = m.to_bytes_be();
        let mut recovered = [0u8; RSA_KEY_BYTES];
        if raw.len() > RSA_KEY_BYTES {
            return false;
        }
        recovered[RSA_KEY_BYTES - raw.len()..].copy_from_slice(&raw);
        recovered == build_pkcs1_v15_sha1_em(message)
    }
}

fn build_pkcs1_v15_sha1_em(message: &[u8]) -> [u8; RSA_KEY_BYTES] {
    let digest = Sha1::digest(message);
    let mut em = [0u8; RSA_KEY_BYTES];
    em[0] = 0x00;
    em[1] = 0x01;
    em[2..220].fill(0xff);
    em[220] = 0x00;
    em[221..236].copy_from_slice(&SHA1_DIGEST_INFO_PREFIX);
    em[236..256].copy_from_slice(&digest);
    em
}

fn generate_prime_1024(e: &BigUint) -> BigUint {
    let mut rng = rand::rng();
    let mut buf = [0u8; 128];
    loop {
        rng.fill_bytes(&mut buf);
        buf[0] |= 0xC0;
        buf[127] |= 0x01;
        let candidate = BigUint::from_bytes_be(&buf);
        if SMALL_PRIMES.iter().any(|&p| (&candidate % p).is_zero()) {
            continue;
        }
        let p_minus_1 = &candidate - 1u32;
        if p_minus_1.gcd(e) != BigUint::one() {
            continue;
        }
        if is_probable_prime_miller_rabin(&candidate, 20) {
            return candidate;
        }
    }
}

fn is_probable_prime_miller_rabin(n: &BigUint, rounds: usize) -> bool {
    let two = BigUint::from(2u32);
    let n_minus_1 = n - 1u32;
    let n_minus_3 = n - 3u32;

    let mut d = n_minus_1.clone();
    let mut r = 0u32;
    while d.is_even() {
        d >>= 1;
        r += 1;
    }

    let mut rng = rand::rng();
    let mut sample = [0u8; 128];
    'witness: for _ in 0..rounds {
        rng.fill_bytes(&mut sample);
        let a = (BigUint::from_bytes_be(&sample) % &n_minus_3) + &two;
        let mut x = a.modpow(&d, n);
        if x == BigUint::one() || x == n_minus_1 {
            continue 'witness;
        }
        for _ in 1..r {
            x = (&x * &x) % n;
            if x == n_minus_1 {
                continue 'witness;
            }
        }
        return false;
    }
    true
}

fn mod_inverse(a: &BigUint, m: &BigUint) -> Option<BigUint> {
    let a_int = BigInt::from_biguint(Sign::Plus, a.clone());
    let m_int = BigInt::from_biguint(Sign::Plus, m.clone());
    let ext = a_int.extended_gcd(&m_int);
    if ext.gcd != BigInt::one() {
        return None;
    }
    let mut x = ext.x % &m_int;
    if x < BigInt::zero() {
        x += &m_int;
    }
    x.to_biguint()
}

fn der_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let len = content.len();
    let mut out = Vec::with_capacity(4 + len);
    out.push(tag);
    if len < 128 {
        out.push(len as u8);
    } else if len <= 0xff {
        out.push(0x81);
        out.push(len as u8);
    } else {
        out.push(0x82);
        out.push(((len >> 8) & 0xff) as u8);
        out.push((len & 0xff) as u8);
    }
    out.extend_from_slice(content);
    out
}

fn der_integer_positive(raw_be: &[u8]) -> Vec<u8> {
    let first_nonzero = raw_be.iter().position(|&b| b != 0).unwrap_or(raw_be.len());
    let trimmed = if first_nonzero == raw_be.len() {
        &[0u8][..]
    } else {
        &raw_be[first_nonzero..]
    };
    let mut body = Vec::with_capacity(trimmed.len() + 1);
    if trimmed[0] & 0x80 != 0 {
        body.push(0x00);
    }
    body.extend_from_slice(trimmed);
    der_tlv(0x02, &body)
}

fn encode_spki_public_key_pem(n: &BigUint, e: &BigUint) -> String {
    let n_der = der_integer_positive(&n.to_bytes_be());
    let e_der = der_integer_positive(&e.to_bytes_be());
    let mut rsa_seq_body = Vec::with_capacity(n_der.len() + e_der.len());
    rsa_seq_body.extend_from_slice(&n_der);
    rsa_seq_body.extend_from_slice(&e_der);
    let rsa_pub_der = der_tlv(0x30, &rsa_seq_body);

    let alg_id: [u8; 15] = [
        0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00,
    ];
    let mut bit_str_body = Vec::with_capacity(1 + rsa_pub_der.len());
    bit_str_body.push(0x00);
    bit_str_body.extend_from_slice(&rsa_pub_der);
    let bit_str_der = der_tlv(0x03, &bit_str_body);

    let mut spki_body = Vec::with_capacity(alg_id.len() + bit_str_der.len());
    spki_body.extend_from_slice(&alg_id);
    spki_body.extend_from_slice(&bit_str_der);
    let spki_der = der_tlv(0x30, &spki_body);

    let b64 = BASE64.encode(&spki_der);
    let mut pem = String::from("-----BEGIN PUBLIC KEY-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        if let Ok(line) = std::str::from_utf8(chunk) {
            pem.push_str(line);
            pem.push('\n');
        }
    }
    pem.push_str("-----END PUBLIC KEY-----\n");
    pem
}

#[derive(Debug, Clone)]
pub struct YggdrasilPlayerEntry {
    pub raw_uuid: String,
    pub username: String,
    pub skin_variant: SkinVariant,
    pub skin_hash: Option<String>,
    pub cape_hash: Option<String>,
    pub elytra_hash: Option<String>,
}

#[derive(Clone)]
struct YggState {
    port: u16,
    players: Arc<RwLock<HashMap<String, YggdrasilPlayerEntry>>>,
    textures: Arc<HashMap<String, Arc<Vec<u8>>>>,
    keypair: YggdrasilRsaKeypair,
}

pub struct LocalYggdrasilHandle {
    pub port: u16,
    pub prefetched_meta_b64: String,
    cancel: CancellationToken,
}

impl LocalYggdrasilHandle {
    pub fn jvm_agent_arg(&self, authlib_jar: &Path) -> String {
        format!(
            "-javaagent:{}=http://127.0.0.1:{}",
            authlib_jar.display(),
            self.port
        )
    }

    pub fn jvm_args(&self, authlib_jar: &Path) -> Vec<String> {
        vec![
            format!(
                "-Dauthlibinjector.yggdrasil.prefetched={}",
                self.prefetched_meta_b64
            ),
            self.jvm_agent_arg(authlib_jar),
        ]
    }

    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

impl Drop for LocalYggdrasilHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub fn normalize_skin_png_bytes_for_minecraft(bytes: &[u8]) -> ThkResult<Vec<u8>> {
    let dyn_img = image::load_from_memory(bytes)
        .map_err(|e| ThkError::InvalidInput(format!("Invalid skin PNG image: {e}")))?;
    let rgba = dyn_img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return Err(ThkError::InvalidInput(
            "Skin image has zero dimensions.".into(),
        ));
    }

    let is_png = matches!(image::guess_format(bytes), Ok(ImageFormat::Png));
    if (w, h) == (64, 64) {
        let (cleaned, modified) = strip_opaque_legacy_hat_black_with_flag(rgba);
        if !modified && is_png {
            return Ok(bytes.to_vec());
        }
        return encode_png_rgba(&cleaned);
    }

    if h * 2 == w && w >= 64 && w % 64 == 0 {
        let square = convert_legacy_2_to_1_skin_to_square(&rgba);
        if w == 64 {
            return encode_png_rgba(&square);
        }
        let downsampled = downsample_square_skin_premultiplied(&square);
        return encode_png_rgba(&downsampled);
    }

    if h == w && w > 64 && w % 64 == 0 {
        let (cleaned, _) = strip_opaque_legacy_hat_black_with_flag(rgba);
        let downsampled = downsample_square_skin_premultiplied(&cleaned);
        return encode_png_rgba(&downsampled);
    }

    if h * 2 == w {
        let resized_half = image::imageops::resize(&rgba, 64, 32, FilterType::Triangle);
        let square = convert_legacy_2_to_1_skin_to_square(&resized_half);
        return encode_png_rgba(&square);
    }

    let resized = image::imageops::resize(&rgba, 64, 64, FilterType::Triangle);
    let (cleaned, _) = strip_opaque_legacy_hat_black_with_flag(resized);
    encode_png_rgba(&cleaned)
}

pub fn normalize_cape_png_bytes_for_minecraft(bytes: &[u8]) -> ThkResult<Vec<u8>> {
    let dyn_img = image::load_from_memory(bytes)
        .map_err(|e| ThkError::InvalidInput(format!("Invalid cape/elytra PNG image: {e}")))?;
    let src = dyn_img.to_rgba8();
    let (w, mut h) = src.dimensions();
    if w == 0 || h == 0 {
        return Err(ThkError::InvalidInput(
            "Cape/Elytra image has zero dimensions.".into(),
        ));
    }

    let is_png = matches!(image::guess_format(bytes), Ok(ImageFormat::Png));
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

    if w >= 64 && w % 64 == 0 && h * 2 == w && h == src.height() {
        let mut canvas = src;
        let synthesized = populate_missing_elytra_from_cape(&mut canvas);
        if !synthesized && is_png {
            return Ok(bytes.to_vec());
        }
        return encode_png_rgba(&canvas);
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
    populate_missing_elytra_from_cape(&mut canvas);
    encode_png_rgba(&canvas)
}

fn encode_png_rgba(img: &RgbaImage) -> ThkResult<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    img.write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| ThkError::InvalidInput(format!("Failed to encode PNG texture: {e}")))?;
    Ok(cursor.into_inner())
}

fn sha256_hex_lower(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn downsample_square_skin_premultiplied(src: &RgbaImage) -> RgbaImage {
    let scale = (src.width() / 64).max(1);
    if scale == 1 && src.height() == 64 {
        return src.clone();
    }
    let count = u64::from(scale) * u64::from(scale);
    let mut dst = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));

    for ty in 0..64 {
        let y0 = ty * scale;
        let y1 = y0 + scale;
        for tx in 0..64 {
            let x0 = tx * scale;
            let x1 = x0 + scale;
            let mut sum_a = 0u64;
            let mut sum_r = 0u64;
            let mut sum_g = 0u64;
            let mut sum_b = 0u64;

            for sy in y0..y1 {
                for sx in x0..x1 {
                    let px = src.get_pixel(sx, sy);
                    let a = u64::from(px[3]);
                    if a > 0 {
                        sum_a += a;
                        sum_r += u64::from(px[0]) * a;
                        sum_g += u64::from(px[1]) * a;
                        sum_b += u64::from(px[2]) * a;
                    }
                }
            }

            if sum_a == 0 {
                continue;
            }
            let out_a = ((sum_a + count / 2) / count).min(255) as u8;
            if out_a == 0 {
                continue;
            }
            let out_r = ((sum_r + sum_a / 2) / sum_a).min(255) as u8;
            let out_g = ((sum_g + sum_a / 2) / sum_a).min(255) as u8;
            let out_b = ((sum_b + sum_a / 2) / sum_a).min(255) as u8;
            dst.put_pixel(tx, ty, Rgba([out_r, out_g, out_b, out_a]));
        }
    }

    dst
}

fn convert_legacy_2_to_1_skin_to_square(src: &RgbaImage) -> RgbaImage {
    let w = src.width();
    let s = (w / 64).max(1);
    let mut dst = RgbaImage::from_pixel(w, w, Rgba([0, 0, 0, 0]));

    for y in 0..src.height() {
        for x in 0..w {
            dst.put_pixel(x, y, *src.get_pixel(x, y));
        }
    }

    copy_mirrored_limb_box(&mut dst, s, (0, 16), (16, 48), (4, 12, 4));
    copy_mirrored_limb_box(&mut dst, s, (40, 16), (32, 48), (4, 12, 4));

    let (cleaned, _) = strip_opaque_legacy_hat_black_with_flag(dst);
    cleaned
}

fn copy_mirrored_limb_box(
    img: &mut RgbaImage,
    scale: u32,
    src_uv: (u32, u32),
    dst_uv: (u32, u32),
    dims: (u32, u32, u32),
) {
    let (su, sv) = (src_uv.0 * scale, src_uv.1 * scale);
    let (du, dv) = (dst_uv.0 * scale, dst_uv.1 * scale);
    let (dx, dy, dz) = (dims.0 * scale, dims.1 * scale, dims.2 * scale);

    blit_flipped_x(img, su + dz, sv, du + dz, dv, dx, dz);
    blit_flipped_x(img, su + dz + dx, sv, du + dz + dx, dv, dx, dz);
    blit_flipped_x(img, su + dz, sv + dz, du + dz, dv + dz, dx, dy);
    blit_flipped_x(
        img,
        su + 2 * dz + dx,
        sv + dz,
        du + 2 * dz + dx,
        dv + dz,
        dx,
        dy,
    );
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

fn strip_opaque_legacy_hat_black_with_flag(mut img: RgbaImage) -> (RgbaImage, bool) {
    let s = (img.width() / 64).max(1);
    let x0 = 32 * s;
    let x1 = (64 * s).min(img.width());
    let y0 = 0;
    let y1 = (16 * s).min(img.height());
    if x0 >= x1 || y0 >= y1 {
        return (img, false);
    }

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
            return (img, true);
        }
    }
    (img, false)
}

fn populate_missing_elytra_from_cape(canvas: &mut RgbaImage) -> bool {
    let s = (canvas.width() / 64).max(1);
    let cape_has_pixels =
        (0..(17 * s)).any(|y| (0..(22 * s)).any(|x| canvas.get_pixel(x, y)[3] > 0));
    if !cape_has_pixels {
        return false;
    }

    let elytra_has_pixels =
        (0..(22 * s)).any(|y| ((22 * s)..(46 * s)).any(|x| canvas.get_pixel(x, y)[3] > 0));
    if elytra_has_pixels {
        return false;
    }

    let edge_px = *canvas.get_pixel(s, s);
    fill_rgba_rect(canvas, 22 * s, 0, 24 * s, 22 * s, edge_px);

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
    true
}

fn resolve_slim_fallback_skin_png_bytes(paths: Option<&AppPaths>) -> ThkResult<Vec<u8>> {
    if let Some(p) = paths {
        let cached = p.cache_dir.join("vanilla_steve_slim.png");
        if cached.is_file() {
            if let Ok(raw) = std::fs::read(&cached) {
                if let Ok(norm) = normalize_skin_png_bytes_for_minecraft(&raw) {
                    return Ok(norm);
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(&p.versions_dir) {
            let mut dirs: Vec<_> = rd
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
                if let Ok(Some(raw)) = thk_core::archive::read_zip_entry_bytes(
                    &jar_path,
                    "assets/minecraft/textures/entity/player/slim/steve.png",
                ) {
                    if let Ok(norm) = normalize_skin_png_bytes_for_minecraft(&raw) {
                        if std::fs::create_dir_all(&p.cache_dir).is_ok() {
                            let _ = std::fs::write(&cached, &norm);
                        }
                        return Ok(norm);
                    }
                }
            }
        }
    }

    encode_png_rgba(&generate_slim_steve_rgba_64())
}

fn generate_slim_steve_rgba_64() -> RgbaImage {
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

    fill_rgba_rect(&mut img, 8, 0, 8, 8, hair);
    fill_rgba_rect(&mut img, 10, 2, 4, 4, hair_dark);
    fill_rgba_rect(&mut img, 16, 0, 8, 8, skin_shade);
    fill_rgba_rect(&mut img, 0, 8, 32, 8, skin);
    fill_rgba_rect(&mut img, 9, 10, 6, 3, skin_light);
    fill_rgba_rect(&mut img, 8, 8, 8, 2, hair);
    img.put_pixel(8, 10, hair);
    img.put_pixel(15, 10, hair);
    fill_rgba_rect(&mut img, 0, 8, 8, 3, hair);
    fill_rgba_rect(&mut img, 0, 11, 4, 3, hair_dark);
    fill_rgba_rect(&mut img, 6, 11, 2, 2, hair);
    fill_rgba_rect(&mut img, 16, 8, 8, 3, hair);
    fill_rgba_rect(&mut img, 16, 11, 2, 2, hair);
    fill_rgba_rect(&mut img, 20, 11, 4, 3, hair_dark);
    fill_rgba_rect(&mut img, 24, 8, 8, 7, hair_dark);

    img.put_pixel(9, 12, Rgba([255, 255, 255, 255]));
    img.put_pixel(10, 12, Rgba([73, 59, 127, 255]));
    img.put_pixel(13, 12, Rgba([73, 59, 127, 255]));
    img.put_pixel(14, 12, Rgba([255, 255, 255, 255]));
    fill_rgba_rect(&mut img, 11, 13, 2, 1, Rgba([156, 102, 76, 255]));
    fill_rgba_rect(&mut img, 10, 14, 4, 2, Rgba([98, 61, 43, 255]));
    fill_rgba_rect(&mut img, 11, 14, 2, 1, Rgba([136, 84, 62, 255]));

    fill_rgba_rect(&mut img, 20, 16, 8, 4, shirt);
    fill_rgba_rect(&mut img, 22, 17, 4, 2, skin_shade);
    fill_rgba_rect(&mut img, 28, 16, 8, 4, shirt_shade);
    fill_rgba_rect(&mut img, 16, 20, 24, 12, shirt);
    fill_rgba_rect(&mut img, 16, 30, 24, 2, shirt_shade);
    fill_rgba_rect(&mut img, 22, 20, 4, 1, skin);
    fill_rgba_rect(&mut img, 23, 21, 2, 2, skin_shade);

    for (u0, v0) in [(40, 16), (32, 48)] {
        fill_rgba_rect(&mut img, u0 + 4, v0, 3, 4, shirt);
        fill_rgba_rect(&mut img, u0 + 7, v0, 3, 4, skin_shade);
        fill_rgba_rect(&mut img, u0, v0 + 4, 14, 4, shirt);
        fill_rgba_rect(&mut img, u0, v0 + 7, 14, 1, shirt_shade);
        fill_rgba_rect(&mut img, u0, v0 + 8, 14, 8, skin);
        fill_rgba_rect(&mut img, u0, v0 + 15, 14, 1, skin_shade);
    }

    for (u0, v0) in [(0, 16), (16, 48)] {
        fill_rgba_rect(&mut img, u0 + 4, v0, 4, 4, pants_shade);
        fill_rgba_rect(&mut img, u0 + 8, v0, 4, 4, shoes_sole);
        fill_rgba_rect(&mut img, u0, v0 + 4, 16, 10, pants);
        fill_rgba_rect(&mut img, u0, v0 + 13, 16, 1, pants_shade);
        fill_rgba_rect(&mut img, u0, v0 + 14, 16, 1, shoes);
        fill_rgba_rect(&mut img, u0, v0 + 15, 16, 1, shoes_sole);
    }

    img
}

fn fill_rgba_rect(img: &mut RgbaImage, x0: u32, y0: u32, w: u32, h: u32, color: Rgba<u8>) {
    for y in y0..(y0 + h).min(img.height()) {
        for x in x0..(x0 + w).min(img.width()) {
            img.put_pixel(x, y, color);
        }
    }
}

pub async fn ensure_authlib_injector(
    downloader: &DownloadManager,
    paths: &AppPaths,
) -> ThkResult<PathBuf> {
    let mut task = DownloadTask::new(AUTHLIB_INJECTOR_URL, paths.authlib_jar.clone());
    task.expected_sha256 = Some(AUTHLIB_INJECTOR_SHA256.to_string());
    task.fallback_urls
        .push("https://authlib-injector.yushi.moe/artifact/56/authlib-injector-1.2.8.jar".into());
    task.fallback_urls.push(
        "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/artifact/56/authlib-injector-1.2.8.jar"
            .into(),
    );
    downloader
        .download_single(&task, None, |_, _, _| {})
        .await?;
    Ok(paths.authlib_jar.clone())
}

struct PreparedLocalYggdrasil {
    keypair: YggdrasilRsaKeypair,
    entry: YggdrasilPlayerEntry,
    textures: HashMap<String, Arc<Vec<u8>>>,
}

pub async fn start_local_yggdrasil(account: &Account) -> ThkResult<LocalYggdrasilHandle> {
    start_local_yggdrasil_with_paths(account, None).await
}

pub async fn start_local_yggdrasil_with_paths(
    account: &Account,
    paths: Option<&AppPaths>,
) -> ThkResult<LocalYggdrasilHandle> {
    let acct_owned = account.clone();
    let paths_owned = paths.cloned();

    let prepared = tokio::task::spawn_blocking(move || -> ThkResult<PreparedLocalYggdrasil> {
        let keypair = local_yggdrasil_keypair().clone();
        let raw_uuid = acct_owned.uuid.replace('-', "").to_ascii_lowercase();
        let mut textures: HashMap<String, Arc<Vec<u8>>> = HashMap::new();

        let skin_hash =
            if let Some(skin_path) = acct_owned.custom_skin_path.as_ref().filter(|p| p.is_file()) {
                let raw = std::fs::read(skin_path)?;
                let png = normalize_skin_png_bytes_for_minecraft(&raw)?;
                let hash = sha256_hex_lower(&png);
                textures.insert(hash.clone(), Arc::new(png));
                Some(hash)
            } else if acct_owned.skin_variant == SkinVariant::Slim {
                let png = resolve_slim_fallback_skin_png_bytes(paths_owned.as_ref())?;
                let hash = sha256_hex_lower(&png);
                textures.insert(hash.clone(), Arc::new(png));
                Some(hash)
            } else {
                None
            };

        let cape_hash =
            if let Some(cape_path) = acct_owned.custom_cape_path.as_ref().filter(|p| p.is_file()) {
                let raw = std::fs::read(cape_path)?;
                let png = normalize_cape_png_bytes_for_minecraft(&raw)?;
                let hash = sha256_hex_lower(&png);
                textures.insert(hash.clone(), Arc::new(png));
                Some(hash)
            } else {
                None
            };

        let elytra_hash = if let Some(elytra_path) = acct_owned
            .custom_elytra_path
            .as_ref()
            .filter(|p| p.is_file())
        {
            let raw = std::fs::read(elytra_path)?;
            let png = normalize_cape_png_bytes_for_minecraft(&raw)?;
            let hash = sha256_hex_lower(&png);
            textures.insert(hash.clone(), Arc::new(png));
            Some(hash)
        } else {
            None
        };

        let entry = YggdrasilPlayerEntry {
            raw_uuid,
            username: acct_owned.username,
            skin_variant: acct_owned.skin_variant,
            skin_hash,
            cape_hash,
            elytra_hash,
        };

        Ok(PreparedLocalYggdrasil {
            keypair,
            entry,
            textures,
        })
    })
    .await
    .map_err(|e| ThkError::Process(format!("Yggdrasil initialization worker failed: {e}")))??;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| ThkError::Process(format!("Failed binding local Yggdrasil server: {e}")))?;
    let addr: SocketAddr = listener.local_addr()?;
    let port = addr.port();

    let prefetched_meta_b64 =
        BASE64.encode(build_root_metadata_json(&prepared.keypair).to_string());

    let mut map = HashMap::new();
    map.insert(prepared.entry.raw_uuid.clone(), prepared.entry);

    let state = YggState {
        port,
        players: Arc::new(RwLock::new(map)),
        textures: Arc::new(prepared.textures),
        keypair: prepared.keypair,
    };

    let app = Router::new()
        .route("/", get(handle_root))
        .route("/minecraft/profile", get(handle_query_profile))
        .route(
            "/minecraftservices/minecraft/profile",
            get(handle_minecraft_services_profile),
        )
        .route(
            "/session/minecraft/profile/{uuid}",
            get(handle_session_profile),
        )
        .route(
            "/sessionserver/session/minecraft/profile/{uuid}",
            get(handle_session_profile),
        )
        .route("/session/minecraft/join", post(handle_no_content))
        .route(
            "/sessionserver/session/minecraft/join",
            post(handle_no_content),
        )
        .route("/session/minecraft/hasJoined", get(handle_has_joined))
        .route(
            "/sessionserver/session/minecraft/hasJoined",
            get(handle_has_joined),
        )
        .route(
            "/api/users/profiles/minecraft/{username}",
            get(handle_lookup_by_name),
        )
        .route(
            "/users/profiles/minecraft/{username}",
            get(handle_lookup_by_name),
        )
        .route(
            "/minecraftservices/minecraft/profile/lookup/name/{username}",
            get(handle_lookup_by_name),
        )
        .route(
            "/minecraftservices/minecraft/profile/lookup/{uuid}",
            get(handle_lookup_by_uuid),
        )
        .route("/api/profiles/minecraft", post(handle_bulk_profiles))
        .route("/profiles/minecraft", post(handle_bulk_profiles))
        .route(
            "/minecraftservices/minecraft/profile/lookup/bulk/byname",
            post(handle_bulk_profiles),
        )
        .route(
            "/minecraftservices/player/attributes",
            get(handle_player_attributes),
        )
        .route("/player/attributes", get(handle_player_attributes))
        .route("/textures/{id}", get(handle_texture))
        .route("/authserver/validate", any(handle_no_content))
        .route("/authserver/invalidate", any(handle_no_content))
        .route("/authserver/signout", any(handle_no_content))
        .route("/authserver/refresh", any(handle_auth_refresh))
        .route("/auth/validate", any(handle_no_content))
        .route("/auth/invalidate", any(handle_no_content))
        .route("/auth/signout", any(handle_no_content))
        .route("/auth/refresh", any(handle_auth_refresh))
        .with_state(state);

    let cancel = CancellationToken::new();
    let shutdown_tok = cancel.clone();

    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown_tok.cancelled().await;
            })
            .await;
    });

    Ok(LocalYggdrasilHandle {
        port,
        prefetched_meta_b64,
        cancel,
    })
}

fn build_root_metadata_json(keypair: &YggdrasilRsaKeypair) -> Value {
    json!({
        "meta": {
            "serverName": "TriHienKun Laucher Local Skin Server",
            "implementationName": "thk-yggdrasil",
            "implementationVersion": "1.0.0",
            "feature.non_email_login": true
        },
        "skinDomains": ["127.0.0.1", "localhost", "::1"],
        "signaturePublickey": keypair.public_key_pem()
    })
}

async fn handle_root(State(state): State<YggState>) -> Json<Value> {
    Json(build_root_metadata_json(&state.keypair))
}

#[derive(Deserialize)]
struct ProfileQuery {
    uuid: Option<String>,
    id: Option<String>,
    unsigned: Option<bool>,
}

async fn handle_query_profile(
    State(state): State<YggState>,
    Query(q): Query<ProfileQuery>,
) -> Response {
    let target = q
        .uuid
        .or(q.id)
        .map(|s| s.replace('-', "").to_ascii_lowercase());
    let signed = q.unsigned == Some(false);
    let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
    let found = match target {
        Some(ref id) => guard.get(id),
        None => guard.values().next(),
    };
    match found {
        Some(entry) => Json(build_profile_json(
            entry,
            state.port,
            signed,
            &state.keypair,
        ))
        .into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn handle_minecraft_services_profile(State(state): State<YggState>) -> Response {
    let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = guard.values().next() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut base = build_profile_json(entry, state.port, true, &state.keypair);
    let mut skins = Vec::new();
    if let Some(ref hash) = entry.skin_hash {
        skins.push(json!({
            "id": hash,
            "state": "ACTIVE",
            "url": format!("http://127.0.0.1:{}/textures/{hash}", state.port),
            "variant": match entry.skin_variant {
                SkinVariant::Slim => "SLIM",
                SkinVariant::Wide => "CLASSIC",
            }
        }));
    }
    let mut capes = Vec::new();
    if let Some(hash) = entry.cape_hash.as_ref().or(entry.elytra_hash.as_ref()) {
        capes.push(json!({
            "id": hash,
            "state": "ACTIVE",
            "url": format!("http://127.0.0.1:{}/textures/{hash}", state.port),
            "alias": "Custom"
        }));
    }
    if let Some(obj) = base.as_object_mut() {
        obj.insert("skins".into(), Value::Array(skins));
        obj.insert("capes".into(), Value::Array(capes));
    }
    Json(base).into_response()
}

#[derive(Deserialize)]
struct SessionProfileQuery {
    unsigned: Option<bool>,
}

async fn handle_session_profile(
    State(state): State<YggState>,
    AxumPath(uuid): AxumPath<String>,
    Query(q): Query<SessionProfileQuery>,
) -> Response {
    let key = uuid.replace('-', "").to_ascii_lowercase();
    let signed = q.unsigned == Some(false);
    let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
    match guard.get(&key) {
        Some(entry) => Json(build_profile_json(
            entry,
            state.port,
            signed,
            &state.keypair,
        ))
        .into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

#[derive(Deserialize)]
struct HasJoinedQuery {
    username: Option<String>,
}

async fn handle_has_joined(
    State(state): State<YggState>,
    Query(q): Query<HasJoinedQuery>,
) -> Response {
    let Some(uname) = q
        .username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return StatusCode::NO_CONTENT.into_response();
    };
    let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
    match guard
        .values()
        .find(|entry| entry.username.eq_ignore_ascii_case(uname))
    {
        Some(entry) => {
            Json(build_profile_json(entry, state.port, true, &state.keypair)).into_response()
        }
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn handle_lookup_by_name(
    State(state): State<YggState>,
    AxumPath(username): AxumPath<String>,
) -> Response {
    let trimmed = username.trim();
    if trimmed.is_empty() {
        return StatusCode::NO_CONTENT.into_response();
    }
    let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
    match guard
        .values()
        .find(|entry| entry.username.eq_ignore_ascii_case(trimmed))
    {
        Some(entry) => Json(json!({
            "id": entry.raw_uuid,
            "name": entry.username,
        }))
        .into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn handle_lookup_by_uuid(
    State(state): State<YggState>,
    AxumPath(uuid): AxumPath<String>,
) -> Response {
    let key = uuid.replace('-', "").to_ascii_lowercase();
    let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
    match guard.get(&key) {
        Some(entry) => Json(json!({
            "id": entry.raw_uuid,
            "name": entry.username,
        }))
        .into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn handle_bulk_profiles(State(state): State<YggState>, body: Bytes) -> Json<Value> {
    let requested = serde_json::from_slice::<Vec<String>>(&body).unwrap_or_default();
    let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
    let mut seen = HashSet::new();
    let mut matched = Vec::new();
    for raw_name in requested.into_iter().take(100) {
        let name = raw_name.trim();
        if name.is_empty() {
            continue;
        }
        if let Some(entry) = guard
            .values()
            .find(|e| e.username.eq_ignore_ascii_case(name))
        {
            if seen.insert(entry.raw_uuid.clone()) {
                matched.push(json!({
                    "id": entry.raw_uuid,
                    "name": entry.username,
                }));
            }
        }
    }
    Json(Value::Array(matched))
}

async fn handle_player_attributes() -> Json<Value> {
    Json(json!({
        "privileges": {
            "onlineChat": { "enabled": true },
            "multiplayerServer": { "enabled": true },
            "multiplayerRealms": { "enabled": true },
            "telemetry": { "enabled": false }
        },
        "profanityFilterPreferences": {
            "profanityFilterOn": false
        },
        "banStatus": {
            "bannedScopes": {}
        }
    }))
}

async fn handle_auth_refresh(State(state): State<YggState>, body: Bytes) -> Response {
    let parsed = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let access_token = parsed
        .get("accessToken")
        .and_then(Value::as_str)
        .unwrap_or("0");
    let client_token = parsed
        .get("clientToken")
        .and_then(Value::as_str)
        .unwrap_or("0");
    let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = guard.values().next() else {
        return StatusCode::FORBIDDEN.into_response();
    };
    Json(json!({
        "accessToken": access_token,
        "clientToken": client_token,
        "selectedProfile": {
            "id": entry.raw_uuid,
            "name": entry.username,
        }
    }))
    .into_response()
}

#[derive(Deserialize)]
struct TextureQuery {
    #[serde(rename = "type")]
    tex_type: Option<String>,
}

async fn handle_texture(
    State(state): State<YggState>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<TextureQuery>,
) -> Response {
    let key = id
        .trim_end_matches(".png")
        .replace('-', "")
        .to_ascii_lowercase();

    if let Some(buf) = state.textures.get(&key) {
        return ([(header::CONTENT_TYPE, "image/png")], buf.as_ref().clone()).into_response();
    }

    let fallback_hash = {
        let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
        guard
            .get(&key)
            .and_then(|entry| match q.tex_type.as_deref() {
                Some("cape") => entry
                    .cape_hash
                    .clone()
                    .or_else(|| entry.elytra_hash.clone()),
                Some("elytra") => entry
                    .elytra_hash
                    .clone()
                    .or_else(|| entry.cape_hash.clone()),
                _ => entry.skin_hash.clone(),
            })
    };

    if let Some(hash) = fallback_hash {
        if let Some(buf) = state.textures.get(&hash) {
            return ([(header::CONTENT_TYPE, "image/png")], buf.as_ref().clone()).into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn handle_no_content() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn build_profile_json(
    entry: &YggdrasilPlayerEntry,
    port: u16,
    signed: bool,
    keypair: &YggdrasilRsaKeypair,
) -> Value {
    let mut textures = serde_json::Map::new();
    if let Some(ref skin_hash) = entry.skin_hash {
        let mut skin_obj = serde_json::Map::new();
        skin_obj.insert(
            "url".into(),
            Value::String(format!("http://127.0.0.1:{port}/textures/{skin_hash}")),
        );
        if entry.skin_variant == SkinVariant::Slim {
            skin_obj.insert("metadata".into(), json!({ "model": "slim" }));
        }
        textures.insert("SKIN".into(), Value::Object(skin_obj));
    }
    if let Some(cape_hash) = entry.cape_hash.as_ref().or(entry.elytra_hash.as_ref()) {
        textures.insert(
            "CAPE".into(),
            json!({
                "url": format!("http://127.0.0.1:{port}/textures/{cape_hash}")
            }),
        );
    }
    if let Some(ref elytra_hash) = entry.elytra_hash {
        textures.insert(
            "ELYTRA".into(),
            json!({
                "url": format!("http://127.0.0.1:{port}/textures/{elytra_hash}")
            }),
        );
    }

    let tex_payload = json!({
        "timestamp": Utc::now().timestamp_millis(),
        "profileId": entry.raw_uuid,
        "profileName": entry.username,
        "textures": textures,
    });
    let encoded = BASE64.encode(tex_payload.to_string().as_bytes());
    let mut prop = serde_json::Map::new();
    prop.insert("name".into(), Value::String("textures".into()));
    prop.insert("value".into(), Value::String(encoded.clone()));
    if signed {
        prop.insert(
            "signature".into(),
            Value::String(keypair.sign_sha1_pkcs1_v15_base64(encoded.as_bytes())),
        );
    }

    json!({
        "id": entry.raw_uuid,
        "name": entry.username,
        "properties": [Value::Object(prop)]
    })
}
