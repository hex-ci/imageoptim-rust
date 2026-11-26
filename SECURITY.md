# Security Policy

## Reporting a Vulnerability

Because ImageOptim-Rust invokes image parsers and native C/C++/Rust codecs (`libpng`, `mozjpeg`, `gifsicle`, `pngcrush`, `advpng`, `jpegoptim`, `guetzli`, `pngquant`, `oxipng`, `rquickjs` + `svgo`, `svgcleaner`) on potentially untrusted image files, security reports involving memory safety, path traversal, symlink handling, or unsafe file operations are taken seriously.

**Please do not open a public GitHub issue for security vulnerabilities.**

Instead, please report them privately:
- Use **GitHub Security Advisories** ("Report a vulnerability" under the Security tab of `hex-ci/imageoptim-rust`), or
- Email `hex@codeigniter.org.cn` with reproduction details, affected version/platform, and sample files if applicable.

## Supported Versions

Only the latest release on the `main` branch is actively supported with security updates.
