#!/bin/sh
# Build self-contained Linux distribution artifacts:
#   - dist/ImageOptim-Rust.AppDir/                      (AppImage-compatible portable tree)
#   - dist/ImageOptim-Rust-<ver>-linux-<arch>.tar.gz
#   - dist/ImageOptim-Rust-<ver>-linux-<arch>.AppImage  (when `appimagetool` is available)
#   - dist/imageoptim-rust_<ver>_<debarch>.deb          (when `dpkg-deb` is available)
#
# Usage: ./scripts/package-linux.sh [debug|release] [rust-target-triple]
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
PROFILE=${1:-release}
TARGET=${2:-}
VERSION=${VERSION:-$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)}
VERSION=${VERSION:-0.1.0}

if [ -n "$TARGET" ]; then
    ARCH=${TARGET%%-*}
    TARGET_DIR="$ROOT/target/$TARGET/$PROFILE"
else
    ARCH=$(uname -m)
    TARGET_DIR="$ROOT/target/$PROFILE"
fi

APPDIR="$ROOT/dist/ImageOptim-Rust.AppDir"
BIN="$TARGET_DIR/ImageOptim-Rust"

cd "$ROOT"
if [ "$PROFILE" = "release" ] && [ -n "$TARGET" ]; then
    cargo build --workspace --release --target "$TARGET"
elif [ "$PROFILE" = "release" ]; then
    cargo build --workspace --release
elif [ -n "$TARGET" ]; then
    cargo build --workspace --target "$TARGET"
else
    cargo build --workspace
fi

rm -rf "$APPDIR"
mkdir -p \
    "$APPDIR/usr/bin" \
    "$APPDIR/usr/lib/imageoptim-rust/tools" \
    "$APPDIR/usr/share/applications" \
    "$APPDIR/usr/share/icons/hicolor/256x256/apps" \
    "$APPDIR/usr/share/imageoptim-rust"

cp "$BIN" "$APPDIR/usr/lib/imageoptim-rust/ImageOptim-Rust"
chmod 755 "$APPDIR/usr/lib/imageoptim-rust/ImageOptim-Rust"
ln -sf ../lib/imageoptim-rust/ImageOptim-Rust "$APPDIR/usr/bin/imageoptim-rust"

# Bundle all vendored open-source optimizers built from source in this workspace.
# Closed-source PNGOUT is never redistributed in public builds unless BUNDLE_PNGOUT=1.
if [ "${BUNDLE_PNGOUT:-0}" = "1" ] && [ -f "$ROOT/third_party/bin/linux-$ARCH/pngout" ]; then
    cp "$ROOT/third_party/bin/linux-$ARCH/pngout" "$APPDIR/usr/lib/imageoptim-rust/tools/pngout"
fi
if [ -d "$TARGET_DIR/tools" ]; then
    cp -R "$TARGET_DIR/tools/." "$APPDIR/usr/lib/imageoptim-rust/tools/"
fi
if [ -f "$TARGET_DIR/pngquant" ]; then
    cp "$TARGET_DIR/pngquant" "$APPDIR/usr/lib/imageoptim-rust/tools/pngquant"
fi
chmod -R a+rx "$APPDIR/usr/lib/imageoptim-rust/tools" 2>/dev/null || true

cp "$ROOT/crates/app/assets/app-icon.png" \
    "$APPDIR/usr/share/icons/hicolor/256x256/apps/imageoptim-rust.png"
cp "$ROOT/crates/app/assets/app-icon.png" "$APPDIR/imageoptim-rust.png"

cat > "$APPDIR/usr/share/applications/imageoptim-rust.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=ImageOptim-Rust
Name[zh_CN]=ImageOptim-Rust 图像压缩
GenericName=Image Optimizer
GenericName[zh_CN]=无损图像压缩工具
Comment=Losslessly reduce PNG, JPEG, GIF, and SVG file sizes
Comment[zh_CN]=无损或微量有损压缩 PNG、JPEG、GIF 与 SVG 图像体积
Exec=imageoptim-rust %F
TryExec=imageoptim-rust
Icon=imageoptim-rust
Terminal=false
Categories=Utility;Compression;
MimeType=image/png;image/jpeg;image/gif;image/svg+xml;inode/directory;
StartupNotify=true
Actions=OptimizeHeadless;

