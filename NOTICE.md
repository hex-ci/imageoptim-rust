# Legal Notices & Third-Party Licenses

**ImageOptim-Rust** is an unofficial, independent, cross-platform Rust reimplementation of [ImageOptim](https://imageoptim.com) (`1.9.3`), created by Hex and contributors.
It is **not affiliated with, endorsed by, or sponsored by** Kornel Lesiński or `imageoptim.com`.

Because the complete workspace statically links `pngquant` / `libimagequant` (licensed under GNU GPL v3.0 or later) and incorporates components under GPL v2.0 or later, the combined project is licensed and distributed under the **GNU General Public License v3.0 or later** (`GPL-3.0-or-later`). See [`LICENSE`](LICENSE).

---

## Upstream Attribution (ImageOptim)

- UI layout, icon assets (`crates/app/assets/*.png`, `crates/app/assets/ImageOptim.icns`), help HTML (`crates/app/assets/help/`), AppleScript dictionary (`ImageOptimVerbs.sdef`), localization strings (`i18n.json`), and optimizer heuristics are derived from or inspired by [ImageOptim](https://github.com/ImageOptim/ImageOptim) by **Kornel Lesiński** and contributors (`GPL-2.0-or-later`).
- All upstream copyright notices are preserved in this notice and inside the application About dialog.

---

## Vendored Source & Embedded Optimization Engines (`third_party/src/`)

Every open-source optimizer and compression library in this project is compiled directly from vendored source code or Cargo dependencies with zero reliance on system package managers or external `PATH` tools:

| Component | Purpose | License | Upstream Project |
|---|---|---|---|
| **OxiPNG** (`oxipng` crate) | Lossless PNG optimization & Zopfli support | MIT | https://github.com/oxipng/oxipng |
| **pngquant 3.0.3 & libimagequant 4.x** | Lossy palette PNG quantization (`--cli` & library) | GPL-3.0-or-later | https://pngquant.org / https://github.com/ImageOptim/libimagequant |
| **zopflipng / zopfli** | High-ratio Deflate PNG recompression | Apache-2.0 | https://github.com/google/zopfli |
| **AdvPNG** (`advancecomp 1.15`) | 7-Zip Deflate PNG recompression | GPL-2.0-or-later | http://www.advancemame.it/doc-advpng |
| **PNGCrush** (`1.8.10` + patches) | PNG filter/zlib strategy search & chunk reduction | zlib/libpng-style Permissive | http://pmt.sourceforge.net/pngcrush/ |
| **MozJPEG** (`jpegtran`) & **turbojpeg** | Lossless progressive JPEG transform & DCT | IJG / BSD-3-Clause / zlib | https://github.com/mozilla/mozjpeg / https://libjpeg-turbo.org |
| **Jpegoptim** | Lossless/lossy JPEG optimization & metadata control | GPL-2.0-or-later | https://github.com/tjko/jpegoptim |
| **Guetzli** | Perceptual high-quality lossy JPEG encoder | Apache-2.0 | https://github.com/google/guetzli |
| **Gifsicle** (`giflossy`) | Lossless & lossy GIF optimization | GPL-2.0 | https://www.lcdf.org/gifsicle/ / https://github.com/kornelski/giflossy |
| **SVGO** (`1.3.2`) + **QuickJS** (`rquickjs`) | Embedded JS SVG optimization (zero Node.js needed) | MIT (SVGO & QuickJS) | https://github.com/svg/svgo / https://bellard.org/quickjs/ |
| **svgcleaner** (`0.9.5`) | High-speed Rust SVG cleanup | GPL-2.0-or-later | https://github.com/RazrFalcon/svgcleaner |
| **Little CMS** (`lcms2 2.16`) | ICC color profile transformation for `pngquant` | MIT | https://www.littlecms.com |
| **libpng** (`1.6.44` & `1.4.5-optipng`) | PNG decoding/encoding for C/C++ tools | libpng License v2 | http://www.libpng.org/pub/png/libpng.html |
| **zlib** (`1.2.8` / `optipng`) | DEFLATE compression library | zlib License | https://zlib.net |

Individual upstream `LICENSE`, `COPYING`, and `COPYRIGHT` files are preserved verbatim inside `third_party/src/<component>/`.

---

## Core Rust GUI & Infrastructure Crates

- **egui / eframe / egui_extras** — MIT OR Apache-2.0 — https://github.com/emilk/egui
- **rusqlite** (bundled SQLite) — MIT / Public Domain — https://github.com/rusqlite/rusqlite
- **image / png / img-parts / zopfli** — MIT OR Apache-2.0

---

## Proprietary Optional Tool: PNGOUT

**PNGOUT** (by Ken Silverman / Ardfry Imaging, LLC) is proprietary freeware, not open-source software, and is **not included** in this repository or in prebuilt GitHub Releases.
If a user independently obtains the `pngout` / `pngout.exe` binary for personal use and places it next to the application's `tools/` directory (or points `IMAGEOPTIM_PNGOUT` at it), ImageOptim-Rust automatically detects and enables it cleanly. When absent, PNGOUT stays disabled by default without errors.
