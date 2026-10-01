//! QR codes: SVG out (export screens), text in (screenshots and photos of export screens).

use std::collections::HashSet;
use std::io::Cursor;

use image::{ImageReader, Limits};
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use zeroize::Zeroizing;

/// The largest image dimension accepted, in pixels: a 50-megapixel photo still passes.
const MAX_SIDE: u32 = 12_000;

/// Why a QR code could not be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QrError {
    /// More data than a version-40 code holds.
    #[error("too much data for one QR code")]
    TooLarge,
}

/// Why an image could not be searched for codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    /// Not PNG, JPEG or WebP, damaged, or larger than [`MAX_SIDE`].
    #[error("the image cannot be read")]
    Unreadable,
}

/// `data` as an SVG QR code at level M: black modules on white with the four-module quiet zone,
/// at least 320 × 320 user units so the webview scales it crisply.
pub fn svg(data: &str) -> Result<Zeroizing<String>, QrError> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M).map_err(|_| QrError::TooLarge)?;
    Ok(Zeroizing::new(
        code.render::<svg::Color<'_>>().min_dimensions(320, 320).quiet_zone(true).dark_color(svg::Color("#000000")).light_color(svg::Color("#ffffff")).build(),
    ))
}

/// The text of every QR code in a PNG, JPEG or WebP image, in the order they were found. An
/// image with no readable code yields an empty list.
pub fn decode_image(bytes: &[u8]) -> Result<Vec<Zeroizing<String>>, ImageError> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|_| ImageError::Unreadable)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    reader.limits(limits);
    let luma = reader.decode().map_err(|_| ImageError::Unreadable)?.to_luma8();
    Ok(decode_luma(&luma))
}

/// rxing (a port of ZXing) rather than rqrr: on Lockra's own densest code (version 19) rqrr missed
/// one pixel per module, a 0.5× rescale, a 15° tilt, a σ = 1 blur and JPEG at quality 60, all of
/// which rxing reads; those are what a photo of a laptop screen looks like.
/// The text of every QR code in raw RGBA pixels (a clipboard image).
pub fn decode_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<Zeroizing<String>>, ImageError> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(ImageError::Unreadable);
    }
    let image = image::RgbaImage::from_raw(width, height, rgba.to_vec()).ok_or(ImageError::Unreadable)?;
    Ok(decode_luma(&image::DynamicImage::ImageRgba8(image).to_luma8()))
}

fn decode_luma(luma: &image::GrayImage) -> Vec<Zeroizing<String>> {
    let mut hints = rxing::DecodeHints { PossibleFormats: Some(HashSet::from([rxing::BarcodeFormat::QR_CODE])), ..Default::default() };
    let results = rxing::helpers::detect_multiple_in_luma_with_hints(luma.as_raw().clone(), luma.width(), luma.height(), &mut hints).unwrap_or_default();
    let mut found: Vec<Zeroizing<String>> = Vec::new();
    for result in results {
        let text = Zeroizing::new(result.getText().to_owned());
        if !found.contains(&text) {
            found.push(text);
        }
    }
    found
}

#[cfg(test)]
pub(crate) mod tests {
    use image::{DynamicImage, GrayImage, Luma, imageops};

    use super::*;

