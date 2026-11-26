use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    Png,
    Jpeg,
    Gif,
    Svg,
    Unknown,
}

impl FileKind {
    /// Sniffing matches `-[File initWithData:fromPath:]`: the first six bytes
    /// decide, with an extension fallback for SVG.
    pub fn detect(data: &[u8], path: &Path) -> Self {
        if data.len() >= 6 {
            let h = &data[..6];
            if h.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a]) {
                return Self::Png;
            }
            if h.starts_with(&[0xff, 0xd8, 0xff]) {
                return Self::Jpeg;
            }
            if h.starts_with(b"GIF8") {
                return Self::Gif;
            }
            if h.starts_with(b"<svg") {
                return Self::Svg;
            }
        }
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
        {
            return Self::Svg;
        }
        Self::Unknown
    }

    pub fn mime(self) -> Option<&'static str> {
        match self {
            Self::Png => Some("image/png"),
            Self::Jpeg => Some("image/jpeg"),
            Self::Gif => Some("image/gif"),
            Self::Svg => Some("image/svg"),
            Self::Unknown => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
            Self::Svg => "SVG",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageFile {
    pub path: PathBuf,
    pub kind: FileKind,
    pub size: u64,
}

impl ImageFile {
    pub fn read(path: &Path) -> std::io::Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let mut head = [0u8; 6];
        let n = f.read(&mut head)?;
        let size = f.metadata()?.len();
        Ok(Self {
            path: path.to_path_buf(),
            kind: FileKind::detect(&head[..n], path),
            size,
        })
    }

    /// Upstream: PNG < 2 KiB, other formats < 10 KiB.
    pub fn is_small(&self) -> bool {
        if self.kind == FileKind::Png {
            self.size < 2048
        } else {
            self.size < 10 * 1024
        }
    }

    /// Upstream: PNG > 250 KiB, other formats > 1 MiB.
    pub fn is_large(&self) -> bool {
        if self.kind == FileKind::Png {
            self.size > 250 * 1024
        } else {
            self.size > 1024 * 1024
        }
    }
}

/// Returns `true` if `data` is a PNG containing the given 4-byte chunk type.
pub fn png_has_chunk(data: &[u8], want: &[u8; 4]) -> bool {
    if data.len() < 8 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
        return false;
    }
    let mut i = 8usize;
    while i + 12 <= data.len() {
        let len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let typ = &data[i + 4..i + 8];
        if typ == want {
            return true;
        }
        if typ == b"IEND" || i + 12 + len > data.len() {
            break;
        }
        i += 12 + len;
    }
    false
}

/// Detects Animated PNG (APNG) via presence of the `acTL` animation control chunk.
pub fn is_apng(data: &[u8]) -> bool {
    png_has_chunk(data, b"acTL")
}
