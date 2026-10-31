//! Encoding shared by generated image backends and WeChat upload preparation.

use crate::error::{Error, Result};
use image::{DynamicImage, ImageFormat};
use std::{io::Cursor, path::Path};

pub(crate) fn encode_image(image: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    if format == ImageFormat::Jpeg {
        // JPEG does not support alpha channels.
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 95)
            .encode_image(&image.to_rgb8())
            .map_err(|e| Error::generic(format!("Failed to encode JPEG image: {e}")))?;
    } else {
        image
            .write_to(&mut output, format)
            .map_err(|e| Error::generic(format!("Failed to encode image: {e}")))?;
    }
    Ok(output.into_inner())
}

pub(crate) fn encode_for_path(bytes: Vec<u8>, path: &Path) -> Result<Vec<u8>> {
    let Ok(format) = ImageFormat::from_path(path) else {
        return Ok(bytes);
    };
    if image::guess_format(&bytes).ok() == Some(format) {
        return Ok(bytes);
    }
    let image = image::load_from_memory(&bytes)
        .map_err(|e| Error::generic(format!("Failed to decode generated image: {e}")))?;
    encode_image(&image, format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_png_is_reencoded_for_jpeg_path() {
        let bytes = encode_image(&DynamicImage::new_rgba8(8, 4), ImageFormat::Png).unwrap();
        let jpeg = encode_for_path(bytes, Path::new("cover.JPG")).unwrap();
        assert_eq!(image::guess_format(&jpeg).unwrap(), ImageFormat::Jpeg);
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (8, 4));
    }
}
