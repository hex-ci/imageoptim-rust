# Contributing to ImageOptim-Rust

Thank you for your interest in contributing to ImageOptim-Rust!

## Repository Layout

- `crates/core/` — Core optimization pipeline, settings, SQLite results cache, safe file replacement, embedded SVGO runtime, and `build.rs` compilation of vendored C/C++ tools.
- `crates/app/` — Cross-platform `egui` / `eframe` desktop GUI, `--cli` headless runner, i18n translations, and macOS Cocoa / Action Extension native bindings (`crates/app/macos/`).
- `crates/pngquant/` — Self-contained vendored build of the `pngquant` 3.0.3 CLI binary.
- `third_party/src/` — Vendored C/C++/JS sources (`advpng`, `gifsicle`, `guetzli`, `jpegoptim`, `lcms2`, `libpng`, `libpng16`, `mozjpeg`, `pngcrush`, `pngquant`, `svgo`, `zlib`, `zopflipng`).
- `scripts/` — Packaging helpers and non-intrusive macOS UI interaction regression checks (`qa-smoke.sh`).

## Development Workflow

1. Check formatting and strict Clippy lints:
   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   ```
2. Build and run all tests (verifying all vendored tools compile cleanly):
   ```sh
   IMAGEOPTIM_REQUIRE_EXTERNAL_TOOLS=1 cargo test --workspace
   ```
3. On macOS, you can also run the automated UI interaction smoke suite:
   ```sh
   cargo build --workspace
   ./scripts/qa-smoke.sh target/debug/ImageOptim-Rust
   ```

## Guidelines

- **Zero external runtime dependencies**: New open-source features or codecs must compile from workspace crates or vendored source under `third_party/src/` without requiring external user-installed binaries.
- **Keep proprietary components out of the repo**: Do not commit closed-source binaries (such as `pngout`) or personal paths/credentials.
- **Licensing**: All contributions to this repository are licensed under **GPL-3.0-or-later** unless explicitly noted otherwise in an upstream-vendored subdirectory.
- **No silent panics**: Avoid `.unwrap()` / `.expect()` on runtime IO, mutex locks, or untrusted input paths; return actionable errors instead.
