use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, DynamicImage, Frame, ImageFormat, Luma, Rgb, Rgba, RgbaImage};
use imageoptim_core::cache::ResultsCache;
use imageoptim_core::engine;
use imageoptim_core::filetype::{FileKind, is_apng, png_has_chunk};
#[cfg(unix)]
use imageoptim_core::optimize::save_output;
use imageoptim_core::optimize::{JobOutcome, optimize_bytes, optimize_path};
use imageoptim_core::settings::Settings;
use img_parts::Bytes;
use img_parts::png::{Png, PngChunk};

fn ensure_tools_dir() {}

fn make_rgba_png(w: u32, h: u32) -> Vec<u8> {
    let mut img = RgbaImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = Rgba([
            ((x * 255) / w.max(1)) as u8,
            ((y * 255) / h.max(1)) as u8,
            (((x + y) * 180) / (w + h).max(1)) as u8,
            if (x + y) % 7 == 0 { 128 } else { 255 },
        ]);
    }
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

#[test]
fn golden_png_color_types_and_bit_depths() {
    ensure_tools_dir();
    let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();

    // 1. RGB8
    let mut rgb = image::RgbImage::new(96, 72);
    for (x, y, p) in rgb.enumerate_pixels_mut() {
        *p = Rgb([(x * 2) as u8, (y * 3) as u8, ((x + y) * 2) as u8]);
    }
    let mut rgb_bytes = Vec::new();
    rgb.write_to(&mut Cursor::new(&mut rgb_bytes), ImageFormat::Png)
        .unwrap();
    cases.push(("rgb8", rgb_bytes));

    // 2. RGBA8
    cases.push(("rgba8", make_rgba_png(96, 72)));

    // 3. Grayscale8
    let mut gray = image::GrayImage::new(96, 72);
    for (x, y, p) in gray.enumerate_pixels_mut() {
        *p = Luma([((x * 3 + y * 5) % 256) as u8]);
    }
    let mut gray_bytes = Vec::new();
    gray.write_to(&mut Cursor::new(&mut gray_bytes), ImageFormat::Png)
        .unwrap();
    cases.push(("gray8", gray_bytes));

    // 4. Indexed8 (palette PNG)
    let mut indexed_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut indexed_bytes, 64, 64);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        let mut palette = Vec::with_capacity(256 * 3);
        for i in 0..=255u8 {
            palette.extend_from_slice(&[i, 255u8.wrapping_sub(i), i.wrapping_mul(3)]);
        }
        enc.set_palette(palette);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().unwrap();
        let indices: Vec<u8> = (0..(64 * 64)).map(|i| (i % 32) as u8).collect();
        writer.write_image_data(&indices).unwrap();
    }
    cases.push(("indexed8", indexed_bytes));

    // 5. RGBA16 (16-bit per channel)
    let mut rgba16_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut rgba16_bytes, 48, 48);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Sixteen);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().unwrap();
        let mut raw = Vec::with_capacity(48 * 48 * 8);
        for y in 0..48u16 {
            for x in 0..48u16 {
                for ch in [x * 1024, y * 1024, (x + y) * 512, 65535] {
                    raw.extend_from_slice(&ch.to_be_bytes());
                }
            }
        }
        writer.write_image_data(&raw).unwrap();
    }
    cases.push(("rgba16", rgba16_bytes));

    let settings = Settings {
        level: 2,
        pngout_enabled: false,
        zopfli_enabled: false,
        ..Settings::default()
    };
    let cancel = AtomicBool::new(false);

    for (label, input) in cases {
        let orig_img = image::load_from_memory(&input).unwrap();
        let res = optimize_bytes(
            &input,
            FileKind::Png,
            &settings,
            &cancel,
            &mut |_| {},
            false,
        );
        assert!(res.error.is_none(), "{label} errored: {:?}", res.error);
        let out = res
            .output
            .unwrap_or_else(|| panic!("{label} did not shrink"));
        assert!(
            out.len() < input.len(),
            "{label}: {} -> {}",
            input.len(),
            out.len()
        );
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (orig_img.width(), orig_img.height()),
            "{label} dimensions changed"
        );
    }
}

