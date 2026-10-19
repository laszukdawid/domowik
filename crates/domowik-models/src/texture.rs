//! Bounded embedded base-colour images. UV origin is the glTF image's top left.
use serde::Serialize;
use std::sync::Arc;

pub(crate) const MAX_EDGE: u32 = 2048;
const MAX_SOURCE_EDGE: u32 = 4096;
pub(crate) const MAX_TEXTURE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_LAYERS: usize = 16;

#[derive(Clone)]
pub(crate) struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone)]
pub(crate) struct Surface {
    pub image: Arc<Image>,
    /// 0 repeat, 1 clamp, 2 mirrored repeat. Shared CPU/GPU convention.
    pub wrap: [u8; 2],
}

/// Derived GPU texture array, tightly packed sRGB RGBA8, top-origin rows.
/// All layers share a resolution (at most 2048 per edge), at most 16 layers
/// and 64 MiB total. Five floats per mesh vertex: UV, layer, wrap S, wrap T.
/// A layer of -1 means untextured. Images are resized only in this derived
/// representation; imported bytes are unchanged. Caller owns all buffers.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTexture {
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub rgba: Vec<u8>,
    pub coordinates: Vec<f32>,
}

pub(crate) fn decode(bytes: &[u8], mime: &str) -> Result<Arc<Image>, String> {
    let format = match mime {
        "image/png" => image::ImageFormat::Png,
        "image/jpeg" => image::ImageFormat::Jpeg,
        "image/webp" => image::ImageFormat::WebP,
        _ => return Err("Unsupported embedded base-colour image format".into()),
    };
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_EDGE);
    limits.max_image_height = Some(MAX_SOURCE_EDGE);
    limits.max_alloc = Some(MAX_TEXTURE_BYTES as u64);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| format!("Base-colour image: {e}"))?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err("Empty base-colour image".into());
    }
    let decoded = if decoded.width() > MAX_EDGE || decoded.height() > MAX_EDGE {
        decoded.resize(MAX_EDGE, MAX_EDGE, image::imageops::FilterType::Triangle)
    } else {
        decoded
    };
    Ok(Arc::new(Image {
        width: decoded.width(),
        height: decoded.height(),
        rgba: decoded.into_rgba8().into_raw(),
    }))
}

pub(crate) fn linear(v: f64) -> f64 {
    if v == 0. || v == 1. {
        return v;
    }
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
pub(crate) fn srgb(v: f64) -> f64 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}

fn coordinate(v: f64, wrap: u8) -> f64 {
    match wrap {
        1 => v.clamp(0., 1.),
        2 => 1. - (v.rem_euclid(2.) - 1.).abs(),
        _ => v.rem_euclid(1.),
    }
}

/// Bilinear filtering in linear RGB; alpha remains linear. Wrapping is applied
/// to each texel tap so repeat seams match GPU sampling, including negative UVs.
pub(crate) fn sample(surface: &Surface, uv: [f64; 2], factor: [f64; 4]) -> [f64; 4] {
    static LINEAR: std::sync::OnceLock<[f64; 256]> = std::sync::OnceLock::new();
    let lookup = LINEAR.get_or_init(|| std::array::from_fn(|i| linear(i as f64 / 255.)));
    let image = &surface.image;
    let p = [
        uv[0] * f64::from(image.width) - 0.5,
        uv[1] * f64::from(image.height) - 0.5,
    ];
    let base = p.map(f64::floor);
    let f = [p[0] - base[0], p[1] - base[1]];
    let mut out = [0.; 4];
    for y in 0..2 {
        for x in 0..2 {
            let tap = [base[0] + x as f64 + 0.5, base[1] + y as f64 + 0.5];
            let u = coordinate(tap[0] / f64::from(image.width), surface.wrap[0]);
            let v = coordinate(tap[1] / f64::from(image.height), surface.wrap[1]);
            let ix = ((u * f64::from(image.width)) as u32).min(image.width - 1);
            let iy = ((v * f64::from(image.height)) as u32).min(image.height - 1);
            let i = ((iy * image.width + ix) * 4) as usize;
            let weight =
                (if x == 0 { 1. - f[0] } else { f[0] }) * (if y == 0 { 1. - f[1] } else { f[1] });
            for (c, out) in out.iter_mut().enumerate() {
                let value = f64::from(image.rgba[i + c]) / 255.;
                *out += weight
                    * if c < 3 {
                        lookup[image.rgba[i + c] as usize]
                    } else {
                        value
                    };
            }
        }
    }
    for c in 0..3 {
        out[c] = srgb(out[c] * linear(factor[c]));
    }
    out[3] *= factor[3];
    out
}

pub(crate) fn array(
    images: &[Arc<Image>],
    coordinates: Vec<f32>,
) -> Result<Option<ModelTexture>, String> {
    if images.is_empty() {
        return Ok(None);
    }
    if images.len() > MAX_LAYERS {
        return Err("Model exceeds 16 base-colour images".into());
    }
    let width = images.iter().map(|i| i.width).max().unwrap();
    let height = images.iter().map(|i| i.height).max().unwrap();
    if width as usize * height as usize * 4 * images.len() > MAX_TEXTURE_BYTES {
        return Err("GPU base-colour images exceed 64 MiB".into());
    }
    let mut rgba = Vec::new();
    for image in images {
        if image.width == width && image.height == height {
            rgba.extend_from_slice(&image.rgba);
        } else {
            let pixels = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
                .ok_or("Invalid decoded image")?;
            rgba.extend(
                image::imageops::resize(
                    &pixels,
                    width,
                    height,
                    image::imageops::FilterType::Triangle,
                )
                .into_raw(),
            );
        }
    }
    Ok(Some(ModelTexture {
        width,
        height,
        layers: images.len() as u32,
        rgba,
        coordinates,
    }))
}
