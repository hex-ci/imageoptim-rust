English | [简体中文](#简体中文)

## Downloads

| Platform | File | Install |
| --- | --- | --- |
| macOS (Apple Silicon) | `ImageOptim-Rust-{{VERSION}}-macos-arm64.dmg` | Open DMG and drag `ImageOptim-Rust.app` to `Applications` |
| macOS (Apple Silicon, Portable) | `ImageOptim-Rust-{{VERSION}}-macos-arm64.zip` | Unzip and move `ImageOptim-Rust.app` to `Applications` |
| macOS (Intel) | `ImageOptim-Rust-{{VERSION}}-macos-x86_64.dmg` | Open DMG and drag `ImageOptim-Rust.app` to `Applications` |
| macOS (Intel, Portable) | `ImageOptim-Rust-{{VERSION}}-macos-x86_64.zip` | Unzip and move `ImageOptim-Rust.app` to `Applications` |
| Windows x64 (Installer) | `ImageOptim-Rust-{{VERSION}}-windows-x64-setup.exe` | Run the NSIS setup installer |
| Windows x64 (Portable) | `ImageOptim-Rust-{{VERSION}}-windows-x64.zip` | Extract and run `ImageOptim-Rust.exe` |
| Linux x64 (Debian / Ubuntu) | `imageoptim-rust_{{VERSION}}_amd64.deb` | `sudo apt install ./imageoptim-rust_{{VERSION}}_amd64.deb` |
| Linux x64 (AppImage) | `ImageOptim-Rust-{{VERSION}}-linux-x86_64.AppImage` | `chmod +x` and run directly |
| Linux x64 (Tarball) | `ImageOptim-Rust-{{VERSION}}-linux-x86_64.tar.gz` | Extract and run `ImageOptim-Rust.AppDir/AppRun` |

Verify checksums: `sha256sum -c SHA256SUMS.txt` (on macOS: `shasum -a 256 -c SHA256SUMS.txt`).

## Installation

**macOS**: The DMG and `.app` bundle are only ad-hoc signed and are not notarized by Apple, so Gatekeeper will block them on first launch. Since macOS 15, right-click "Open" no longer bypasses Gatekeeper; follow these steps **before opening the app for the first time** (replace `arm64` with `x86_64` on Intel Macs):

1. If double-clicking the DMG says Apple cannot verify it for malware and refuses to mount, clear the quarantine attribute on the downloaded archive first (same for `.zip` before extracting):

   ```bash
   xattr -d com.apple.quarantine ~/Downloads/ImageOptim-Rust-{{VERSION}}-macos-arm64.dmg
   ```

   Then open the DMG and drag `ImageOptim-Rust.app` into `Applications`.

2. After moving `ImageOptim-Rust.app` into `Applications`, recursively clear the quarantine attribute **before first launch** (this authorizes both the main app and the 8 bundled optimizers in `Contents/Resources/tools/` and prevents Gatekeeper / `syspolicyd` from getting stuck bouncing in the Dock):

   ```bash
   xattr -dr com.apple.quarantine /Applications/ImageOptim-Rust.app
   ```

3. GUI alternative: after the initial block, click "Done", open **System Settings → Privacy & Security**, scroll to **Security**, click **Open Anyway**, authenticate, and launch again from Finder. If repeated launch attempts caused the app icon to bounce endlessly in the Dock without showing a window, force-quit it, run step 2 above, and restart `syspolicyd` (`sudo killall syspolicyd`) or reboot your Mac.

These `xattr` commands only remove the browser's download quarantine flag and do not alter the binaries. Repeat step 1 when downloading future releases via a browser.

**Windows**: The NSIS installer installs per-user under `%LOCALAPPDATA%\Programs\ImageOptim-Rust` (no Administrator UAC prompt required) and registers a Start Menu shortcut plus Explorer right-click menu entries for PNG, JPEG, GIF, SVG, and folders. The portable `.zip` includes `install-context-menu.ps1` and `uninstall-context-menu.ps1`.

**Linux**: The `.deb` package installs the GUI (`ImageOptim-Rust`), `/usr/bin/imageoptim-rust` CLI symlink, icon, `.desktop` menu entry, and an optional Nautilus right-click script installer (`/usr/share/imageoptim-rust/install-nautilus-script.sh`). `.AppImage` runs after `chmod +x`. GUI mode requires X11 or Wayland; use `--cli` for headless environments and CI.

## Notes

- macOS binaries use ad-hoc signing (`codesign --force --deep --sign -`) without an Apple Developer ID certificate or notarization, and Windows binaries are unsigned (no Authenticode certificate), so Gatekeeper, SmartScreen, or antivirus warnings may appear.
- Licensed under `GPL-3.0-or-later`. All bundled open-source optimizers are built from vendored source (OxiPNG, ZopfliPNG, pngquant 3.0.3, AdvPNG, PNGCrush, MozJPEG `jpegtran`, Jpegoptim, Guetzli, Gifsicle, SVGO, svgcleaner); closed-source `PNGOUT` is omitted.
- Unofficial independent Rust port inspired by ImageOptim; not affiliated with or endorsed by the upstream ImageOptim project.

---

## 简体中文

### 下载

| 平台 | 文件 | 安装 |
| --- | --- | --- |
| macOS（Apple Silicon / M 系列） | `ImageOptim-Rust-{{VERSION}}-macos-arm64.dmg` | 打开 DMG，把 `ImageOptim-Rust.app` 拖入 `Applications` |
| macOS（Apple Silicon 便携包） | `ImageOptim-Rust-{{VERSION}}-macos-arm64.zip` | 解压后把 `ImageOptim-Rust.app` 拖入 `Applications` |
| macOS（Intel） | `ImageOptim-Rust-{{VERSION}}-macos-x86_64.dmg` | 打开 DMG，把 `ImageOptim-Rust.app` 拖入 `Applications` |
| macOS（Intel 便携包） | `ImageOptim-Rust-{{VERSION}}-macos-x86_64.zip` | 解压后把 `ImageOptim-Rust.app` 拖入 `Applications` |
| Windows x64（安装器） | `ImageOptim-Rust-{{VERSION}}-windows-x64-setup.exe` | 双击运行 NSIS 安装器 |
| Windows x64（便携版） | `ImageOptim-Rust-{{VERSION}}-windows-x64.zip` | 解压后直接运行 `ImageOptim-Rust.exe` |
| Linux x64（Debian / Ubuntu） | `imageoptim-rust_{{VERSION}}_amd64.deb` | `sudo apt install ./imageoptim-rust_{{VERSION}}_amd64.deb` |
| Linux x64（通用 AppImage） | `ImageOptim-Rust-{{VERSION}}-linux-x86_64.AppImage` | `chmod +x` 后直接运行 |
| Linux x64（便携压缩包） | `ImageOptim-Rust-{{VERSION}}-linux-x86_64.tar.gz` | 解压后运行 `ImageOptim-Rust.AppDir/AppRun` |

校验：`sha256sum -c SHA256SUMS.txt`（macOS 用 `shasum -a 256 -c SHA256SUMS.txt`）。

### 安装说明

**macOS**：DMG 与 `.app` 仅为 ad-hoc 签名、未做 Apple 公证，Gatekeeper 会拦截。macOS 15 起右键「打开」已无法绕过，建议**在首次打开前**按下面步骤处理（Intel 机型请将文件名里的 `arm64` 替换为 `x86_64`）：

1. 双击 DMG 就提示“Apple 无法验证……是否包含可能危害 Mac 安全或泄漏隐私的恶意软件”，镜像无法挂载——先去掉下载文件的隔离标记（`.zip` 同理，先清隔离标记再解压）：

   ```bash
   xattr -d com.apple.quarantine ~/Downloads/ImageOptim-Rust-{{VERSION}}-macos-arm64.dmg
   ```

   之后即可正常挂载，把 `ImageOptim-Rust.app` 拖进 `Applications`。

2. 拖进 `Applications` 后、**首次启动前**，对应用本体执行一次递归清理（会一并放行主程序与 `Contents/Resources/tools/` 内置压缩工具，避免触发 Gatekeeper 后卡在 Dock 跳动）：

   ```bash
   xattr -dr com.apple.quarantine /Applications/ImageOptim-Rust.app
   ```

3. 不想敲命令行：首次被拦后点击“完成”，打开“系统设置 → 隐私与安全性”，下滚到“安全性”区域点“仍要打开”并确认密码 / Touch ID，再回 Finder 打开。若此前反复点开导致应用一直在 Dock 跳动无法弹出窗口，先强退跳动进程、执行上面第 2 步命令后运行 `sudo killall syspolicyd`（或重启电脑）即可恢复。

以上操作仅撤掉浏览器赋予下载文件的隔离标记，不修改程序本身；后续下载新版本升级时按同样方式处理即可。

**Windows**：NSIS 安装器默认安装到当前用户目录 `%LOCALAPPDATA%\Programs\ImageOptim-Rust`（无需管理员权限），会自动添加开始菜单快捷方式及资源管理器右键菜单（支持 PNG / JPEG / GIF / SVG 与文件夹）；便携 `.zip` 解压即可运行，内附 `install-context-menu.ps1` 与 `uninstall-context-menu.ps1`。

**Linux**：`.deb` 会安装 GUI、`/usr/bin/imageoptim-rust` 命令行软链、图标、`.desktop` 菜单项及可选 Nautilus 右键安装脚本（`/usr/share/imageoptim-rust/install-nautilus-script.sh`）；`.AppImage` 赋予执行权限后即可运行；GUI 需要 X11 / Wayland 环境，无桌面或 CI 环境可直接加 `--cli` 以无头模式运行。

### 说明

- macOS 二进制仅做 ad-hoc 签名（`codesign --force --deep --sign -`），无 Apple Developer ID 签名与公证；Windows 安装包与可执行文件未购买 Authenticode 代码签名证书，因此可能触发 Gatekeeper、SmartScreen 或杀毒软件提示。
- 本项目以 `GPL-3.0-or-later` 协议开源，内置开源优化工具全部从仓库源码编译打包（OxiPNG、ZopfliPNG、pngquant 3.0.3、AdvPNG、PNGCrush、MozJPEG `jpegtran`、Jpegoptim、Guetzli、Gifsicle、SVGO、svgcleaner）；由于许可证限制，公开版本不包含闭源的 `PNGOUT`。
- 本项目为基于 Rust 编写的非官方跨平台复刻版，灵感来自 ImageOptim，与原版 ImageOptim 项目及作者无隶属或背书关系。
