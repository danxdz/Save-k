//! Read-only, user-invoked photo duplicate analysis.
//!
//! "Exact" means matching BLAKE3 file content. "Visual" means possible matches
//! from a conservative 64-bit dHash fingerprint, NOT evidence that files are
//! interchangeable. No function in this module modifies or deletes originals.
use crate::catalog::CatalogEntry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

const MAX_VISUAL_IMAGE_BYTES: u64 = 40 * 1024 * 1024;
const MAX_SOURCE_ALLOC: u64 = 192 * 1024 * 1024;
const MAX_SOURCE_DIMENSION: u32 = 9000;
const MAX_CANDIDATE_COMPARISONS: usize = 300_000;
const MAX_DHASH_DISTANCE: u32 = 3;
const MAX_LUMA_DIFFERENCE: u8 = 24;

/// A metadata-only reference from the local catalog.
#[derive(Debug, Clone)]
pub struct Photo {
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    pub photos: Vec<Photo>,
}

#[derive(Debug, Default)]
pub struct DuplicateReport {
    pub photo_count: usize,
    pub exact_groups: Vec<DuplicateGroup>,
    pub visual_groups: Vec<DuplicateGroup>,
    pub unreadable_files: usize,
    pub unsupported_previews: usize,
    pub comparison_limit_reached: bool,
    pub checked_visual_candidates: usize,
}

#[derive(Debug, Clone, Copy)]
struct Fingerprint {
    hash: u64,
    width: u32,
    height: u32,
    mean_luma: u8,
}

/// Hash stored photo paths read-only, without following known link-like paths.
///
/// This function assumes the user explicitly requested duplicate analysis.
/// Catalog entries can become stale, and files can change while scanning.
/// Treat all reported *visual* groups as review suggestions only.
pub fn analyze_photos(entries: &[CatalogEntry]) -> DuplicateReport {
    let mut report = DuplicateReport::default();
    let mut unique_paths = HashSet::new();
    let mut photos = Vec::new();
    for entry in entries {
        if unique_paths.insert(entry.path.clone()) {
            photos.push(Photo {
                path: entry.path.clone(),
                size_bytes: entry.size_bytes,
            });
        }
    }
    report.photo_count = photos.len();

    // Hash only same-sized files for exact matches; hashing unique sizes is
    // unnecessary. Hashes are computed by streaming bytes from local files.
    let mut sizes: HashMap<u64, Vec<usize>> = HashMap::new();
    for (index, photo) in photos.iter().enumerate() {
        sizes.entry(photo.size_bytes).or_default().push(index);
    }
    let mut equal_content: BTreeMap<(u64, String), Vec<usize>> = BTreeMap::new();
    for (size, indices) in sizes {
        if indices.len() < 2 {
            continue;
        }
        for index in indices {
            let file = &photos[index];
            match hash_file(Path::new(&file.path), size) {
                Ok(Some(hash)) => {
                    equal_content.entry((size, hash.to_hex().to_string()))
                        .or_default()
                        .push(index);
                }
                Ok(None) | Err(_) => report.unreadable_files += 1,
            }
        }
    }
    for group in equal_content.into_values().filter(|group| group.len() > 1) {
        report.exact_groups.push(to_group(&photos, &group));
    }

    // 4 x 16-bit bands give a fast candidate shortlist. A dHash Hamming
    // distance <= 3 necessarily leaves at least one band unchanged.
    // We still compare aspect ratios and average luminance; these are
    // conservative suggestions and may miss cropped/rotated photographs.
    let mut union = DisjointSet::new(photos.len());
    let mut buckets: HashMap<(u8, u16), Vec<usize>> = HashMap::new();
    let mut same_hash_leaders: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut fingerprints: Vec<Option<Fingerprint>> = vec![None; photos.len()];

    for (index, photo) in photos.iter().enumerate() {
        let fp = match visual_fingerprint(Path::new(&photo.path)) {
            Ok(fp) => fp,
            Err(_) => {
                report.unsupported_previews += 1;
                continue;
            }
        };
        fingerprints[index] = Some(fp);

        // Thousands of identical tiny logos should not produce quadratic
        // comparisons or flood the candidate bucket vectors.
        let mut same_hash_representative = false;
        if let Some(leaders) = same_hash_leaders.get(&fp.hash) {
            for &leader in leaders {
                if let Some(other) = fingerprints[leader] {
                    if visually_compatible(fp, other) {
                        union.join(index, leader);
                        same_hash_representative = true;
                        break;
                    }
                }
            }
        }
        if same_hash_representative {
            continue;
        }

        let mut checked_this_image = HashSet::new();
        for band in 0..4_u8 {
            let bits = ((fp.hash >> (u32::from(band) * 16)) & 0xFFFF) as u16;
            if let Some(candidates) = buckets.get(&(band, bits)) {
                for &other_index in candidates {
                    if !checked_this_image.insert(other_index) {
                        continue;
                    }
                    if report.checked_visual_candidates >= MAX_CANDIDATE_COMPARISONS {
                        report.comparison_limit_reached = true;
                        break;
                    }
                    report.checked_visual_candidates += 1;
                    if let Some(other) = fingerprints[other_index] {
                        if (fp.hash ^ other.hash).count_ones() <= MAX_DHASH_DISTANCE
                            && visually_compatible(fp, other)
                        {
                            union.join(index, other_index);
                        }
                    }
                }
            }
            if report.comparison_limit_reached {
                break;
            }
        }
        same_hash_leaders.entry(fp.hash).or_default().push(index);
        for band in 0..4_u8 {
            let bits = ((fp.hash >> (u32::from(band) * 16)) & 0xFFFF) as u16;
            buckets.entry((band, bits)).or_default().push(index);
        }
        if report.comparison_limit_reached {
            break;
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for index in 0..photos.len() {
        if fingerprints[index].is_some() {
            let root = union.find(index);
            groups.entry(root).or_default().push(index);
        }
    }
    for group in groups.into_values().filter(|group| group.len() > 1) {
        report.visual_groups.push(to_group(&photos, &group));
    }
    report.exact_groups.sort_by_key(|g| std::cmp::Reverse(g.photos.len()));
    report.visual_groups.sort_by_key(|g| std::cmp::Reverse(g.photos.len()));
    report
}

fn to_group(photos: &[Photo], indices: &[usize]) -> DuplicateGroup {
    DuplicateGroup {
        photos: indices.iter().map(|&index| photos[index].clone()).collect(),
    }
}

fn is_link_like(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true; // Windows reparse point/junction.
        }
    }
    false
}