#[test]
fn golden_pngquant_preserves_or_strips_metadata_chunks() {
    let base = make_rgba_png(128, 96);
    let mut png = Png::from_bytes(Bytes::from(base)).unwrap();
    // Insert pHYs (2835x2835 px/m) and tEXt right after IHDR (index 1).
    let phys_payload = [0, 0, 0x0B, 0x13, 0, 0, 0x0B, 0x13, 1u8];
    png.chunks_mut().insert(
        1,
        PngChunk::new(*b"pHYs", Bytes::copy_from_slice(&phys_payload)),
    );
    png.chunks_mut().insert(
        2,
        PngChunk::new(*b"tEXt", Bytes::from_static(b"Software\0ImageOptim-Rust")),
    );
    let mut with_meta = Vec::new();
    png.encoder().write_to(&mut with_meta).unwrap();
    assert!(png_has_chunk(&with_meta, b"pHYs"));
    assert!(png_has_chunk(&with_meta, b"tEXt"));

    // When `strip = false`, built-in pngquant must preserve both pHYs and tEXt.
    let kept = engine::pngquant_with_options(&with_meta, 65, 3, false).unwrap();
    assert!(png_has_chunk(&kept, b"pHYs"), "pHYs chunk was lost");
    assert!(png_has_chunk(&kept, b"tEXt"), "tEXt chunk was lost");
    image::load_from_memory(&kept).unwrap();

    // When `strip = true`, built-in pngquant must omit ancillary metadata chunks.
    let stripped = engine::pngquant_with_options(&with_meta, 65, 3, true).unwrap();
    assert!(
        !png_has_chunk(&stripped, b"pHYs"),
        "pHYs should be stripped"
    );
    assert!(
        !png_has_chunk(&stripped, b"tEXt"),
        "tEXt should be stripped"
    );
    image::load_from_memory(&stripped).unwrap();

    // Also exercise the full pipeline (invoking the compiled-from-source `pngquant` CLI
    // built from vendored `pngquant` + `libpng` + `zlib` + `lcms2` + `imagequant`):
    let require_cli = std::env::var_os("IMAGEOPTIM_REQUIRE_EXTERNAL_TOOLS").is_some();
    if require_cli || imageoptim_core::tools::binary_available("pngquant") {
        assert!(
            imageoptim_core::tools::binary_available("pngquant"),
            "pngquant CLI binary must be built when IMAGEOPTIM_REQUIRE_EXTERNAL_TOOLS is set"
        );
        let cancel = AtomicBool::new(false);
        let cli_keep = optimize_bytes(
            &with_meta,
            FileKind::Png,
            &Settings {
                lossy_enabled: true,
                png_quality: 65,
                strip_png_metadata: false,
                pngcrush_enabled: false,
                pngout_enabled: false,
                oxipng_enabled: false,
                advpng_enabled: false,
                zopfli_enabled: false,
                ..Settings::default()
            },
            &cancel,
            &mut |_| {},
            false,
        );
        let out_keep = cli_keep
            .output
            .expect("compiled pngquant CLI should shrink");
        assert!(
            png_has_chunk(&out_keep, b"pHYs"),
            "compiled pngquant CLI lost pHYs chunk"
        );
        assert!(
            png_has_chunk(&out_keep, b"tEXt"),
            "compiled pngquant CLI lost tEXt chunk"
        );

        let cli_strip = optimize_bytes(
            &with_meta,
            FileKind::Png,
            &Settings {
                lossy_enabled: true,
                png_quality: 65,
                strip_png_metadata: true,
                pngcrush_enabled: false,
                pngout_enabled: false,
                oxipng_enabled: false,
                advpng_enabled: false,
                zopfli_enabled: false,
                ..Settings::default()
            },
            &cancel,
            &mut |_| {},
            false,
        );
        let out_strip = cli_strip
            .output
            .expect("compiled pngquant CLI should shrink");
        assert!(!png_has_chunk(&out_strip, b"pHYs"));
        assert!(!png_has_chunk(&out_strip, b"tEXt"));
    }
}

#[test]
fn golden_apng_animation_survives_lossless_and_lossy_pipelines() {
    ensure_tools_dir();
    let mut apng = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut apng, 32, 32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        enc.set_animated(2, 0).unwrap();
        let mut writer = enc.write_header().unwrap();
        let frame0 = [255u8, 0, 0, 255].repeat(32 * 32);
        let frame1 = [0u8, 128, 255, 255].repeat(32 * 32);
        writer.write_image_data(&frame0).unwrap();
        writer.write_image_data(&frame1).unwrap();
    }
    assert!(is_apng(&apng));
    assert!(png_has_chunk(&apng, b"fcTL"));
    assert!(png_has_chunk(&apng, b"fdAT"));

    // Even with both `lossy_enabled: true` and `strip_png_metadata: true`,
    // APNG animation chunks must never be destroyed!
    let settings = Settings {
        level: 2,
        lossy_enabled: true,
        strip_png_metadata: true,
        pngcrush_enabled: true,
        pngout_enabled: true,
        advpng_enabled: true,
        oxipng_enabled: true,
        zopfli_enabled: true,
        ..Settings::default()
    };
    let mut ran_tools = Vec::new();
    let cancel = AtomicBool::new(false);
    let res = optimize_bytes(
        &apng,
        FileKind::Png,
        &settings,
        &cancel,
        &mut |t| ran_tools.push(t.to_string()),
        false,
    );
    assert!(res.error.is_none(), "APNG pipeline error: {:?}", res.error);
    // Single-frame tools that strip acTL/fdAT must be skipped automatically.
    for forbidden in ["pngquant", "Pngcrush", "PNGOUT", "AdvPNG"] {
        assert!(
            !ran_tools.iter().any(|t| t == forbidden),
            "{forbidden} should not run on APNG"
        );
    }
    if let Some(out) = res.output {
        assert!(is_apng(&out), "APNG acTL chunk lost after optimization");
        assert!(png_has_chunk(&out, b"fcTL"), "APNG fcTL chunk lost");
        assert!(png_has_chunk(&out, b"fdAT"), "APNG fdAT chunk lost");
    }
}

