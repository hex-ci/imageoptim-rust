#!/bin/sh
# Build a self-contained macOS .app bundle, zip archive, and DMG disk image
# (including Finder Services, AppleScript dictionary, and ImageOptimize.appex).
# Usage: ./scripts/package-macos.sh [debug|release] [rust-target-triple]
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
PROFILE=${1:-release}
TARGET=${2:-}
VERSION=${VERSION:-$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)}
VERSION=${VERSION:-0.1.0}

case "$TARGET" in
    aarch64-apple-darwin)
        PKG_ARCH=arm64
        CLANG_ARCH=arm64
        ;;
    x86_64-apple-darwin)
        PKG_ARCH=x86_64
        CLANG_ARCH=x86_64
        ;;
    "")
        HOST_ARCH=$(uname -m)
        PKG_ARCH=$HOST_ARCH
        CLANG_ARCH=$HOST_ARCH
        ;;
    *)
        echo "Unsupported macOS target: $TARGET" >&2
        exit 1
        ;;
esac

if [ -n "$TARGET" ]; then
    TARGET_DIR="$ROOT/target/$TARGET/$PROFILE"
else
    TARGET_DIR="$ROOT/target/$PROFILE"
fi

APP="$ROOT/dist/ImageOptim-Rust.app"
APPEX="$APP/Contents/PlugIns/ImageOptimize.appex"
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

rm -rf "$ROOT/dist/ImageOptim.app" "$APP"
mkdir -p \
    "$APP/Contents/MacOS" \
    "$APP/Contents/Resources/tools" \
    "$APP/Contents/Resources/en.lproj" \
    "$APP/Contents/Resources/zh-Hans.lproj" \
    "$APPEX/Contents/MacOS" \
    "$APPEX/Contents/Resources/en.lproj" \
    "$APPEX/Contents/Resources/zh-Hans.lproj"

cp "$BIN" "$APP/Contents/MacOS/ImageOptim-Rust"
chmod 755 "$APP/Contents/MacOS/ImageOptim-Rust"

# Ship the bundled .icns and AppleScript dictionary.
if [ -f "$ROOT/crates/app/assets/ImageOptim.icns" ]; then
    cp "$ROOT/crates/app/assets/ImageOptim.icns" "$APP/Contents/Resources/ImageOptim-Rust.icns"
    cp "$ROOT/crates/app/assets/ImageOptim.icns" "$APPEX/Contents/Resources/ImageOptim-Rust.icns"
fi
if [ -f "$ROOT/crates/app/assets/ImageOptimVerbs.sdef" ]; then
    cp "$ROOT/crates/app/assets/ImageOptimVerbs.sdef" "$APP/Contents/Resources/ImageOptimVerbs.sdef"
fi

# Localized Finder Services menu titles.
cat > "$APP/Contents/Resources/en.lproj/ServicesMenu.strings" <<'STR'
"ImageOptimize" = "Optimize with ImageOptim-Rust";
STR
cat > "$APP/Contents/Resources/zh-Hans.lproj/ServicesMenu.strings" <<'STR'
"ImageOptimize" = "使用 ImageOptim-Rust 优化图像";
STR

# Bundle all vendored open-source optimizers built from source in this workspace.
# Closed-source PNGOUT is never redistributed in public builds unless BUNDLE_PNGOUT=1.
if [ "${BUNDLE_PNGOUT:-0}" = "1" ] && [ -f "$ROOT/third_party/bin/pngout" ]; then
    cp "$ROOT/third_party/bin/pngout" "$APP/Contents/Resources/tools/pngout"
fi
if [ -d "$TARGET_DIR/tools" ]; then
    cp -R "$TARGET_DIR/tools/." "$APP/Contents/Resources/tools/"
fi
if [ -f "$TARGET_DIR/pngquant" ]; then
    cp "$TARGET_DIR/pngquant" "$APP/Contents/Resources/tools/pngquant"
fi
chmod -R a+rx "$APP/Contents/Resources/tools" 2>/dev/null || true

# Compile the native Action / Markup UI Service Extension (`ImageOptimize.appex`).
clang \
    -arch "$CLANG_ARCH" \
    -O2 \
    -Wall -Wextra \
    -mmacosx-version-min=11.0 \
    -fobjc-arc \
    -fapplication-extension \
    -e _NSExtensionMain \
    -framework Cocoa \
    "$ROOT/crates/app/macos/extension.m" \
    -o "$APPEX/Contents/MacOS/ImageOptimize"

cat > "$APPEX/Contents/Resources/en.lproj/InfoPlist.strings" <<'STR'
"CFBundleDisplayName" = "ImageOptimize";
"CFBundleName" = "ImageOptimize";
STR
cat > "$APPEX/Contents/Resources/zh-Hans.lproj/InfoPlist.strings" <<'STR'
"CFBundleDisplayName" = "使用 ImageOptim-Rust 优化图像";
"CFBundleName" = "ImageOptimize";
STR

cat > "$APPEX/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>ImageOptimize</string>
    <key>CFBundleExecutable</key><string>ImageOptimize</string>
    <key>CFBundleIconFile</key><string>ImageOptim-Rust.icns</string>
    <key>CFBundleIdentifier</key><string>io.github.hex-ci.imageoptim-rust.ImageOptimizeExtension</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>ImageOptimize</string>
    <key>CFBundlePackageType</key><string>XPC!</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleSupportedPlatforms</key><array><string>MacOSX</string></array>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSExtension</key>
    <dict>
        <key>NSExtensionAttributes</key>
        <dict>
            <key>NSExtensionActivationRule</key>
            <dict>
                <key>NSExtensionActivationSupportsImageWithMaxCount</key>
                <integer>100</integer>
            </dict>
            <key>NSExtensionServiceRoleType</key>
            <string>NSExtensionServiceRoleTypeEditor</string>
        </dict>
        <key>NSExtensionPointIdentifier</key>
        <string>com.apple.ui-services</string>
        <key>NSExtensionPrincipalClass</key>
        <string>ExtensionController</string>
    </dict>