[Desktop Action OptimizeHeadless]
Name=Optimize Headless (CLI)
Name[zh_CN]=静默压缩 (无界面)
Exec=imageoptim-rust --cli %F
DESKTOP

cp "$APPDIR/usr/share/applications/imageoptim-rust.desktop" "$APPDIR/imageoptim-rust.desktop"

cat > "$APPDIR/AppRun" <<'APPRUN'
#!/bin/sh
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$HERE/usr/lib/imageoptim-rust/ImageOptim-Rust" "$@"
APPRUN
chmod 755 "$APPDIR/AppRun"

# Optional GNOME Files (Nautilus) right-click script installer.
cat > "$APPDIR/usr/share/imageoptim-rust/install-nautilus-script.sh" <<'NAUTILUS'
#!/bin/sh
set -eu
SCRIPT_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/nautilus/scripts"
mkdir -p "$SCRIPT_DIR"
TARGET="$SCRIPT_DIR/Optimize with ImageOptim-Rust"
cat > "$TARGET" <<'EOF'
#!/bin/sh
IFS='
'
exec imageoptim-rust $NAUTILUS_SCRIPT_SELECTED_FILE_PATHS
EOF
chmod 755 "$TARGET"
echo "Installed Nautilus right-click script: $TARGET"
NAUTILUS
chmod 755 "$APPDIR/usr/share/imageoptim-rust/install-nautilus-script.sh"

TARBALL="$ROOT/dist/ImageOptim-Rust-$VERSION-linux-$ARCH.tar.gz"
tar -C "$ROOT/dist" -czf "$TARBALL" ImageOptim-Rust.AppDir
echo "built $TARBALL"

TOOL_APPIMAGE=${APPIMAGETOOL:-appimagetool}
if command -v "$TOOL_APPIMAGE" >/dev/null 2>&1 || [ -x "$TOOL_APPIMAGE" ]; then
    APPIMAGE="$ROOT/dist/ImageOptim-Rust-$VERSION-linux-$ARCH.AppImage"
    ARCH="$ARCH" "$TOOL_APPIMAGE" "$APPDIR" "$APPIMAGE"
    echo "built $APPIMAGE"
fi

if command -v dpkg-deb >/dev/null 2>&1; then
    case "$ARCH" in
        x86_64) DEB_ARCH=amd64 ;;
        aarch64|arm64) DEB_ARCH=arm64 ;;
        *) DEB_ARCH=$(dpkg --print-architecture 2>/dev/null || echo amd64) ;;
    esac
    DEB_STAGE="$ROOT/dist/deb-stage"
    rm -rf "$DEB_STAGE"
    mkdir -p "$DEB_STAGE/DEBIAN"
    cp -R "$APPDIR/usr" "$DEB_STAGE/usr"
    cat > "$DEB_STAGE/DEBIAN/control" <<EOF
Package: imageoptim-rust
Version: $VERSION
Section: graphics
Priority: optional
Architecture: $DEB_ARCH
Maintainer: Hex <hex@codeigniter.org.cn>
Homepage: https://github.com/hex-ci/imageoptim-rust
Depends: libc6, libstdc++6
Description: Lossless and lossy PNG, JPEG, GIF, and SVG optimizer
 ImageOptim-Rust is an unofficial cross-platform Rust port inspired by ImageOptim,
 bundling OxiPNG, Zopfli, AdvPNG, PNGCrush, pngquant, MozJPEG, Jpegoptim, Guetzli,
 Gifsicle, SVGO, and svgcleaner compiled from source.
EOF
    DEB_OUT="$ROOT/dist/imageoptim-rust_${VERSION}_${DEB_ARCH}.deb"
    dpkg-deb --build --root-owner-group "$DEB_STAGE" "$DEB_OUT"
    rm -rf "$DEB_STAGE"
    echo "built $DEB_OUT"
fi
