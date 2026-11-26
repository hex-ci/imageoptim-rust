<div align="center">

# ImageOptim-Rust

**Cross-platform lossless & lossy image optimizer in Rust, inspired by ImageOptim.**

English | [简体中文](README.zh-CN.md)

[![CI](https://github.com/hex-ci/imageoptim-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/hex-ci/imageoptim-rust/actions/workflows/ci.yml)
[![Release](https://github.com/hex-ci/imageoptim-rust/actions/workflows/release.yml/badge.svg)](https://github.com/hex-ci/imageoptim-rust/actions/workflows/release.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)

</div>

> [!NOTE]
> **Unofficial Port**: ImageOptim-Rust is an independent, unofficial cross-platform Rust port inspired by [ImageOptim](https://imageoptim.com/) by Kornel Lesiński and contributors. It is not affiliated with or endorsed by the upstream ImageOptim project.

---

## Features

- **All open-source optimizers built from vendored source**: Zero dependency on Homebrew, system codec packages, Node.js, or external `PATH` tools. Every open-source engine compiles directly from [`third_party/src/`](third_party/src) and Rust workspace crates.
- **Cross-platform Desktop GUI + Headless CLI**:
  - macOS (`arm64` & `x86_64`), Linux (`x86_64`), and Windows (`x64`).
  - Full GUI with drag-and-drop, column customization & persistence, Undo/Redo, Trash revert, comparison preview, and SQLite (`Results.db`) optimization cache.
  - Pure headless `--cli` / `--headless` mode with zero window-server dependency for CI, SSH, and batch automation.
- **Safe Atomic File Replacement**: Preserves permissions, hardlinks, macOS Finder tags/ACLs/birthtime, and non-cache extended attributes; reversible via Trash or scratch backups.
- **Lossless & Quality-Bounded Pipelines**: Always compares candidates and picks the smallest output; enforces the upstream lossy quality pipeline rules (`pngquant`, `jpegoptim`, `guetzli`, `svgo`, `svgcleaner`).
- **Platform Integrations**:
  - **macOS**: `.app` bundle, AppleScript (`do queuecount command`), Finder Services (`Optimize with ImageOptim-Rust`), Finder **Action / Markup Extension** (`ImageOptimize.appex`), Quick Look comparison, native file drag-out.
  - **Linux**: AppImage / `.tar.gz` / `.deb` package, `imageoptim-rust` CLI symlink, `.desktop` menu actions, optional Nautilus right-click installer.
  - **Windows**: Portable `.zip` & NSIS setup installer, Explorer right-click menu scripts (`HKCU`, no UAC required).
- **Bilingual UI**: Automatic English and Simplified Chinese localization across menus, dialogs, status messages, and bundled HTML help.

---

## Bundled Optimization Engines

| Format | Tool | Build Source | License |
| --- | --- | --- | --- |
| PNG | **OxiPNG** (`10.2`) | Rust crate | MIT |
| PNG | **ZopfliPNG** | Rust crate + `third_party/src/zopflipng` | Apache-2.0 |
| PNG | **pngquant** (`3.0.3`) / `libimagequant` | `crates/pngquant` + `third_party/src/pngquant` | GPL-3.0-or-later |
| PNG | **AdvPNG** | `third_party/src/advpng` | GPL-2.0-or-later |
| PNG | **PNGCrush** | `third_party/src/pngcrush` | Zlib-like permissive |
| PNG | **PNGOUT** *(optional, not bundled)* | User-supplied proprietary binary | Freeware (proprietary) |
| JPEG | **MozJPEG** (`jpegtran`) | `turbojpeg` crate + `third_party/src/mozjpeg` | BSD-3-Clause / IJG / Zlib |
| JPEG | **Jpegoptim** | `third_party/src/jpegoptim` | GPL-3.0-or-later |
| JPEG | **Guetzli** | `third_party/src/guetzli` | Apache-2.0 |
| GIF | **Gifsicle** | `third_party/src/gifsicle` | GPL-2.0 |
| SVG | **SVGO** | Embedded `rquickjs` runtime (`crates/core/src/svgo`) | MIT |
| SVG | **svgcleaner** | Rust crate (`svgcleaner`) | GPL-2.0-or-later |

> [!IMPORTANT]
> Closed-source **PNGOUT** is **not** redistributed in source archives or GitHub Releases. If you want PNGOUT in your local setup, download it from the original author's website and either place it next to the bundled tools (`Contents/Resources/tools/pngout` on macOS, `tools/pngout` next to the binary on Linux/Windows) or set `IMAGEOPTIM_PNGOUT=/path/to/pngout` (or `IMAGEOPTIM_TOOLS_DIR`).

---

## Installation

Download prebuilt packages for your platform from [GitHub Releases](https://github.com/hex-ci/imageoptim-rust/releases):

- **macOS**: Separate builds for Apple Silicon (`macos-arm64.dmg` / `.zip`) and Intel (`macos-x86_64.dmg` / `.zip`).
- **Linux (`x86_64`)**: `.deb`, `.AppImage`, or portable `.tar.gz` (installs GUI + `/usr/bin/imageoptim-rust`).
- **Windows (`x64`)**: NSIS installer (`-setup.exe`) or portable `.zip`.

---

## CLI Usage

```sh
# Open GUI, optimize files/folders, and quit when done
ImageOptim-Rust path/to/images/

# Linux package/symlink uses lowercase CLI name:
imageoptim-rust --cli path/to/images/

# Headless CLI options
ImageOptim-Rust --cli [OPTIONS] <PATHS...>
```

Options verified against [`crates/app/src/cli.rs`](crates/app/src/cli.rs):

| Option | Description |
| --- | --- |
| `--cli`, `--headless` | Run cleanly without a GUI window (scripts, CI, SSH, extensions) |
| `--lossy` | Enable lossy minification for this run |
| `--level <0..6>` | Override optimization effort (`0` Fast .. `6` Insane) |
| `--settings <FILE>` | Load preferences from an explicit `settings.json` path |
| `--no-backup` | Overwrite files in place without creating a Trash/scratch backup |
| `-h`, `--help` | Print CLI help message |
| `-V`, `--version` | Print version information |

---

## Building from Source

### Prerequisites

- **Rust**: Stable Rust toolchain `>= 1.99` (`cargo`, `rustc`)
- **C / C++ toolchain & CMake**:
  - **macOS**: Xcode Command Line Tools (`clang`) + `cmake` (and `nasm` recommended for SIMD codecs)
  - **Linux (Debian/Ubuntu)**:
    ```sh
    sudo apt-get update
    sudo apt-get install -y \
      build-essential cmake nasm pkg-config \
      libasound2-dev libxkbcommon-dev libwayland-dev libudev-dev \
      libgtk-3-dev libx11-dev dpkg
    ```
  - **Windows**: Visual Studio Build Tools (MSVC C++ workload) + CMake (`nasm` optional)

### Build & Test

```sh
cargo build --workspace --release

# Run full test suite (requiring all vendored tools built from source)
IMAGEOPTIM_REQUIRE_EXTERNAL_TOOLS=1 cargo test --workspace
```

### Packaging Installers

```sh
# macOS (.app, .zip, .dmg)
./scripts/package-macos.sh release                 # host architecture
./scripts/package-macos.sh release x86_64-apple-darwin

# Linux (.tar.gz, .deb, and .AppImage if appimagetool is installed)
./scripts/package-linux.sh release

# Windows (.zip, plus -setup.exe if NSIS makensis is installed)
pwsh ./scripts/package-windows.ps1 -Profile release
```

---

## Contributing & License

- Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md).
- For security reports, please follow [SECURITY.md](SECURITY.md).
- Licensed under the **GNU General Public License v3.0 or later** ([`LICENSE`](LICENSE)).
- Full third-party copyright notices and component attributions are documented in [`NOTICE.md`](NOTICE.md).
