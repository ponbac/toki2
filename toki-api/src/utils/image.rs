use image::imageops::FilterType;
use std::error::Error;

/// `input` = original image bytes (PNG/JPEG/etc.)
///
/// Returns compressed WebP bytes.
///
/// `filter` is the filter type to use for the resize operation.
/// If `None`, `FilterType::Lanczos3` is used.
pub fn compress_image_webp(
    input: &[u8],
    max_size: u32, // e.g. 256
    quality: f32,  // 0.0 - 100.0
    filter: Option<FilterType>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let img = image::load_from_memory(input)?;

    // Resize to fit inside max_size x max_size (keeping aspect ratio)
    let resized = img.resize(max_size, max_size, filter.unwrap_or(FilterType::Lanczos3));

    // Convert to RGBA8 for the encoder
    let rgba = resized.to_rgba8();
    let (width, height) = rgba.dimensions();

    // Encode as WebP
    let encoder = webp::Encoder::from_rgba(&rgba, width, height);
    let webp = encoder.encode(quality); // quality 0–100

    Ok(webp.to_vec())
}
