use std::io::Cursor;

use image::{ImageFormat, Rgba, RgbaImage};
use imageoptim_core::engine;
use imageoptim_core::filetype::{FileKind, ImageFile};
use imageoptim_core::optimize_bytes;
use imageoptim_core::settings::Settings;

/// Source-compiled binaries in `target/<profile>/tools` and `target/<profile>/pngquant`
/// are discovered automatically relative to the test runner executable.
fn ensure_tools_dir() {}

fn gradient_png() -> Vec<u8> {
    let mut img = RgbaImage::new(320, 240);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let r = ((x * 255) / 320) as u8;
        let g = ((y * 255) / 240) as u8;
        let b = (((x + y) * 127) / 560) as u8;
        *p = Rgba([r, g, b, 255]);
    }
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

#[test]
fn detects_formats_by_magic() {
    let png = gradient_png();
    assert_eq!(FileKind::detect(&png, "x.bin".as_ref()), FileKind::Png);
    assert_eq!(
        FileKind::detect(b"\xff\xd8\xff\xe0\x00\x10", "x.bin".as_ref()),
        FileKind::Jpeg
    );
    assert_eq!(FileKind::detect(b"GIF89a", "x.bin".as_ref()), FileKind::Gif);
    assert_eq!(FileKind::detect(b"<svg x", "x.bin".as_ref()), FileKind::Svg);
    let f = ImageFile::read("tests/fixtures/unoptimized.png".as_ref()).unwrap();
    assert_eq!(f.kind, FileKind::Png);
    assert_eq!(f.path.to_string_lossy(), "tests/fixtures/unoptimized.png");
}

#[test]
fn oxipng_shrinks_png() {
    let input = std::fs::read("tests/fixtures/unoptimized.png").unwrap();
    let out = engine::oxipng(&input, 4, true, false, None).unwrap();
    assert!(out.len() < input.len(), "{} -> {}", input.len(), out.len());
    assert_eq!(FileKind::detect(&out, "x.png".as_ref()), FileKind::Png);
}

#[test]
fn zopfli_shrinks_png() {
    let input = gradient_png();
    let out = engine::oxipng(&input, 2, false, true, None).unwrap();
    assert!(out.len() < input.len(), "{} -> {}", input.len(), out.len());
}

#[test]
fn pngquant_produces_palette_png() {
    let input = gradient_png();
    let out = engine::pngquant(&input, 40, 4).unwrap();
    assert!(out.len() < input.len(), "{} -> {}", input.len(), out.len());
    let img = image::load_from_memory(&out).unwrap();
    assert_eq!(img.width(), 320);
    assert_eq!(img.height(), 240);
}

fn sample_jpeg() -> Vec<u8> {
    let mut img = image::RgbImage::new(320, 240);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = image::Rgb([
            ((x * 7) % 256) as u8,
            ((y * 11) % 256) as u8,
            (((x * y) / 7) % 256) as u8,
        ]);
    }
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Jpeg)
        .unwrap();
    out
}

#[test]
fn jpegtran_optimizes_without_size_growth() {
    let input = sample_jpeg();
    let out = engine::jpegtran(&input, true).unwrap();
    assert!(out.len() <= input.len(), "{} -> {}", input.len(), out.len());
    assert_eq!(FileKind::detect(&out, "x.jpg".as_ref()), FileKind::Jpeg);
    let src = image::load_from_memory(&input).unwrap();
    let dst = image::load_from_memory(&out).unwrap();
    assert_eq!((src.width(), src.height()), (dst.width(), dst.height()));
}

#[test]
fn jpegoptim_shrinks_lossy() {
    let input = sample_jpeg();
    let out = engine::jpegoptim(&input, 70, true).unwrap();
    assert!(out.len() < input.len(), "{} -> {}", input.len(), out.len());
    assert_eq!(FileKind::detect(&out, "x.jpg".as_ref()), FileKind::Jpeg);
    let src = image::load_from_memory(&input).unwrap();
    let dst = image::load_from_memory(&out).unwrap();
    assert_eq!((src.width(), src.height()), (dst.width(), dst.height()));
}

#[test]
fn svgcleaner_shrinks_svg() {
    let input = br##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" version="1.1">
  <!-- a comment -->
  <metadata>hello</metadata>
  <defs><linearGradient id="unused"><stop offset="0" stop-color="#fff"/></linearGradient></defs>
  <rect x="10" y="10" width="80" height="80" fill="#ff0000" stroke="none" stroke-width="1"/>
</svg>"##;
    let out = engine::svgcleaner(input).unwrap();
    assert!(out.len() < input.len(), "{} -> {}", input.len(), out.len());
}