fn hash_file(path: &Path, expected_size: u64) -> io::Result<Option<blake3::Hash>> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link_like(&metadata) || !metadata.is_file() || metadata.len() != expected_size {
        return Ok(None);
    }
    let mut input = File::open(path)?;
    let before = input.metadata()?;
    if before.len() != expected_size {
        return Ok(None);
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let after = input.metadata()?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Ok(None);
    }
    Ok(Some(hasher.finalize()))
}

fn visual_fingerprint(path: &Path) -> Result<Fingerprint, &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "unavailable")?;
    if is_link_like(&metadata) || !metadata.is_file() {
        return Err("not a regular file");
    }
    if metadata.len() > MAX_VISUAL_IMAGE_BYTES {
        return Err("image too large");
    }
    let mut reader = image::ImageReader::open(path)
        .map_err(|_| "cannot open")?
        .with_guessed_format()
        .map_err(|_| "unrecognized format")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_DIMENSION);
    limits.max_image_height = Some(MAX_SOURCE_DIMENSION);
    limits.max_alloc = Some(MAX_SOURCE_ALLOC);
    reader.limits(limits);
    let photo = reader.decode().map_err(|_| "cannot decode")?;
    if photo.width() == 0 || photo.height() == 0 {
        return Err("empty image");
    }
    let small = photo.resize_exact(9, 8, image::imageops::FilterType::Triangle).to_luma8();
    let mut hash = 0_u64;
    let mut sum = 0_u32;
    for y in 0..8 {
        for x in 0..8 {
            let left = small.get_pixel(x, y)[0];
            let right = small.get_pixel(x + 1, y)[0];
            hash = (hash << 1) | u64::from(left > right);
            sum += u32::from(left);
        }
    }
    Ok(Fingerprint {
        hash,
        width: photo.width(),
        height: photo.height(),
        mean_luma: (sum / 64) as u8,
    })
}

