//! Image optimization engines.
//!
//! Each engine takes a byte slice and, on success, returns a *complete candidate
//! file* plus the human-readable tool name shown in the "Best tool" column.
//! The pipeline in [`crate::optimize`] is responsible for keeping only candidates
//! that are smaller than the current best.

use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result, bail};
use imagequant::{Attributes, RGBA};
use oxipng::{Deflater, Options as OxipngOptions, StripChunks, ZopfliOptions};

// ---------------------------------------------------------------------------
// PNG
// ---------------------------------------------------------------------------

/// OxiPNG, configured like `-oN -i0 -a [--strip=safe]`.
pub fn oxipng(
    data: &[u8],
    level: u8,
    strip: bool,
    zopfli: bool,
    timeout: Option<std::time::Duration>,
) -> Result<Vec<u8>> {
    let mut opts = OxipngOptions::from_preset(level.min(6));
    opts.interlace = Some(false);
    opts.optimize_alpha = true;
    opts.strip = if strip {
        StripChunks::Safe
    } else {
        StripChunks::None
    };
    opts.timeout = timeout;
    if zopfli {
        let mut z = ZopfliOptions::default();
        // Upstream's zopflipng uses `3 + 3*level` iterations.
        z.iteration_count = std::num::NonZeroU64::new(u64::from(3 + 3 * level)).unwrap();
        opts.deflater = Deflater::Zopfli(z);
    }
    oxipng::optimize_from_memory(data, &opts).context("oxipng failed")
}

/// Lossy PNG quantization, mirroring pngquant's `--quality min-max`.
///
/// `quality` is the minimum acceptable quality (30..=99). The output keeps an
/// alpha channel when the source has transparency.
pub fn pngquant(data: &[u8], quality: u8, speed: u8) -> Result<Vec<u8>> {
    pngquant_with_options(data, quality, speed, true)
}

/// Like [`pngquant`], preserving color-space, DPI and text ancillary chunks from
/// `data` when `strip` is `false` (matching `pngquant` CLI without `--strip`).
pub fn pngquant_with_options(data: &[u8], quality: u8, speed: u8, strip: bool) -> Result<Vec<u8>> {
    if crate::filetype::is_apng(data) {
        bail!("pngquant does not support animated PNG (APNG)");
    }
    let img = image::load_from_memory(data)?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);

    let mut attr = Attributes::new();
    attr.set_quality(quality, quality.saturating_add(20).min(100))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    attr.set_speed(i32::from(speed.min(10)))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let rgba: Vec<RGBA> = img
        .pixels()
        .map(|p| RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();
    let mut liq_image = attr
        .new_image(rgba, w, h, 0.0)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut result = attr
        .quantize(&mut liq_image)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    result
        .set_dithering_level(1.0)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let (palette, indices) = result
        .remapped(&mut liq_image)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let preserved = if strip {
        Vec::new()
    } else {
        extract_png_ancillary_chunks(data)
    };
    encode_indexed_png(w as u32, h as u32, &palette, &indices, &preserved)
}

/// Extracts raw `[len + type + data + crc]` PNG chunks that remain valid on an
/// indexed-color output (`iCCP`, `sRGB`, `gAMA`, `cHRM`, `pHYs`, `tEXt`, `zTXt`,
/// `iTXt`, `eXIf`). Placed right after `IHDR` (before `PLTE` and `IDAT`), which
/// satisfies PNG 1.2 chunk ordering rules for all nine chunk types.
fn extract_png_ancillary_chunks(data: &[u8]) -> Vec<u8> {
    const KEEP: &[&[u8; 4]] = &[
        b"iCCP", b"sRGB", b"gAMA", b"cHRM", b"pHYs", b"tEXt", b"zTXt", b"iTXt", b"eXIf",
    ];
    let mut out = Vec::new();
    if data.len() < 8 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
        return out;
    }
    let mut i = 8usize;
    while i + 12 <= data.len() {
        let len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        if i + 12 + len > data.len() {
            break;
        }
        let typ = &data[i + 4..i + 8];
        if typ == b"IEND" {
            break;
        }
        if KEEP.iter().any(|k| *k == typ) {
            out.extend_from_slice(&data[i..i + 12 + len]);
        }
        i += 12 + len;
    }
    out
}