#[test]
fn pipeline_picks_smallest_candidate() {
    let input = gradient_png();
    let settings = Settings {
        level: 2,
        zopfli_enabled: true,
        ..Settings::default()
    };
    let mut tools = Vec::new();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let result = optimize_bytes(
        &input,
        FileKind::Png,
        &settings,
        &cancel,
        &mut |t| tools.push(t.to_string()),
        false,
    );
    assert!(result.error.is_none());
    let out = result.output.expect("should optimize");
    assert!(out.len() < input.len());
    assert!(result.best_tool.is_some());
    assert!(tools.contains(&"OxiPNG".to_string()));
}

#[test]
fn save_preserves_hardlink_and_revert_restores() {
    use imageoptim_core::optimize::{revert, save_output};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hard.png");
    let original = std::fs::read("tests/fixtures/unoptimized.png").unwrap();
    std::fs::write(&path, &original).unwrap();
    let link = dir.path().join("link.png");
    std::fs::hard_link(&path, &link).unwrap();

    let settings = Settings {
        preserve_permissions: true,
        ..Settings::default()
    };
    let optimized = engine::oxipng(&original, 4, false, false, None).unwrap();
    assert!(optimized.len() < original.len());
    let backup = save_output(&path, &optimized, &settings).unwrap().unwrap();

    // The hard link must observe the new content: the inode was written in place.
    assert_eq!(std::fs::read(&link).unwrap(), optimized);
    assert_eq!(std::fs::read(&path).unwrap(), optimized);

    revert(&path, &backup).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read(&link).unwrap(), original);
}

#[test]
fn jpeg_metadata_is_preserved_or_stripped() {
    use img_parts::jpeg::{Jpeg, JpegSegment, markers};

    let input = sample_jpeg();
    let mut jpeg = Jpeg::from_bytes(input.clone().into()).unwrap();
    jpeg.segments_mut().insert(
        0,
        JpegSegment::new_with_contents(markers::COM, b"hello-metadata".as_slice().into()),
    );
    let mut with_meta = Vec::new();
    jpeg.encoder().write_to(&mut with_meta).unwrap();

    let kept = engine::jpegoptim(&with_meta, 70, false).unwrap();
    let parsed = Jpeg::from_bytes(kept.into()).unwrap();
    assert!(parsed.segment_by_marker(markers::COM).is_some());

    let stripped = engine::jpegoptim(&with_meta, 70, true).unwrap();
    let parsed = Jpeg::from_bytes(stripped.into()).unwrap();
    assert!(parsed.segment_by_marker(markers::COM).is_none());
}

#[test]
fn end_to_end_job_saves_and_caches() {
    use imageoptim_core::cache::ResultsCache;
    use imageoptim_core::optimize::{JobOutcome, optimize_path};

    ensure_tools_dir();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.png");
    let original = std::fs::read("tests/fixtures/unoptimized.png").unwrap();
    std::fs::write(&path, &original).unwrap();

    let settings = Settings::default();
    let cache = ResultsCache::open(&settings);
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let outcome = optimize_path(&path, &settings, &cache, &cancel, &mut |_| {}, false);
    let JobOutcome::Optimized(saved) = outcome else {
        panic!("expected optimized, got {outcome:?}");
    };
    assert!(saved.optimized_size < saved.original_size);
    assert!(saved.revert.is_some());
    let after = std::fs::read(&path).unwrap();
    assert!(after.len() < original.len());
    image::load_from_memory(&after).unwrap();
}

#[test]
fn serial_pipeline_runs_oxipng_before_zopfli() {
    use imageoptim_core::optimize::optimize_bytes;
    let input = gradient_png();
    let settings = Settings {
        level: 2,
        oxipng_enabled: true,
        zopfli_enabled: true,
        ..Settings::default()
    };
    let mut seen = Vec::new();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let result = optimize_bytes(
        &input,
        FileKind::Png,
        &settings,
        &cancel,
        &mut |t| seen.push(t.to_string()),
        false,
    );
    assert!(result.error.is_none());
    let oxi = seen.iter().position(|t| t == "OxiPNG").expect("OxiPNG ran");
    let zop = seen.iter().position(|t| t == "Zopfli").expect("Zopfli ran");
    assert!(oxi < zop);
    let out = result.output.expect("optimized");
    assert!(out.len() < input.len());
}

