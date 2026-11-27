<div align="center">

# ImageOptim-Rust

**使用 Rust 编写的跨平台无损/有损图像压缩工具，设计灵感源自 ImageOptim。**

[English](README.md) | 简体中文

[![CI](https://github.com/hex-ci/imageoptim-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/hex-ci/imageoptim-rust/actions/workflows/ci.yml)
[![Release](https://github.com/hex-ci/imageoptim-rust/actions/workflows/release.yml/badge.svg)](https://github.com/hex-ci/imageoptim-rust/actions/workflows/release.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)

</div>

> [!NOTE]
> **非官方移植声明**：ImageOptim-Rust 是基于 Kornel Lesiński 与贡献者们开发的 [ImageOptim](https://imageoptim.com/) 打造的独立跨平台 Rust 非官方移植与重制项目，与上游 ImageOptim 官方项目无隶属或背书关系。

---

## 核心特性

- **全部开源引擎内置源码编译**：不依赖 Homebrew、系统图像库、Node.js 或外部 `PATH` 命令，所有开源优化器均直接从 [`third_party/src/`](third_party/src) 与工作区 Rust crate 编译产物内嵌打包。
- **跨平台桌面 GUI + 纯净 CLI**：
  - 完整支持 macOS（Apple Silicon `arm64` 与 Intel `x86_64` 独立安装包）、Linux（`x86_64`）和 Windows（`x64`）。
  - 桌面端支持拖拽导入/拖出、列排序/宽度与窗口持久化、撤销/重做、废纸篓恢复、双图对比预览、以及基于 SQLite（`Results.db`）的重复文件优化缓存。
  - 原生 `--cli` / `--headless` 无界面 CLI 模式，零窗口服务依赖，适合 CI、SSH 与批处理脚本。
- **原子级安全文件替换**：保留文件权限、硬链接、macOS Finder 标签/ACL/创建时间以及非缓存扩展属性，并支持通过废纸篓或临时备份还原原图。
- **无损与高质量有损管线**：多引擎串联与候选体体积择优；内置有损压缩质量约束策略（`pngquant`、`jpegoptim`、`guetzli`、`svgo`、`svgcleaner`）。
- **深度系统集成**：
  - **macOS**：完整 `.app` 包、AppleScript（`do queuecount command`）、访达“服务”右键菜单、系统 **Action / 标记扩展**（`ImageOptimize.appex`）、Quick Look 空格预览、原生拖出文件。
  - **Linux**：`.tar.gz` / `.deb` / `.AppImage` 安装包、`/usr/bin/imageoptim-rust` CLI 软链、`.desktop` 桌面菜单与 Nautilus 右键安装脚本。
  - **Windows**：NSIS 安装包与便携版 `.zip`，附带免管理员权限（`HKCU`）资源管理器右键菜单注册。
- **双语界面与内置帮助**：完整支持英文与简体中文界面及离线帮助文档。

---

## 内置压缩引擎清单

| 图像格式 | 引擎 | 构建方式 | 开源协议 |
| --- | --- | --- | --- |
| PNG | **OxiPNG** (`10.2`) | Rust crate | MIT |
| PNG | **ZopfliPNG** | Rust crate + `third_party/src/zopflipng` | Apache-2.0 |
| PNG | **pngquant** (`3.0.3`) / `libimagequant` | `crates/pngquant` + `third_party/src/pngquant` | GPL-3.0-or-later |
| PNG | **AdvPNG** | `third_party/src/advpng` | GPL-2.0-or-later |
| PNG | **PNGCrush** | `third_party/src/pngcrush` | Zlib 兼容许可 |
| PNG | **PNGOUT** *(可选，默认不分发)* | 用户自行提供的闭源二进制 | 闭源免费软件 |
| JPEG | **MozJPEG** (`jpegtran`) | `turbojpeg` crate + `third_party/src/mozjpeg` | BSD-3-Clause / IJG / Zlib |
| JPEG | **Jpegoptim** | `third_party/src/jpegoptim` | GPL-3.0-or-later |
| JPEG | **Guetzli** | `third_party/src/guetzli` | Apache-2.0 |
| GIF | **Gifsicle** | `third_party/src/gifsicle` | GPL-2.0 |
| SVG | **SVGO** | 内置 `rquickjs` JS 运行时 (`crates/core/src/svgo`) | MIT |
| SVG | **svgcleaner** | Rust crate (`svgcleaner`) | GPL-2.0-or-later |

> [!IMPORTANT]
> 由于 **PNGOUT** 为闭源专有软件，本项目的源码仓库和 GitHub Releases 公开安装包均**不包含** `pngout`。如需启用 PNGOUT，请自行从原作者官网获取对应平台的可执行文件，并将其放入工具目录（macOS 为 `ImageOptim-Rust.app/Contents/Resources/tools/pngout`，Linux/Windows 为可执行文件同级 `tools/` 目录），或通过环境变量 `IMAGEOPTIM_PNGOUT=/path/to/pngout` / `IMAGEOPTIM_TOOLS_DIR` 指定路径。

---

## 安装

请前往 [GitHub Releases](https://github.com/hex-ci/imageoptim-rust/releases) 下载对应平台的预编译包：

- **macOS**：提供独立的 Apple Silicon（`macos-arm64.dmg` / `.zip`）与 Intel（`macos-x86_64.dmg` / `.zip`）安装包。
- **Linux (`x86_64`)**：提供 `.deb`、`.AppImage` 与便携 `.tar.gz`（安装后可在终端直接调用 `imageoptim-rust`）。
- **Windows (`x64`)**：提供 NSIS 安装程序（`-setup.exe`）与便携压缩包（`.zip`）。

---

## CLI 命令行用法

```sh
# 打开图形界面处理文件并在完成后退出
ImageOptim-Rust path/to/images/

# Linux 安装包内置小写终端命令：
imageoptim-rust --cli path/to/images/

# 纯命令行（无界面）模式
ImageOptim-Rust --cli [OPTIONS] <PATHS...>
```

以下参数对应 [`crates/app/src/cli.rs`](crates/app/src/cli.rs) 中的实际实现：

| 参数 | 说明 |
| --- | --- |
| `--cli`, `--headless` | 纯命令行无界面运行（适用于脚本、CI、SSH 及系统扩展） |
| `--lossy` | 本次运行启用有损压缩 |
| `--level <0..6>` | 覆盖优化等级（`0` 快速 .. `6` 极致） |
| `--settings <FILE>` | 指定自定义 `settings.json` 配置文件路径 |
| `--no-backup` | 原地覆盖，不向废纸篓或临时目录保留原图备份 |
| `-h`, `--help` | 显示命令行帮助 |
| `-V`, `--version` | 显示版本信息 |

---

## 从源码构建与打包

### 环境准备

- **Rust**：Stable 工具链 `>= 1.99`（`cargo`、`rustc`）
- **C / C++ 编译器与 CMake**：
  - **macOS**：Xcode Command Line Tools (`clang`) + `cmake`（推荐安装 `nasm`）
  - **Linux (Debian/Ubuntu)**：
    ```sh
    sudo apt-get update
    sudo apt-get install -y \
      build-essential cmake nasm pkg-config \
      libasound2-dev libxkbcommon-dev libwayland-dev libudev-dev \
      libgtk-3-dev libx11-dev dpkg
    ```
  - **Windows**：Visual Studio Build Tools（MSVC C++ 组件）+ CMake

### 编译与测试

```sh
cargo build --workspace --release

# 运行完整测试套件（先编译工作区工具，再强制校验所有源码工具可用）
cargo build --workspace
IMAGEOPTIM_REQUIRE_EXTERNAL_TOOLS=1 cargo test --workspace
```

### 本地打包安装包

```sh
# macOS (.app, .zip, .dmg)
./scripts/package-macos.sh release
./scripts/package-macos.sh release x86_64-apple-darwin

# Linux (.tar.gz, .deb；若安装了 appimagetool 还会生成 .AppImage)
./scripts/package-linux.sh release

# Windows (.zip；若安装了 NSIS makensis 还会生成 -setup.exe)
pwsh ./scripts/package-windows.ps1 -Profile release
```

---

## 参与贡献与开源协议

- 欢迎提交 Issue 与 Pull Request，请先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。
- 若发现安全问题，请遵循 [SECURITY.md](SECURITY.md) 进行负责任披露。
- 本项目依据 **GNU General Public License v3.0 or later** 开源（[`LICENSE`](LICENSE)）。
- 完整第三方组件许可及致谢见 [`NOTICE.md`](NOTICE.md)。