    /// A code drawn module by module: `scale` pixels per module, four modules of quiet zone.
    pub(crate) fn render(data: &str, scale: u32) -> GrayImage {
        let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M).unwrap();
        let width = code.width() as u32;
        let colors = code.to_colors();
        let side = (width + 8) * scale;
        GrayImage::from_fn(side, side, |x, y| {
            let (mx, my) = ((x / scale) as i64 - 4, (y / scale) as i64 - 4);
            let dark = mx >= 0 && my >= 0 && mx < width as i64 && my < width as i64 && colors[(my * width as i64 + mx) as usize] == qrcode::Color::Dark;
            Luma([if dark { 0 } else { 255 }])
        })
    }

    fn rotate(img: &GrayImage, degrees: f64) -> GrayImage {
        let (w, h) = (img.width() as f64, img.height() as f64);
        let (sin, cos) = degrees.to_radians().sin_cos();
        let side = ((w * cos.abs() + h * sin.abs()).ceil() as u32).max(1);
        let (cx, cy, c) = (w / 2.0, h / 2.0, side as f64 / 2.0);
        GrayImage::from_fn(side, side, |x, y| {
            let (dx, dy) = (x as f64 - c, y as f64 - c);
            let (sx, sy) = (cos * dx + sin * dy + cx, -sin * dx + cos * dy + cy);
            if sx >= 0.0 && sy >= 0.0 && sx < w && sy < h { *img.get_pixel(sx as u32, sy as u32) } else { Luma([255]) }
        })
    }

    fn png(img: &GrayImage) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageLuma8(img.clone()).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn jpeg(img: &GrayImage, quality: u8) -> Vec<u8> {
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality).encode_image(&DynamicImage::ImageLuma8(img.clone())).unwrap();
        out
    }

    /// A Google export code with ten accounts: the densest code Lockra itself emits.
    fn dense_payload() -> String {
        let accounts: Vec<lockra_otp::OtpAuth> = (0..10)
            .map(|i| lockra_otp::uri::parse(&format!("otpauth://totp/Service{i}:user{i}%40example.com?secret=JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP")).unwrap())
            .collect();
        let refs: Vec<&lockra_otp::OtpAuth> = accounts.iter().collect();
        crate::google::encode(&refs).unwrap().remove(0).uri.to_string()
    }

    fn decodes_to(bytes: &[u8], expected: &str) {
        let found = decode_image(bytes).unwrap();
        assert_eq!(found.len(), 1, "expected one code");
        assert_eq!(found[0].as_str(), expected);
    }

    #[test]
    fn svg_is_a_black_on_white_code() {
        let drawn = svg("otpauth://totp/x?secret=GEZDGNBV").unwrap();
        assert!(drawn.starts_with("<?xml") && drawn.contains("<svg"), "{}", &drawn[..80]);
        assert!(drawn.contains("#000000") && drawn.contains("#ffffff"));
        assert_eq!(svg("x".repeat(8000).as_str()).unwrap_err(), QrError::TooLarge);
    }

    #[test]
    fn decodes_across_scales() {
        let data = dense_payload();
        for scale in [1, 2, 3, 6, 12] {
            decodes_to(&png(&render(&data, scale)), &data);
        }
        // 0.5× of a 4-pixel rendering: two pixels per module, filtered down.
        let half = imageops::resize(&render(&data, 4), (render(&data, 4).width()) / 2, render(&data, 4).height() / 2, imageops::FilterType::Triangle);
        decodes_to(&png(&half), &data);
    }

    #[test]
    fn decodes_rotated_codes() {
        let data = dense_payload();
        let base = render(&data, 5);
        decodes_to(&png(&imageops::rotate90(&base)), &data);
        decodes_to(&png(&imageops::rotate180(&base)), &data);
        decodes_to(&png(&rotate(&base, 15.0)), &data);
    }

    #[test]
    fn decodes_blurred_and_compressed_codes() {
        let data = dense_payload();
        let base = render(&data, 5);
        decodes_to(&png(&imageops::blur(&base, 1.0)), &data);
        decodes_to(&jpeg(&base, 60), &data);
    }

    #[test]
    fn finds_codes_inside_a_screenshot() {
        let first = dense_payload();
        let second = "otpauth://totp/Solo:me?secret=JBSWY3DPEHPK3PXP".to_owned();
        let mut screen = GrayImage::from_fn(1600, 900, |x, y| Luma([if (x / 40 + y / 40) % 2 == 0 { 236 } else { 250 }]));
        imageops::overlay(&mut screen, &render(&first, 4), 120, 140);
        imageops::overlay(&mut screen, &render(&second, 6), 1000, 200);
        let found = decode_image(&png(&screen)).unwrap();
        let mut texts: Vec<&str> = found.iter().map(|t| t.as_str()).collect();
        texts.sort_unstable();
        let mut expected = vec![first.as_str(), second.as_str()];
        expected.sort_unstable();
        assert_eq!(texts, expected);
    }

    #[test]
    fn decodes_raw_rgba_pixels() {
        let data = "otpauth://totp/Clip:board?secret=JBSWY3DPEHPK3PXP";
        let rgba = DynamicImage::ImageLuma8(render(data, 4)).to_rgba8();
        let found = decode_rgba(rgba.width(), rgba.height(), rgba.as_raw()).unwrap();
        assert_eq!(found.iter().map(|t| t.as_str()).collect::<Vec<_>>(), [data]);
        assert_eq!(decode_rgba(2, 2, &[0; 3]).unwrap_err(), ImageError::Unreadable);
        assert_eq!(decode_rgba(0, 2, &[]).unwrap_err(), ImageError::Unreadable);
    }

    #[test]
    fn no_code_and_garbage_are_told_apart() {
        let blank = GrayImage::from_pixel(300, 300, Luma([255]));
        assert!(decode_image(&png(&blank)).unwrap().is_empty());
        assert_eq!(decode_image(b"not an image").unwrap_err(), ImageError::Unreadable);
    }
}