#[test]
fn golden_animated_gif_preserves_frame_count() {
    ensure_tools_dir();
    let mut gif_bytes = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut gif_bytes);
        enc.set_repeat(Repeat::Infinite).unwrap();
        for step in 0..3u8 {
            let mut img = RgbaImage::new(48, 48);
            for (x, y, p) in img.enumerate_pixels_mut() {
                *p = Rgba([
                    step.wrapping_mul(80).wrapping_add(x as u8),
                    y as u8 * 4,
                    180,
                    255,
                ]);
            }
            let frame = Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(100, 1));
            enc.encode_frame(frame).unwrap();
        }
    }

    for lossy in [false, true] {
        let settings = Settings {
            lossy_enabled: lossy,
            gif_quality: 80,
            gifsicle_enabled: true,
            ..Settings::default()
        };
        let cancel = AtomicBool::new(false);
        let res = optimize_bytes(
            &gif_bytes,
            FileKind::Gif,
            &settings,
            &cancel,
            &mut |_| {},
            false,
        );
        assert!(res.error.is_none(), "GIF lossy={lossy}: {:?}", res.error);
        if let Some(out) = res.output {
            assert!(out.len() <= gif_bytes.len());
            let dec = image::codecs::gif::GifDecoder::new(Cursor::new(&out)).unwrap();
            use image::AnimationDecoder as _;
            let frames = dec.into_frames().collect_frames().unwrap();
            assert_eq!(frames.len(), 3, "animated GIF frame count changed");
        }
    }
}

#[test]
fn golden_svg_lossless_and_lossy_pipelines() {
    let svg = br##"<?xml version="1.0" encoding="UTF-8"?>
<!-- Generator: Adobe Illustrator 27.0.0 -->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
  <metadata>Created for regression suite</metadata>
  <defs>
    <linearGradient id="unused_grad">
      <stop offset="0%" stop-color="#ffffff"/>
      <stop offset="100%" stop-color="#000000"/>
    </linearGradient>
  </defs>
  <g id="layer1">
    <path d="M 10.123456 20.654321 L 180.987654 20.654321 L 95.500000 175.250000 Z"
          fill="#ff0000" stroke="#000000" stroke-width="2.0000"/>
  </g>
</svg>"##;

    for lossy in [false, true] {
        let settings = Settings {
            lossy_enabled: lossy,
            svgo_enabled: true,
            svgcleaner_enabled: true,
            ..Settings::default()
        };
        let cancel = AtomicBool::new(false);
        let res = optimize_bytes(svg, FileKind::Svg, &settings, &cancel, &mut |_| {}, false);
        assert!(res.error.is_none());
        let out = res.output.expect("SVG should shrink");
        assert!(out.len() < svg.len());
        let text = std::str::from_utf8(&out).expect("valid UTF-8 SVG");
        assert!(text.contains("<svg"));
    }
}

#[cfg(unix)]
#[test]
fn edge_case_symlink_target_is_optimized_without_breaking_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let real_path = dir.path().join("real.png");
    let sym_path = dir.path().join("alias.png");
    let original = std::fs::read("tests/fixtures/unoptimized.png").unwrap();
    std::fs::write(&real_path, &original).unwrap();
    std::os::unix::fs::symlink(&real_path, &sym_path).unwrap();

    // `preserve_permissions: false` exercises the temp-file + atomic rename path,
    // which previously replaced `sym_path` with a regular file if symlinks were not resolved.
    let settings = Settings {
        preserve_permissions: false,
        ..Settings::default()
    };
    let optimized = engine::oxipng(&original, 2, true, false, None).unwrap();
    save_output(&sym_path, &optimized, &settings).unwrap();

    assert!(
        std::fs::symlink_metadata(&sym_path)
            .unwrap()
            .file_type()
            .is_symlink(),
        "symlink must remain a symbolic link after optimization"
    );
    assert_eq!(std::fs::read(&real_path).unwrap(), optimized);
    assert_eq!(std::fs::read(&sym_path).unwrap(), optimized);
}

