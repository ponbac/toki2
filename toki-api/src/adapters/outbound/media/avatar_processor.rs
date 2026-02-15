use image::imageops::FilterType;

use crate::domain::{models::AvatarImage, ports::outbound::AvatarProcessor, AvatarError};

pub struct WebpAvatarProcessor;

impl Default for WebpAvatarProcessor {
    fn default() -> Self {
        Self
    }
}

impl AvatarProcessor for WebpAvatarProcessor {
    fn process(
        &self,
        input: &[u8],
        _content_type: Option<&str>,
    ) -> Result<AvatarImage, AvatarError> {
        let image = image::load_from_memory(input).map_err(|_| AvatarError::InvalidImage)?;

        let resized = image.resize(512, 512, FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let (width, height) = rgba.dimensions();

        let encoder = webp::Encoder::from_rgba(&rgba, width, height);
        let webp = encoder.encode(80.0);

        Ok(AvatarImage::new(webp.to_vec(), "image/webp"))
    }
}
