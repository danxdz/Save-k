//! Persistent metadata-only catalog stored in a user-selected local SQLite file.
//!
//! The library never writes to the indexed source. An unsuccessful or incomplete
//! scan leaves the last successfully indexed snapshot intact.
use crate::{scan_directory, FileCategory, ScanWarning};
use rusqlite::{params, Connection};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug)]
pub enum CatalogError {
    Io(io::Error),
    Database(rusqlite::Error),
    InvalidSource(&'static str),
    NonUnicodePath(PathBuf),
    IncompleteScan(Vec<ScanWarning>),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "filesystem error: {e}"),
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::InvalidSource(msg) => write!(f, "invalid source: {msg}"),
            Self::NonUnicodePath(p) => write!(f, "non-Unicode path unsupported: {}", p.display()),
            Self::IncompleteScan(warnings) => {
                write!(f, "scan incomplete ({} warnings); catalog not changed", warnings.len())
            }
        }
    }
}
impl Error for CatalogError {}
impl From<io::Error> for CatalogError {
    fn from(value: io::Error) -> Self { Self::Io(value) }
}
impl From<rusqlite::Error> for CatalogError {
    fn from(value: rusqlite::Error) -> Self { Self::Database(value) }
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub path: String,
    pub source_root: String,
    pub size_bytes: u64,
    pub category: FileCategory,
    /// Seconds since Unix epoch; missing or pre-epoch dates are None.
    pub modified_unix_seconds: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshStats {
    pub indexed_files: usize,
    pub skipped_links: usize,
    pub skipped_mounts: usize,
}

/// Metadata is **not encrypted**. Store this file in a private local folder,
/// outside all indexed source trees. Never put it on the NAS being scanned.
pub struct Catalog {
    connection: Connection,
    database_path: PathBuf,
}

impl Catalog {
    pub fn open(db_path: &Path) -> Result<Self, CatalogError> {
        if db_path == Path::new(":memory:") {
            return Err(CatalogError::InvalidSource("use a real local database path"));
        }
        let connection = Connection::open(db_path)?;
        let database_path = fs::canonicalize(db_path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS catalog_sources (
                 id INTEGER PRIMARY KEY,
                 root TEXT NOT NULL UNIQUE
             );
             CREATE TABLE IF NOT EXISTS catalog_files (
                 source_id INTEGER NOT NULL REFERENCES catalog_sources(id) ON DELETE CASCADE,
                 path TEXT NOT NULL,
                 name TEXT NOT NULL,
                 size_bytes INTEGER NOT NULL,
                 category TEXT NOT NULL,
                 modified_unix_seconds INTEGER,
                 PRIMARY KEY(source_id, path)
             );
             CREATE INDEX IF NOT EXISTS idx_catalog_name ON catalog_files(name);
             CREATE INDEX IF NOT EXISTS idx_catalog_category ON catalog_files(category);"
        )?;
        Ok(Self { connection, database_path })
    }

