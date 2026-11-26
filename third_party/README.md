# Third-party optimizer binaries

`bin/` is populated by `scripts/bundle-tools-macos.sh` and is intentionally
git-ignored. On macOS the script copies the exact binaries shipped in the
official ImageOptim.app (including `libimageoptimjpeg.dylib` and
`liblibpng.dylib`) so the Rust port produces byte-identical output.

Windows/Linux builds will get their own copy of this directory from CI; until
then the app falls back to the Rust engines (OxiPNG, turbojpeg, svgcleaner)
and skips tools that are not present.
