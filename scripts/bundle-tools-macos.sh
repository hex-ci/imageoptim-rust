#!/bin/sh
# Optional developer helper to populate third_party/bin from a local ImageOptim.app or Homebrew.
# Note: proprietary PNGOUT is never redistributed in public releases (see NOTICE.md).
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
DEST="$ROOT/third_party/bin"
mkdir -p "$DEST"

# 1) Prefer the exact binaries shipped inside the official app: byte-parity.
RES="/Applications/ImageOptim.app/Contents/Frameworks/ImageOptimGPL.framework/Versions/A/Resources"
if [ -d "$RES" ]; then
    for f in advpng gifsicle guetzli jpegoptim jpegtran oxipng pngcrush pngout pngquant svgcleaner zopflipng libimageoptimjpeg.dylib liblibpng.dylib; do
        if [ -f "$RES/$f" ]; then
            cp -f "$RES/$f" "$DEST/$f"
            chmod u+w "$DEST/$f"
        fi
    done
    echo "copied tools from $RES"
    exit 0
fi

# 2) Fallback: Homebrew.
copy() {
    src=$(command -v "$1" || true)
    if [ -x "$src" ]; then cp -f "$src" "$DEST/$1"; echo "copied $1"; else echo "missing $1" >&2; fi
}
copy_path() {
    if [ -x "$2" ]; then cp -f "$2" "$DEST/$1"; echo "copied $1"; else echo "missing $1" >&2; fi
}
copy advpng
copy pngcrush
copy gifsicle
copy guetzli
copy jpegoptim
copy svgo
if [ -x /opt/homebrew/opt/mozjpeg/bin/jpegtran ]; then
    copy_path jpegtran /opt/homebrew/opt/mozjpeg/bin/jpegtran
elif [ -x /usr/local/opt/mozjpeg/bin/jpegtran ]; then
    copy_path jpegtran /usr/local/opt/mozjpeg/bin/jpegtran
else
    copy jpegtran
fi
echo "zopflipng/pngout/pngquant/oxipng/svgcleaner may be missing in the Homebrew fallback" >&2