#[test]
fn edge_case_corrupt_and_tiny_files_fail_safely_without_touching_disk() {
    let dir = tempfile::tempdir().unwrap();
    let settings = Settings::default();
    let cache = ResultsCache::disabled();
    let cancel = AtomicBool::new(false);

    let cases: [(&str, &[u8]); 3] = [
        ("tiny.png", b"tiny-under-30b"),
        (
            "corrupt.png",
            b"\x89PNG\r\n\x1a\ncorrupt_png_payload_exceeding_thirty_bytes_12345",
        ),
        (
            "corrupt.jpg",
            b"\xff\xd8\xff\xe0corrupt_jpeg_payload_exceeding_thirty_bytes_12345",
        ),
    ];

    for (name, bytes) in cases {
        let p = dir.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        let outcome = optimize_path(&p, &settings, &cache, &cancel, &mut |_| {}, false);
        assert!(
            matches!(outcome, JobOutcome::Failed(_)),
            "expected Failed for {name}, got {outcome:?}"
        );
        assert_eq!(
            std::fs::read(&p).unwrap(),
            bytes,
            "{name} was modified on failure"
        );
    }
}

#[cfg(unix)]
#[test]
fn edge_case_read_only_directory_and_file_fail_cleanly() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("readonly");
    std::fs::create_dir_all(&sub).unwrap();
    let file_path = sub.join("locked.png");
    let original = std::fs::read("tests/fixtures/unoptimized.png").unwrap();
    std::fs::write(&file_path, &original).unwrap();
    let optimized = engine::oxipng(&original, 2, true, false, None).unwrap();

    std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o444)).unwrap();
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o555)).unwrap();

    let res_atomic = save_output(
        &file_path,
        &optimized,
        &Settings {
            preserve_permissions: false,
            ..Settings::default()
        },
    );
    let res_inplace = save_output(
        &file_path,
        &optimized,
        &Settings {
            preserve_permissions: true,
            ..Settings::default()
        },
    );

    // Restore permissions before asserting so tempdir cleanup always succeeds.
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o644)).unwrap();

    assert!(res_atomic.is_err());
    assert!(res_inplace.is_err());
    assert_eq!(std::fs::read(&file_path).unwrap(), original);
}

#[test]
fn instant_cancellation_aborts_active_tool_and_leaves_file_untouched() {
    ensure_tools_dir();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("heavy.png");

    // Generate a high-entropy 512x512 RGBA image so external tools (`pngcrush -brute` /
    // `zopflipng`) spend multiple seconds unless killed by `cancel`.
    let mut img = RgbaImage::new(512, 512);
    let mut state = 0x1234_5678u32;
    for (_, _, p) in img.enumerate_pixels_mut() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *p = Rgba([
            (state >> 24) as u8,
            (state >> 16) as u8,
            (state >> 8) as u8,
            255,
        ]);
    }
    let mut raw_png = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut raw_png), ImageFormat::Png)
        .unwrap();
    std::fs::write(&path, &raw_png).unwrap();

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = cancel.clone();
    let path_worker = path.clone();

    let start = Instant::now();
    let handle = std::thread::spawn(move || {
        let settings = Settings {
            level: 6,
            oxipng_enabled: false,
            pngcrush_enabled: true,
            pngout_enabled: false,
            zopfli_enabled: true,
            ..Settings::default()
        };
        let cache = ResultsCache::disabled();
        optimize_path(
            &path_worker,
            &settings,
            &cache,
            &cancel_worker,
            &mut |_| {},
            false,
        )
    });

    std::thread::sleep(Duration::from_millis(80));
    cancel.store(true, Ordering::Relaxed);
    let outcome = handle.join().unwrap();
    let elapsed = start.elapsed();

    assert!(
        matches!(outcome, JobOutcome::Cancelled),
        "expected Cancelled, got {outcome:?}"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "cancellation took too long: {elapsed:?}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), raw_png);
}

#[test]
fn sqlite_results_cache_wal_and_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("Results.db");
    let settings = Settings::default();
    let cache = ResultsCache::open_at(&db_path, &settings);

    let h1 = cache.file_hash(b"sample-image-bytes", false);
    let h1_rerun = cache.file_hash(b"sample-image-bytes", true);
    assert_ne!(h1, h1_rerun);
    assert!(!cache.contains(&h1));

    cache.remember_unoptimizable(&h1, 1234);
    assert!(cache.contains(&h1));
    assert!(!cache.contains(&h1_rerun));
}
