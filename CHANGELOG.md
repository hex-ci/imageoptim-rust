# Changelog

All notable changes to **ImageOptim-Rust** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-11-27

### Fixed
- Persist `ui-state.json` window geometry reliably across launches: create missing configuration directories on write, flush viewport state before macOS early window/auto-quit exits and in `on_exit`, preserve saved window rects when minimized, and schedule a trailing flush after drag/resize stops.

## [0.1.0] - 2026-11-27

### Added
- Initial public open-source release of **ImageOptim-Rust** (`hex-ci/imageoptim-rust`) under `GPL-3.0-or-later`.
- Self-contained vendored source builds for OxiPNG, ZopfliPNG, pngquant `3.0.3`, AdvPNG, PNGCrush, MozJPEG, Jpegoptim, Guetzli, Gifsicle, SVGO (`rquickjs`), and svgcleaner.
- Cross-platform `egui` desktop GUI with English / Simplified Chinese localization, drag-and-drop, column & window state persistence, Trash revert, Undo/Redo, quick preview, and SQLite results cache.
- Pure headless `--cli` / `--headless` mode for automation, CI, and extensions.
- macOS Finder Services, AppleScript support, Quick Look comparison, and `ImageOptimize.appex` extension, plus Linux `.tar.gz`/`.deb`/`.AppImage` and Windows `.zip`/NSIS installers.
