//! Raster export of the graph: PNG and JPEG.
//!
//! The SVG produced by the engine is rendered with `resvg`, so the picture is
//! exactly the one the user sees in the window (same ticks, text, notes and
//! annotations) and no screen record is involved. The bundled DejaVu faces are
//! registered with the renderer, so text does not depend on the fonts installed
//! on the machine.

use resvg::{tiny_skia, usvg};

/// Renders an SVG to PNG at `scale` times its own size.
///
/// # Errors
///
/// Returns a message when the SVG is malformed or the pixmap cannot be created.
pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    let pixmap = rasterize(svg, scale)?;
    pixmap
        .encode_png()
        .map_err(|error| format!("PNG encoding failed: {error}"))
}

/// Renders an SVG to a JPEG with the given quality (1–100).
///
/// # Errors
///
/// As [`svg_to_png`], plus a JPEG encoding failure.
pub fn svg_to_jpeg(svg: &str, scale: f32, quality: u8) -> Result<Vec<u8>, String> {
    let pixmap = rasterize(svg, scale)?;
    let width = u16::try_from(pixmap.width()).map_err(|_| "image is too wide".to_owned())?;
    let height = u16::try_from(pixmap.height()).map_err(|_| "image is too tall".to_owned())?;
    let data = pixmap.data();
    let mut rgb = Vec::with_capacity(data.len() / 4 * 3);
    for pixel in data.as_chunks::<4>().0 {
        rgb.extend_from_slice(&pixel[..3]);
    }
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, quality)
        .encode(&rgb, width, height, jpeg_encoder::ColorType::Rgb)
        .map_err(|error| format!("JPEG encoding failed: {error}"))?;
    Ok(out)
}

fn rasterize(svg: &str, scale: f32) -> Result<tiny_skia::Pixmap, String> {
    let mut options = usvg::Options::default();
    for font in [
        &include_bytes!("../../fp-ui/assets/DejaVuSans.ttf")[..],
        &include_bytes!("../../fp-ui/assets/DejaVuSerif.ttf")[..],
        &include_bytes!("../../fp-ui/assets/DejaVuSansMono.ttf")[..],
    ] {
        options.fontdb_mut().load_font_data(font.to_vec());
    }
    let tree = usvg::Tree::from_str(svg, &options)
        .map_err(|error| format!("the graph could not be read: {error}"))?;
    let scale = if scale.is_finite() {
        scale.clamp(0.5, 4.0)
    } else {
        2.0
    };
    let width = (tree.size().width() * scale).round().max(1.0) as u32;
    let height = (tree.size().height() * scale).round().max(1.0) as u32;
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).ok_or_else(|| "the image is too large".to_owned())?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}