</dict>
</plist>
PLIST

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleAllowMixedLocalizations</key><true/>
    <key>CFBundleExecutable</key><string>ImageOptim-Rust</string>
    <key>CFBundleIdentifier</key><string>io.github.hex-ci.imageoptim-rust</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>ImageOptim-Rust</string>
    <key>CFBundleDisplayName</key><string>ImageOptim-Rust</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleIconFile</key><string>ImageOptim-Rust.icns</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>GPL-3.0-or-later — Unofficial Rust port of ImageOptim</string>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>NSAppleScriptEnabled</key><true/>
    <key>OSAScriptingDefinition</key><string>ImageOptimVerbs.sdef</string>
    <key>NSServices</key>
    <array>
        <dict>
            <key>NSMenuItem</key>
            <dict>
                <key>default</key><string>ImageOptimize</string>
            </dict>
            <key>NSMessage</key><string>handleServices</string>
            <key>NSRequiredContext</key>
            <dict>
                <key>NSApplicationIdentifier</key><string>com.apple.finder</string>
                <key>NSTextContent</key><string>FilePath</string>
            </dict>
            <key>NSSendFileTypes</key>
            <array>
                <string>public.png</string>
                <string>public.jpeg</string>
                <string>com.compuserve.gif</string>
                <string>public.svg-image</string>
                <string>public.folder</string>
            </array>
            <key>NSServiceDescription</key><string>Losslessly reduce image file sizes with ImageOptim-Rust</string>
        </dict>
    </array>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key><string>PNG</string>
            <key>CFBundleTypeRole</key><string>Editor</string>
            <key>LSHandlerRank</key><string>Alternate</string>
            <key>CFBundleTypeExtensions</key><array><string>png</string><string>PNG</string></array>
            <key>CFBundleTypeMIMETypes</key><array><string>image/png</string></array>
            <key>LSItemContentTypes</key><array><string>public.png</string></array>
        </dict>
        <dict>
            <key>CFBundleTypeName</key><string>JPEG</string>
            <key>CFBundleTypeRole</key><string>Editor</string>
            <key>LSHandlerRank</key><string>Alternate</string>
            <key>CFBundleTypeExtensions</key><array><string>jpeg</string><string>jpg</string><string>JPG</string><string>JPEG</string></array>
            <key>CFBundleTypeMIMETypes</key><array><string>image/jpeg</string><string>image/pjpeg</string></array>
            <key>LSItemContentTypes</key><array><string>public.jpeg</string></array>
        </dict>
        <dict>
            <key>CFBundleTypeName</key><string>GIF</string>
            <key>CFBundleTypeRole</key><string>Editor</string>
            <key>LSHandlerRank</key><string>Alternate</string>
            <key>CFBundleTypeExtensions</key><array><string>gif</string><string>GIF</string></array>
            <key>CFBundleTypeMIMETypes</key><array><string>image/gif</string></array>
            <key>LSItemContentTypes</key><array><string>com.compuserve.gif</string></array>
        </dict>
        <dict>
            <key>CFBundleTypeName</key><string>SVG</string>
            <key>CFBundleTypeRole</key><string>Editor</string>
            <key>LSHandlerRank</key><string>Alternate</string>
            <key>CFBundleTypeExtensions</key><array><string>svg</string><string>SVG</string></array>
            <key>CFBundleTypeMIMETypes</key><array><string>image/svg+xml</string></array>
            <key>LSItemContentTypes</key><array><string>public.svg-image</string></array>
        </dict>
        <dict>
            <key>CFBundleTypeName</key><string>Folder</string>
            <key>CFBundleTypeRole</key><string>Editor</string>
            <key>LSHandlerRank</key><string>Alternate</string>
            <key>LSItemContentTypes</key><array><string>public.folder</string></array>
        </dict>
    </array>
</dict>
</plist>
PLIST

codesign --force --sign - "$APPEX" >/dev/null 2>&1 || true
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
echo "built $APP"

if [ "${CREATE_ZIP:-1}" = "1" ]; then
    ZIP_OUT="$ROOT/dist/ImageOptim-Rust-$VERSION-macos-$PKG_ARCH.zip"
    rm -f "$ZIP_OUT"
    ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP_OUT"
    echo "built $ZIP_OUT"
fi

if [ "${CREATE_DMG:-1}" = "1" ]; then
    DMG_OUT="$ROOT/dist/ImageOptim-Rust-$VERSION-macos-$PKG_ARCH.dmg"
    DMG_STAGE=$(mktemp -d)
    trap 'rm -rf "$DMG_STAGE"' EXIT
    cp -R "$APP" "$DMG_STAGE/ImageOptim-Rust.app"
    ln -s /Applications "$DMG_STAGE/Applications"
    rm -f "$DMG_OUT"
    hdiutil create \
        -volname "ImageOptim-Rust $VERSION" \
        -srcfolder "$DMG_STAGE" \
        -ov \
        -format UDZO \
        "$DMG_OUT" >/dev/null
    echo "built $DMG_OUT"
fi