fn visually_compatible(a: Fingerprint, b: Fingerprint) -> bool {
    if a.mean_luma.abs_diff(b.mean_luma) > MAX_LUMA_DIFFERENCE {
        return false;
    }
    // Compare aspect ratios without floating-point operations.
    let left = u64::from(a.width) * u64::from(b.height);
    let right = u64::from(b.width) * u64::from(a.height);
    let baseline = left.max(right).max(1);
    left.abs_diff(right).saturating_mul(100) <= baseline.saturating_mul(3)
}

struct DisjointSet { parents: Vec<usize>, sizes: Vec<usize> }
impl DisjointSet {
    fn new(n: usize) -> Self {
        Self { parents: (0..n).collect(), sizes: vec![1; n] }
    }
    fn find(&mut self, index: usize) -> usize {
        let mut current = index;
        while current != self.parents[current] {
            self.parents[current] = self.parents[self.parents[current]];
            current = self.parents[current];
        }
        current
    }
    fn join(&mut self, left: usize, right: usize) {
        let mut a = self.find(left);
        let mut b = self.find(right);
        if a == b { return; }
        if self.sizes[a] < self.sizes[b] {
            std::mem::swap(&mut a, &mut b);
        }
        self.parents[b] = a;
        self.sizes[a] += self.sizes[b];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileCategory;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary() -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("savek-duplicates-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(path: &Path) -> CatalogEntry {
        CatalogEntry {
            path: path.to_str().unwrap().to_owned(),
            source_root: path.parent().unwrap().to_str().unwrap().to_owned(),
            size_bytes: fs::metadata(path).unwrap().len(),
            category: FileCategory::Photo,
            modified_unix_seconds: None,
        }
    }

    #[test]
    fn identical_bytes_different_names_are_exact_copies() {
        let root = temporary();
        let a = root.join("original.jpg");
        let b = root.join("another-name.jpg");
        fs::write(&a, b"not a real image but identical bytes").unwrap();
        fs::write(&b, fs::read(&a).unwrap()).unwrap();
        let originals = fs::read(&a).unwrap();
        let report = analyze_photos(&[entry(&a), entry(&b)]);
        assert_eq!(report.exact_groups.len(), 1);
        assert_eq!(report.exact_groups[0].photos.len(), 2);
        assert!(report.visual_groups.is_empty());
        assert_eq!(fs::read(&a).unwrap(), originals);
        assert_eq!(fs::read(&b).unwrap(), originals);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_pixels_in_png_and_bmp_are_visual_candidates() {
        let root = temporary();
        let bmp = root.join("photo.bmp");
        let png = root.join("photo.png");
        let mut pixels = image::RgbImage::new(160, 120);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = image::Rgb([
                ((x * 7 + y) % 256) as u8,
                ((x + y * 3) % 256) as u8,
                ((x * 2 + y * 5) % 256) as u8,
            ]);
        }
        image::DynamicImage::ImageRgb8(pixels.clone()).save(&png).unwrap();
        image::DynamicImage::ImageRgb8(pixels).save(&bmp).unwrap();
        let report = analyze_photos(&[entry(&png), entry(&bmp)]);
        assert!(report.exact_groups.is_empty());
        assert_eq!(report.visual_groups.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_size_is_not_verified_as_exact_copy() {
        let root = temporary();
        let a = root.join("first.jpg");
        let b = root.join("second.jpg");
        fs::write(&a, b"abc").unwrap();
        fs::write(&b, b"abc").unwrap();
        let entries = [entry(&a), entry(&b)];
        fs::write(&b, b"modified after indexing").unwrap();
        let report = analyze_photos(&entries);
        assert!(report.exact_groups.is_empty());
        assert_eq!(report.unreadable_files, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn close_aspect_required() {
        let a = Fingerprint { hash: 1, width: 100, height: 100, mean_luma: 100 };
        let b = Fingerprint { hash: 1, width: 150, height: 100, mean_luma: 100 };
        assert!(!visually_compatible(a, b));
    }
}
