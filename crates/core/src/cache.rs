use std::path::{Path, PathBuf};

use md5::{Digest, Md5};
use rusqlite::{Connection, OptionalExtension, params};

use crate::settings::Settings;

/// Results cache: remembers files that could not be improved further, keyed by
/// a hash of (settings, file bytes). Mirrors the original `Results.db` schema.
pub struct ResultsCache {
    conn: Option<Connection>,
    settings_hash: [u8; 16],
}

impl ResultsCache {
    pub fn open(settings: &Settings) -> Self {
        let dir = crate::settings::project_dirs()
            .map(|d| d.cache_dir().to_path_buf())
            .unwrap_or_else(std::env::temp_dir);
        let _ = std::fs::create_dir_all(&dir);
        Self::open_at(&dir.join("Results.db"), settings)
    }

    pub fn open_at(path: &Path, settings: &Settings) -> Self {
        let conn = Connection::open(path).ok();
        if let Some(c) = &conn {
            let _ = c.execute_batch(
                "PRAGMA journal_mode = WAL;\
                 PRAGMA synchronous = NORMAL;\
                 PRAGMA busy_timeout = 5000;\
                 CREATE TABLE IF NOT EXISTS results(\
                    inputs_hashsh BLOB(16) NOT NULL PRIMARY KEY, \
                    size INT NOT NULL, \
                    status INT NOT NULL DEFAULT 0);\
                 CREATE INDEX IF NOT EXISTS results_size ON results(size);",
            );
            // Bound unbounded cache growth across long-lived installations.
            if let Ok(count) =
                c.query_row("SELECT COUNT(*) FROM results", [], |r| r.get::<_, i64>(0))
                && count > 12_000
            {
                let _ = c.execute(
                    "DELETE FROM results WHERE rowid NOT IN \
                     (SELECT rowid FROM results ORDER BY rowid DESC LIMIT 10000)",
                    [],
                );
            }
        }
        Self {
            conn,
            settings_hash: Self::settings_hash(settings),
        }
    }

    pub fn disabled() -> Self {
        Self {
            conn: None,
            settings_hash: [0; 16],
        }
    }

    fn settings_hash(settings: &Settings) -> [u8; 16] {
        let mut h = Md5::new();
        h.update(b"3");
        // Only the knobs that change optimizer output belong in the hash.
        for v in [
            settings.lossy_enabled as u8,
            settings.jpeg_quality,
            settings.png_quality,
            settings.gif_quality,
            settings.level,
            settings.oxipng_enabled as u8,
            settings.zopfli_enabled as u8,
            settings.jpegoptim_enabled as u8,
            settings.jpegtran_enabled as u8,
            settings.strip_jpeg_metadata as u8,
            settings.strip_png_metadata as u8,
            settings.gifsicle_enabled as u8,
            settings.svgcleaner_enabled as u8,
            settings.guetzli_enabled as u8,
            settings.pngcrush_enabled as u8,
            settings.pngout_enabled as u8,
            settings.advpng_enabled as u8,
            settings.svgo_enabled as u8,
        ] {
            h.update([v]);
        }
        h.finalize().into()
    }

    pub fn file_hash(&self, data: &[u8], rerun: bool) -> [u8; 16] {
        let mut h = Md5::new();
        h.update(self.settings_hash);
        // Upstream's worker settingsIdentifier includes alternativeStrategy,
        // which is set for files that have been optimized before.
        h.update([u8::from(rerun)]);
        h.update(data);
        h.finalize().into()
    }

    pub fn contains(&self, hash: &[u8; 16]) -> bool {
        let Some(conn) = &self.conn else {
            return false;
        };
        conn.query_row(
            "SELECT 1 FROM results WHERE inputs_hashsh = ?1 LIMIT 1",
            params![hash.as_slice()],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some()
    }

    pub fn remember_unoptimizable(&self, hash: &[u8; 16], size: u64) {
        let Some(conn) = &self.conn else {
            return;
        };
        let _ = conn.execute(
            "INSERT OR REPLACE INTO results(inputs_hashsh, size, status) VALUES(?1, ?2, 1)",
            params![hash.as_slice(), size as i64],
        );
    }

    pub fn clear(&self) -> rusqlite::Result<()> {
        if let Some(conn) = &self.conn {
            conn.execute("DELETE FROM results", [])?;
        }
        Ok(())
    }
}

/// Per-user scratch area for revert copies and temporary output.
pub fn scratch_dir() -> PathBuf {
    let dir = crate::settings::project_dirs()
        .map(|d| d.cache_dir().to_path_buf())
        .unwrap_or_else(std::env::temp_dir)
        .join("revert");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn unique_scratch_path(file_name: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    scratch_dir().join(format!("{stamp:x}-{file_name}"))
}

/// Best-effort cleanup of old revert copies (called at startup).
pub fn prune_scratch(max_age: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(scratch_dir()) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if now.duration_since(modified).unwrap_or_default() > max_age {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub fn is_writable_dir(path: &Path) -> bool {
    let dir = path.parent().unwrap_or(path);
    std::fs::metadata(dir)
        .map(|m| !m.permissions().readonly())
        .unwrap_or(false)
}
