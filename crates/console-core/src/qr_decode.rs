//! Decode an uploaded image of a mesh invite QR into a validated join token.
//!
//! The invite QR an owner shares encodes a join command such as
//! `mesh-llm serve --join <token>` (see the app's `qr.rs`/onboarding success
//! screen). Joining "by uploading a photo of the QR" reads that image, pulls
//! the token back out, and runs it through the same SDK validation the paste
//! path uses ([`crate::sdk_session::validate_invite_token`]).
//!
//! Split into small pieces so the fallible image/QR step and the pure token
//! extraction are independently testable without a live SDK token.

/// Read the text payload out of a QR code in an encoded image (PNG/JPEG bytes).
///
/// Real uploads are rarely the crisp PNG our own modal renders — they're phone
/// photos or scaled screenshots where the code is small, soft, or low-contrast.
/// `rqrr`'s single pass misses those, so we try a few normalized variants
/// (rescaled to a detector-friendly size, plus a binarized pass) and take the
/// first that decodes.
pub fn read_qr_text(image_bytes: &[u8]) -> Result<String, String> {
    let img = image::load_from_memory(image_bytes)
        .map_err(|e| format!("could not read image: {e}"))?;
    for luma in qr_detection_candidates(&img) {
        let mut prepared = rqrr::PreparedImage::prepare(luma);
        for grid in prepared.detect_grids() {
            if let Ok((_meta, content)) = grid.decode() {
                return Ok(content);
            }
        }
    }
    Err("no QR code found in the image".to_string())
}

/// Grayscale variants to feed the detector, cheapest/most-likely first.
fn qr_detection_candidates(img: &image::DynamicImage) -> Vec<image::GrayImage> {
    use image::imageops::{resize, FilterType};

    let base = img.to_luma8();
    let (w, h) = (base.width(), base.height());
    let long_edge = w.max(h);

    let mut candidates = vec![base.clone()];

    // `rqrr` detects most reliably when the code's modules are a few pixels
    // each: tiny images get upscaled, oversized photos downscaled.
    if long_edge > 0 {
        for target in [1024u32, 1600] {
            if long_edge != target && w > 0 && h > 0 {
                let scale = target as f32 / long_edge as f32;
                let nw = ((w as f32 * scale).round() as u32).max(1);
                let nh = ((h as f32 * scale).round() as u32).max(1);
                candidates.push(resize(&base, nw, nh, FilterType::Triangle));
            }
        }
    }

    // Hard black/white pass rescues low-contrast or unevenly lit photos.
    candidates.push(binarize(&base));
    candidates
}

/// Threshold to pure black/white using the image's mean luma.
fn binarize(src: &image::GrayImage) -> image::GrayImage {
    let pixels = (src.width() as u64 * src.height() as u64).max(1);
    let sum: u64 = src.pixels().map(|p| p.0[0] as u64).sum();
    let mean = (sum / pixels) as u8;
    let mut out = src.clone();
    for p in out.pixels_mut() {
        p.0[0] = if p.0[0] > mean { 255 } else { 0 };
    }
    out
}

/// Pull the join token out of a decoded QR payload. Handles the invite command
/// shapes (`mesh-llm serve --join <token>`, `mesh-llm --join <token> --model x`,
/// `mesh-llm --client --join <token>`) as well as a bare token with no command.
pub fn extract_join_token(qr_text: &str) -> String {
    let text = qr_text.trim();
    match text.find("--join") {
        Some(idx) => text[idx + "--join".len()..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string(),
        None => text.to_string(),
    }
}

/// Decode an uploaded QR image to a canonical, validated invite token. Errors
/// are human-readable for surfacing next to the upload button.
pub fn decode_invite_qr(image_bytes: &[u8]) -> Result<String, String> {
    let payload = read_qr_text(image_bytes)?;
    let candidate = extract_join_token(&payload);
    if candidate.is_empty() {
        return Err("that QR code doesn't contain an invite token".to_string());
    }
    crate::sdk_session::validate_invite_token(&candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rasterise a QR for `payload` to PNG bytes using only the workspace
    /// `image` crate (no coupling to `qrcode`'s optional `image` feature).
    fn qr_png(payload: &str) -> Vec<u8> {
        let code = qrcode::QrCode::new(payload.as_bytes()).unwrap();
        let width = code.width();
        let colors = code.to_colors();
        let scale: u32 = 6;
        let quiet: u32 = 4;
        let dim = (width as u32 + quiet * 2) * scale;
        let mut img = image::GrayImage::from_pixel(dim, dim, image::Luma([255u8]));
        for (i, color) in colors.iter().enumerate() {
            if *color == qrcode::Color::Dark {
                let x0 = (i % width) as u32 + quiet;
                let y0 = (i / width) as u32 + quiet;
                for dy in 0..scale {
                    for dx in 0..scale {
                        img.put_pixel(x0 * scale + dx, y0 * scale + dy, image::Luma([0u8]));
                    }
                }
            }
        }
        let mut bytes = Vec::new();
        image::DynamicImage::ImageLuma8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn reads_qr_text_round_trip() {
        let payload = "mesh-llm serve --join EXAMPLE-TOKEN-123";
        let png = qr_png(payload);
        assert_eq!(read_qr_text(&png).unwrap(), payload);
    }

    #[test]
    fn reads_low_contrast_qr() {
        // Simulate a dim photo: remap pure black/white to muddy grays so the
        // detector has to lean on the normalization/binarize passes.
        let payload = "mesh-llm serve --join DIM-PHOTO-TOKEN";
        let png = qr_png(payload);
        let mut img = image::load_from_memory(&png).unwrap().to_luma8();
        for p in img.pixels_mut() {
            p.0[0] = if p.0[0] < 128 { 96 } else { 168 };
        }
        let mut bytes = Vec::new();
        image::DynamicImage::ImageLuma8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        assert_eq!(read_qr_text(&bytes).unwrap(), payload);
    }

    #[test]
    fn read_qr_text_errors_on_image_without_a_code() {
        // A flat white image carries no QR grid.
        let blank = image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(
            96,
            96,
            image::Luma([255u8]),
        ));
        let mut bytes = Vec::new();
        blank
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        assert!(read_qr_text(&bytes).is_err());
    }

    #[test]
    fn read_qr_text_errors_on_non_image_bytes() {
        assert!(read_qr_text(b"not an image").is_err());
    }

    #[test]
    fn extracts_token_from_each_invite_shape() {
        assert_eq!(extract_join_token("mesh-llm serve --join ABC"), "ABC");
        assert_eq!(
            extract_join_token("mesh-llm --join ABC --model qwen3"),
            "ABC"
        );
        assert_eq!(extract_join_token("mesh-llm --client --join ABC"), "ABC");
        assert_eq!(extract_join_token("  bare-token  "), "bare-token");
        assert_eq!(extract_join_token("mesh-abc123"), "mesh-abc123");
    }
}