    /// Replace one source's snapshot only after a fully successful scan.
    ///
    /// All writes are made solely to the local database, in one transaction.
    /// If the share disappears or any subtree cannot be read, the prior source
    /// snapshot is preserved. A source must be explicitly selected each time.
    pub fn refresh_source(&mut self, selected_root: &Path) -> Result<RefreshStats, CatalogError> {
        let scan = scan_directory(selected_root)?;
        if !scan.warnings.is_empty() {
            return Err(CatalogError::IncompleteScan(scan.warnings));
        }
        let root = fs::canonicalize(selected_root)?;
        if self.database_path.starts_with(&root) {
            return Err(CatalogError::InvalidSource(
                "catalog database must be outside the indexed source"
            ));
        }
        let root_text = path_as_text(&root)?.to_owned();
        let skipped_links = scan.skipped_links;
        let skipped_mounts = scan.skipped_mounts;

        // Prepare/validate all paths before making any catalog changes.
        let mut records = Vec::with_capacity(scan.files.len());
        for file in scan.files {
            let relative = file.path.strip_prefix(selected_root).map_err(|_| {
                CatalogError::InvalidSource("file escaped selected source")
            })?;
            let absolute = root.join(relative);
            let path = path_as_text(&absolute)?.to_owned();
            let name = absolute.file_name().and_then(|v| v.to_str()).ok_or_else(|| {
                CatalogError::NonUnicodePath(absolute.clone())
            })?.to_owned();
            let size = i64::try_from(file.size_bytes).map_err(|_| {
                CatalogError::InvalidSource("file size exceeds catalog limits")
            })?;
            let modified = file.modified
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_secs()).ok());
            records.push((path, name, size, category_name(file.category), modified));
        }
        let count = records.len();
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO catalog_sources(root) VALUES (?1) ON CONFLICT(root) DO NOTHING",
            params![root_text],
        )?;
        let source_id: i64 = transaction.query_row(
            "SELECT id FROM catalog_sources WHERE root = ?1",
            params![root_text],
            |row| row.get(0),
        )?;
        transaction.execute(
            "DELETE FROM catalog_files WHERE source_id = ?1",
            params![source_id],
        )?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO catalog_files
                 (source_id, path, name, size_bytes, category, modified_unix_seconds)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
            )?;
            for (path, name, size, category, modified) in records {
                insert.execute(params![source_id, path, name, size, category, modified])?;
            }
        }
        transaction.commit()?;
        Ok(RefreshStats { indexed_files: count, skipped_links, skipped_mounts })
    }

    /// Search across all retained source snapshots. Results may describe files
    /// on currently disconnected or changed sources; no file I/O is performed.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<CatalogEntry>, CatalogError> {
        // User searches are literal substring matches, not SQL wildcard patterns.
        let escaped = query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let pattern = format!("%{escaped}%");
        let limit = i64::try_from(limit.min(1000)).unwrap_or(1000);
        let mut statement = self.connection.prepare(
            "SELECT f.path, s.root, f.size_bytes, f.category, f.modified_unix_seconds
             FROM catalog_files f
             JOIN catalog_sources s ON s.id = f.source_id
             WHERE f.name LIKE ?1 ESCAPE '\\' OR f.path LIKE ?1 ESCAPE '\\'
             ORDER BY f.name COLLATE NOCASE, f.path
             LIMIT ?2"
        )?;
        let rows = statement.query_map(params![pattern, limit], |row| {
            let size: i64 = row.get(2)?;
            let category: String = row.get(3)?;
            Ok(CatalogEntry {
                path: row.get(0)?,
                source_root: row.get(1)?,
                size_bytes: u64::try_from(size).unwrap_or(0),
                category: parse_category(&category),
                modified_unix_seconds: row.get(4)?,
            })
        })?;
        let mut entries = Vec::new();
        for row in rows { entries.push(row?); }
        Ok(entries)
    }

    pub fn file_count(&self) -> Result<u64, CatalogError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM catalog_files", [], |row| row.get(0)
        )?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

fn path_as_text(path: &Path) -> Result<&str, CatalogError> {
    path.to_str().ok_or_else(|| CatalogError::NonUnicodePath(path.to_path_buf()))
}

fn category_name(category: FileCategory) -> &'static str {
    match category {
        FileCategory::Photo => "photo",
        FileCategory::Video => "video",
        FileCategory::Audio => "audio",
        FileCategory::Document => "document",
        FileCategory::Archive => "archive",
        FileCategory::Other => "other",
    }
}
fn parse_category(category: &str) -> FileCategory {
    match category {
        "photo" => FileCategory::Photo,
        "video" => FileCategory::Video,
        "audio" => FileCategory::Audio,
        "document" => FileCategory::Document,
        "archive" => FileCategory::Archive,
        _ => FileCategory::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("savek-catalog-{}-{}", std::process::id(), nanos));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn indexes_searches_and_refreshes_without_changing_source_bytes() {
        let temporary = temp_dir();
        let root = temporary.join("photos");
        fs::create_dir(&root).unwrap();
        let original = root.join("Trip.JPG");
        fs::write(&original, b"keep original").unwrap();
        let mut catalog = Catalog::open(&temporary.join("library.sqlite")).unwrap();
        assert_eq!(catalog.refresh_source(&root).unwrap().indexed_files, 1);
        assert_eq!(catalog.file_count().unwrap(), 1);
        assert_eq!(catalog.search("trip", 20).unwrap()[0].category, FileCategory::Photo);
        assert_eq!(catalog.search("%", 20).unwrap().len(), 0);
        assert_eq!(fs::read(&original).unwrap(), b"keep original");
        fs::remove_file(&original).unwrap();
        assert_eq!(catalog.refresh_source(&root).unwrap().indexed_files, 0);
        assert_eq!(catalog.file_count().unwrap(), 0);
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn disconnected_source_keeps_last_snapshot() {
        let temporary = temp_dir();
        let root = temporary.join("nas");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("contract.pdf"), b"private").unwrap();
        let mut catalog = Catalog::open(&temporary.join("library.sqlite")).unwrap();
        catalog.refresh_source(&root).unwrap();
        fs::remove_dir_all(&root).unwrap();
        assert!(catalog.refresh_source(&root).is_err());
        assert_eq!(catalog.file_count().unwrap(), 1);
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn refuses_database_inside_indexed_folder() {
        let root = temp_dir();
        let mut catalog = Catalog::open(&root.join("library.sqlite")).unwrap();
        assert!(matches!(catalog.refresh_source(&root),
            Err(CatalogError::InvalidSource(_))));
        fs::remove_dir_all(root).unwrap();
    }
}