fn encode_indexed_png(
    width: u32,
    height: u32,
    palette: &[RGBA],
    indices: &[u8],
    ancillary_chunks: &[u8],
) -> Result<Vec<u8>> {
    let mut rgb = Vec::with_capacity(palette.len() * 3);
    let mut alpha = Vec::with_capacity(palette.len());
    for p in palette {
        rgb.extend_from_slice(&[p.r, p.g, p.b]);
        alpha.push(p.a);
    }
    // tRNS may be truncated after the last non-opaque entry.
    let last_transparent = alpha.iter().rposition(|a| *a != 255);
    let trns = last_transparent.map(|i| &alpha[..=i]);

    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(Cursor::new(&mut out), width, height);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(rgb);
        if let Some(t) = trns {
            enc.set_trns(t.to_vec());
        }
        let mut writer = enc.write_header()?;
        writer.write_image_data(indices)?;
    }
    if !ancillary_chunks.is_empty() && out.len() >= 33 {
        // Byte 33 is immediately after the 8-byte PNG signature + 25-byte IHDR chunk.
        let mut merged = Vec::with_capacity(out.len() + ancillary_chunks.len());
        merged.extend_from_slice(&out[..33]);
        merged.extend_from_slice(ancillary_chunks);
        merged.extend_from_slice(&out[33..]);
        out = merged;
    }
    // Pick better filters; level 2 keeps this fast relative to quantization.
    let mut opts = OxipngOptions::from_preset(2);
    opts.interlace = Some(false);
    opts.strip = StripChunks::None;
    Ok(oxipng::optimize_from_memory(&out, &opts).unwrap_or(out))
}

// ---------------------------------------------------------------------------
// JPEG
// ---------------------------------------------------------------------------

/// Lossless jpegtran-style recompression (`-copy none|all -optimize`).
pub fn jpegtran(data: &[u8], strip: bool) -> Result<Vec<u8>> {
    let mut transform = turbojpeg::Transform::default();
    transform.optimize = true;
    transform.copy_none = strip;
    // jpegtran's default output is baseline; `-progressive` is not passed.
    transform.progressive = false;

    let mut transformer = turbojpeg::Transformer::new().context("turbojpeg init")?;
    transformer
        .transform_to_vec(&transform, data)
        .context("jpegtran transform")
}

/// Lossy JPEG recompression at `quality`, mirroring jpegoptim's quality mode.
///
/// `strip` removes metadata; when false, APP1..APP15 and COM segments from the
/// source are re-inserted into the output.
pub fn jpegoptim(data: &[u8], quality: u8, strip: bool) -> Result<Vec<u8>> {
    let image = turbojpeg::decompress(data, turbojpeg::PixelFormat::RGB).context("jpeg decode")?;

    let mut comp = turbojpeg::Compressor::new()?;
    comp.set_quality(i32::from(quality))?;
    comp.set_progressive(true)?;
    comp.set_optimize(true)?;
    let out = comp
        .compress_to_owned(image.as_deref())
        .context("jpeg encode")?
        .to_vec();

    if strip {
        Ok(out)
    } else {
        copy_jpeg_metadata(data, out)
    }
}

fn copy_jpeg_metadata(src: &[u8], dst: Vec<u8>) -> Result<Vec<u8>> {
    use img_parts::jpeg::{Jpeg, JpegSegment, markers};

    let src = Jpeg::from_bytes(src.to_vec().into()).context("reading source JPEG")?;
    let mut dst = Jpeg::from_bytes(dst.into()).context("reading recompressed JPEG")?;
    let metadata: Vec<JpegSegment> = src
        .segments()
        .iter()
        .filter(|s| matches!(s.marker(), markers::APP1..=markers::APP15 | markers::COM))
        .cloned()
        .collect();
    for segment in metadata.into_iter().rev() {
        dst.segments_mut().insert(0, segment);
    }
    let mut out = Vec::new();
    dst.encoder().write_to(&mut out)?;
    Ok(out)
}

/// Guetzli is not available as a maintained Rust crate. Keep the hook so the
/// pipeline can report the tool as unsupported instead of silently ignoring it.
pub fn guetzli(_data: &[u8], _quality: u8) -> Result<Vec<u8>> {
    bail!("Guetzli is not available in this build")
}

// ---------------------------------------------------------------------------
// GIF (external gifsicle — the same CLI the original app bundles)
// ---------------------------------------------------------------------------

pub fn find_gifsicle() -> Option<PathBuf> {
    crate::tools::find_external("gifsicle")
}

/// Lossless (`quality == 100`) or lossy gifsicle run.
pub fn gifsicle(
    input: &Path,
    interlace: bool,
    quality: u8,
    small: bool,
    large: bool,
) -> Result<Vec<u8>> {
    gifsicle_cancellable(
        input,
        interlace,
        quality,
        small,
        large,
        &AtomicBool::new(false),
    )
}