#[test]
fn missing_external_tools_report_failure() {
    use imageoptim_core::optimize::optimize_bytes;
    use imageoptim_core::tools::Tool;
    if Tool::PngCrush.is_available() || Tool::PngOut.is_available() || Tool::AdvPng.is_available() {
        return; // exercise only the unavailable path
    }
    let input = gradient_png();
    let settings = Settings {
        level: 3,
        oxipng_enabled: false,
        zopfli_enabled: false,
        pngcrush_enabled: true,
        pngout_enabled: true,
        advpng_enabled: true,
        ..Settings::default()
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let result = optimize_bytes(
        &input,
        FileKind::Png,
        &settings,
        &cancel,
        &mut |_| {},
        false,
    );
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|e| e.ends_with("failed to start"))
    );
    assert!(result.best_tool.is_none());
}

#[test]
fn lossy_png_pipeline_quantizes() {
    use imageoptim_core::optimize::optimize_bytes;
    ensure_tools_dir();
    let input = gradient_png();
    let settings = Settings {
        lossy_enabled: true,
        png_quality: 60,
        level: 2,
        oxipng_enabled: true,
        zopfli_enabled: false,
        ..Settings::default()
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut seen = Vec::new();
    let result = optimize_bytes(
        &input,
        FileKind::Png,
        &settings,
        &cancel,
        &mut |t| seen.push(t.to_string()),
        false,
    );
    assert!(result.error.is_none());
    assert!(seen.iter().any(|t| t == "pngquant"));
    let out = result.output.expect("lossy output");
    assert!(out.len() < input.len());
    image::load_from_memory(&out).unwrap();
}

#[test]
fn lossy_jpeg_pipeline_respects_five_percent_rule() {
    use imageoptim_core::optimize::optimize_bytes;
    let input = sample_jpeg();
    let settings = Settings {
        lossy_enabled: true,
        jpeg_quality: 70,
        ..Settings::default()
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut seen = Vec::new();
    let result = optimize_bytes(
        &input,
        FileKind::Jpeg,
        &settings,
        &cancel,
        &mut |t| seen.push(t.to_string()),
        false,
    );
    assert!(result.error.is_none());
    assert!(seen.iter().any(|t| t == "JpegOptim"));
    // sample_jpeg is tiny; the 5% rule may legitimately reject the result.
    if let Some(out) = result.output {
        assert!(out.len() * 100 <= input.len() * 95);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn save_restores_birthtime_when_replacing_inode() {
    use imageoptim_core::optimize::save_output;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("birth.png");
    let original = std::fs::read("tests/fixtures/unoptimized.png").unwrap();
    std::fs::write(&path, &original).unwrap();
    let before = std::fs::metadata(&path).unwrap().created().unwrap();

    let settings = Settings {
        preserve_permissions: false,
        preserve_dates: true,
        ..Settings::default()
    };
    let optimized = engine::oxipng(&original, 2, false, false, None).unwrap();
    let _ = save_output(&path, &optimized, &settings).unwrap();
    let after = std::fs::metadata(&path).unwrap().created().unwrap();
    assert_eq!(before, after);
}

#[cfg(unix)]
#[test]
fn save_preserves_custom_xattrs_and_strips_unwanted() {
    use imageoptim_core::optimize::save_output;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("xattr.png");
    let original = std::fs::read("tests/fixtures/unoptimized.png").unwrap();
    std::fs::write(&path, &original).unwrap();
    xattr::set(&path, "user.imageoptim", b"custom-keep").unwrap();
    let has_apple_xattr = xattr::set(
        &path,
        "com.apple.metadata:kMDItemWhereFroms",
        b"https://example.com",
    )
    .is_ok();

    let settings = Settings {
        preserve_permissions: false,
        preserve_dates: true,
        ..Settings::default()
    };
    let optimized = engine::oxipng(&original, 2, false, false, None).unwrap();
    let _ = save_output(&path, &optimized, &settings).unwrap();
    assert_eq!(
        xattr::get(&path, "user.imageoptim").unwrap().as_deref(),
        Some(b"custom-keep".as_slice())
    );
    if has_apple_xattr {
        assert_eq!(
            xattr::get(&path, "com.apple.metadata:kMDItemWhereFroms").unwrap(),
            None
        );
    }

    // Also verify in-place overwrite (`preserve_permissions: true`) strips unwanted xattrs.
    if has_apple_xattr {
        xattr::set(
            &path,
            "com.apple.metadata:kMDItemWhereFroms",
            b"https://example.com",
        )
        .unwrap();
    }
    let inplace = Settings {
        preserve_permissions: true,
        ..Settings::default()
    };
    let _ = save_output(&path, &optimized, &inplace).unwrap();
    if has_apple_xattr {
        assert_eq!(
            xattr::get(&path, "com.apple.metadata:kMDItemWhereFroms").unwrap(),
            None
        );
    }
    assert_eq!(
        xattr::get(&path, "user.imageoptim").unwrap().as_deref(),
        Some(b"custom-keep".as_slice())
    );
}
