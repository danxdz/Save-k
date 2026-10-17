//! Read-only metadata discovery for sources selected by the user.
//!
//! This initial module does not write to source files, follow directory links,
//! send data to a server, or attempt to guess NAS authentication settings.

pub mod catalog;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCategory {
    Photo,
    Video,
    Audio,
    Document,
    Archive,
    Other,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified: Option<SystemTime>,
    pub category: FileCategory,
}

#[derive(Debug, Clone)]
pub struct ScanWarning {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub files: Vec<FileEntry>,
    pub warnings: Vec<ScanWarning>,
    pub skipped_links: usize,
    pub skipped_mounts: usize,
}

/// Enumerate files below a user-selected directory.
///
/// Directory traversal does not follow symbolic links (including link-like
/// files). A failure below the root is reported as a warning and scanning
/// continues. Metadata may become stale when a source changes concurrently.
///
/// Important: this function is not an isolation/security boundary for hostile
/// concurrently modified filesystems; it is a read-only enumeration primitive.
pub fn scan_directory(root: &Path) -> io::Result<ScanReport> {
    let root_meta = fs::symlink_metadata(root)?;
    if is_link_like(&root_meta) || !root_meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "selected source is not a directory",
        ));
    }
    #[cfg(unix)]
    let root_device = {
        use std::os::unix::fs::MetadataExt;
        root_meta.dev()
    };
    let mut report = ScanReport::default();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let children = match fs::read_dir(&directory) {
            Ok(iter) => iter,
            Err(e) => {
                report.warnings.push(ScanWarning {
                    path: directory,
                    message: e.to_string(),
                });
                continue;
            }
        };
        for child in children {
            let child = match child {
                Ok(child) => child,
                Err(e) => {
                    report.warnings.push(ScanWarning {
                        path: directory.clone(),
                        message: e.to_string(),
                    });
                    continue;
                }
            };
            let path = child.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(e) => {
                    report.warnings.push(ScanWarning {
                        path,
                        message: e.to_string(),
                    });
                    continue;
                }
            };
            let ty = metadata.file_type();
            if is_link_like(&metadata) {
                report.skipped_links += 1;
            } else if ty.is_dir() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if metadata.dev() != root_device {
                        report.skipped_mounts += 1;
                        continue;
                    }
                }
                pending.push(path);
            } else if ty.is_file() {
                report.files.push(FileEntry {
                    category: category_for_path(&path),
                    path,
                    size_bytes: metadata.len(),
                    modified: metadata.modified().ok(),
                });
            }
        }
    }
    report.files.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    Ok(report)
}

fn is_link_like(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT: includes junctions, not just symlinks.
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

pub fn category_for_path(path: &Path) -> FileCategory {
    let extension = path.extension().and_then(|x| x.to_str()).unwrap_or("");
    match extension.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "heic" | "heif" | "tif" | "tiff" | "bmp" | "raw" | "dng" => FileCategory::Photo,
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" => FileCategory::Video,
        "mp3" | "m4a" | "flac" | "wav" | "ogg" | "aac" => FileCategory::Audio,
        "pdf" | "txt" | "md" | "docx" | "doc" | "xlsx" | "xls" | "pptx" | "ppt" | "odt" => FileCategory::Document,
        "zip" | "7z" | "rar" | "tar" | "gz" => FileCategory::Archive,
        _ => FileCategory::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("savek-test-{}-{}", std::process::id(), t));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn scans_files_without_mutating_originals() {
        let root = temp_dir();
        fs::create_dir(root.join("photos")).unwrap();
        let original = root.join("photos").join("holiday.JPG");
        {
            let mut f = fs::File::create(&original).unwrap();
            f.write_all(b"original photo bytes").unwrap();
        }
        let before = fs::read(&original).unwrap();
        let report = scan_directory(&root).unwrap();
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].category, FileCategory::Photo);
        assert_eq!(report.files[0].size_bytes, before.len() as u64);
        assert_eq!(fs::read(&original).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_missing_root() {
        let root = temp_dir();
        fs::remove_dir_all(&root).unwrap();
        assert!(scan_directory(&root).is_err());
    }

    #[test]
    fn recognizes_document_extension() {
        assert_eq!(category_for_path(Path::new("LETTER.PDF")), FileCategory::Document);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_as_root() {
        use std::os::unix::fs::symlink;
        let real = temp_dir();
        let parent = temp_dir();
        let link = parent.join("linked-root");
        symlink(&real, &link).unwrap();
        assert!(scan_directory(&link).is_err());
        fs::remove_dir_all(parent).unwrap();
        fs::remove_dir_all(real).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlink_to_another_directory() {
        use std::os::unix::fs::symlink;
        let root = temp_dir();
        let outside = temp_dir();
        fs::write(outside.join("private.pdf"), b"secret").unwrap();
        symlink(&outside, root.join("link")).unwrap();
        let report = scan_directory(&root).unwrap();
        assert_eq!(report.skipped_links, 1);
        assert!(report.files.is_empty());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