pub fn gifsicle_cancellable(
    input: &Path,
    interlace: bool,
    quality: u8,
    small: bool,
    large: bool,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    let exe = find_gifsicle().context("gifsicle not found")?;
    let out = temp_path("gif");
    let mut args: Vec<OsString> = Vec::new();
    let lossy = quality < 100;
    if lossy {
        let mut loss = (f64::from(100 - quality)).powf(1.8) / 5.0;
        if small {
            loss = 1.0 + loss / 8.0;
        } else if !large {
            loss = 1.0 + loss / 2.0;
        }
        args.push(format!("--lossy={}", loss as i64).into());
    }
    args.extend([
        "-o".into(),
        out.clone().into_os_string(),
        if interlace {
            "--interlace".into()
        } else {
            "--no-interlace".into()
        },
        "-O3".into(),
        "--careful".into(),
        "--no-comments".into(),
        "--no-names".into(),
        "--same-delay".into(),
        "--same-loopcount".into(),
        "--no-warnings".into(),
        "--".into(),
        input.as_os_str().to_os_string(),
    ]);

    let output = crate::tools::run_process_cancellable(&exe, &args, cancel, None, false);
    let bytes = std::fs::read(&out);
    let _ = std::fs::remove_file(&out);
    let output = output?;
    if !output.status.success() {
        bail!(
            "gifsicle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    bytes.context("reading gifsicle output")
}

/// Byte-slice entry point for the serial pipeline.
pub fn gifsicle_data(data: &[u8], quality: u8, interlace: bool) -> Result<Vec<u8>> {
    gifsicle_data_cancellable(data, quality, interlace, &AtomicBool::new(false))
}

pub fn gifsicle_data_cancellable(
    data: &[u8],
    quality: u8,
    interlace: bool,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    let input = temp_path("gif");
    std::fs::write(&input, data)?;
    // Upstream size classes for non-PNG files: small < 10 KB, large > 1 MB.
    let small = data.len() < 10 * 1024;
    let large = data.len() > 1_000_000;
    let out = gifsicle_cancellable(&input, interlace, quality, small, large, cancel);
    let _ = std::fs::remove_file(&input);
    out
}

fn temp_path(ext: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "imageoptim-rust-{}-{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
        ext
    ));
    p
}

// ---------------------------------------------------------------------------
// SVG
// ---------------------------------------------------------------------------

use svgcleaner::CleaningOptions;
use svgcleaner::cleaner;

/// svgcleaner with its CLI's default cleaning preset.
pub fn svgcleaner(data: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(data).context("SVG is not UTF-8")?;
    let mut doc = cleaner::parse_data(text, &svgcleaner::ParseOptions::default())
        .map_err(|e| anyhow::anyhow!("SVG parse: {e}"))?;

    let mut opts = CleaningOptions::default();
    opts.remove_unused_defs = true;
    opts.convert_shapes = true;
    opts.remove_metadata = true;
    opts.remove_title = false;
    opts.remove_desc = false;
    opts.remove_dupl_linear_gradients = true;
    opts.remove_dupl_radial_gradients = true;
    opts.remove_dupl_fe_gaussian_blur = true;
    opts.ungroup_groups = true;
    opts.ungroup_defs = true;
    opts.group_by_style = true;
    opts.merge_gradients = true;
    opts.regroup_gradient_stops = true;
    opts.remove_invalid_stops = true;
    opts.remove_invisible_elements = true;
    opts.resolve_use = true;
    opts.remove_version = true;
    opts.remove_unreferenced_ids = true;
    opts.trim_ids = true;
    opts.remove_text_attributes = true;
    opts.remove_unused_coordinates = true;
    opts.remove_default_attributes = true;
    opts.remove_needless_attributes = true;
    opts.remove_gradient_attributes = true;
    opts.apply_transform_to_gradients = true;
    opts.apply_transform_to_shapes = true;
    opts.paths_to_relative = true;
    opts.remove_unused_segments = true;
    opts.convert_segments = true;
    opts.apply_transform_to_paths = true;

    let write = svgcleaner::WriteOptions::default();
    cleaner::clean_doc(&mut doc, &opts, &write).map_err(|e| anyhow::anyhow!("svgcleaner: {e}"))?;

    let mut out = Vec::new();
    cleaner::write_buffer(&doc, &write, &mut out);
    Ok(out)
}
