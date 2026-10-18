//! On-demand photo thumbnail decoding for the local desktop gallery.
//!
//! Images are read from user-selected files only. They are never modified,
//! uploaded, or stored on disk by this cache.
use eframe::egui::{self, Color32, ColorImage, FontId, Pos2, Rect, TextureHandle, TextureOptions};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};

const CACHE_CAP: usize = 96;
const MAX_PENDING: usize = 8;
const MAX_SOURCE_BYTES: u64 = 40 * 1024 * 1024;
const MAX_SOURCE_ALLOC: u64 = 192 * 1024 * 1024;
const DECODE_EDGE_LIMIT: u32 = 9000;

enum ThumbState {
    Queued,
    Loaded(TextureHandle),
    Unavailable(&'static str),
}
struct Request {
    generation: u64,
    path: String,
}
struct ResultMessage {
    generation: u64,
    path: String,
    image: Result<ColorImage, &'static str>,
}

/// Cache holding at most CACHE_CAP decoded thumbnails as GPU textures.
/// Requests are made only for visible photo cards. The worker is bounded
/// to a single decoder thread, with up to MAX_PENDING queued jobs.
pub struct Thumbnails {
    requests: SyncSender<Request>,
    results: Receiver<ResultMessage>,
    items: HashMap<String, ThumbState>,
    insertion_order: VecDeque<String>,
    pending: usize,
    generation: u64,
}

impl Thumbnails {
    pub fn new(ctx: &egui::Context) -> Self {
        let (request_tx, request_rx) = mpsc::sync_channel::<Request>(MAX_PENDING);
        let (result_tx, result_rx) = mpsc::channel::<ResultMessage>();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            while let Ok(request) = request_rx.recv() {
                let image = decode_thumbnail(Path::new(&request.path));
                let _ = result_tx.send(ResultMessage {
                    generation: request.generation,
                    path: request.path,
                    image,
                });
                repaint.request_repaint();
            }
        });
        Self {
            requests: request_tx,
            results: result_rx,
            items: HashMap::new(),
            insertion_order: VecDeque::new(),
            pending: 0,
            generation: 0,
        }
    }

    /// Invalidate previews after a scan so changed files don't retain old images.
    pub fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.items.clear();
        self.insertion_order.clear();
        self.pending = 0;
    }

    pub fn failure_reason(&self, path: &str) -> Option<&'static str> {
        match self.items.get(path) {
            Some(ThumbState::Unavailable(reason)) => Some(*reason),
            _ => None,
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(message) = self.results.try_recv() {
            if message.generation != self.generation {
                continue;
            }
            self.pending = self.pending.saturating_sub(1);
            let state = match message.image {
                Ok(image) => ThumbState::Loaded(ctx.load_texture(
                    format!("savek-thumb-{}", message.path),
                    image,
                    TextureOptions::LINEAR,
                )),
                Err(reason) => ThumbState::Unavailable(reason),
            };
            self.items.insert(message.path.clone(), state);
            self.insertion_order.push_back(message.path);
            while self.insertion_order.len() > CACHE_CAP {
                if let Some(old) = self.insertion_order.pop_front() {
                    self.items.remove(&old);
                }
            }
        }
    }

    /// Paints a thumbnail if available, otherwise a graceful placeholder.
    /// Image loading begins only once the rectangle is visible.
    pub fn paint(&mut self, ui: &egui::Ui, path: &str, rect: Rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, 6.0, Color32::from_rgb(230, 233, 238));
        if !ui.is_rect_visible(rect) {
            return;
        }
        if !self.items.contains_key(path) && self.pending < MAX_PENDING {
            if self.requests.try_send(Request {
                path: path.to_owned(),
                generation: self.generation,
            }).is_ok() {
                self.pending += 1;
                self.items.insert(path.to_owned(), ThumbState::Queued);
            }
        }
        match self.items.get(path) {
            Some(ThumbState::Loaded(texture)) => {
                let source = texture.size_vec2();
                if source.x > 0.0 && source.y > 0.0 {
                    let factor = (rect.width() / source.x).min(rect.height() / source.y);
                    let image_size = source * factor;
                    let image_rect = Rect::from_center_size(rect.center(), image_size);
                    painter.image(
                        texture.id(),
                        image_rect,
                        Rect::from_min_max(
                            egui::pos2(0.0, 0.0),
                            egui::pos2(1.0, 1.0),
                        ),
                        Color32::WHITE,
                    );
                }
            }
            Some(ThumbState::Queued) => {
                placeholder(painter, rect.center(), "Loading…");
            }
            Some(ThumbState::Unavailable(reason)) => {
                placeholder(painter, rect.center(), reason);
            }
            None => placeholder(painter, rect.center(), "Photo"),
        }
    }
}

fn placeholder(painter: &egui::Painter, center: Pos2, caption: &str) {
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        caption,
        FontId::proportional(13.0),
        Color32::from_rgb(116, 123, 137),
    );
}

/// The decoder runs off the UI thread. Both file size and decoder allocations
/// are capped as best effort; some image formats/decoders may ignore limits.
fn decode_thumbnail(path: &Path) -> Result<ColorImage, &'static str> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "File missing")?;
    if metadata.file_type().is_symlink() {
        return Err("Linked file not previewed");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Linked file not previewed");
        }
    }
    if !metadata.is_file() {
        return Err("Not a regular file");
    }
    if metadata.len() > MAX_SOURCE_BYTES {
        return Err("File over 40 MB");
    }
    let mut reader = image::ImageReader::open(path)
        .map_err(|_| "Cannot open")?
        .with_guessed_format()
        .map_err(|_| "Cannot detect image format")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(DECODE_EDGE_LIMIT);
    limits.max_image_height = Some(DECODE_EDGE_LIMIT);
    limits.max_alloc = Some(MAX_SOURCE_ALLOC);
    reader.limits(limits);
    // Do not enlarge small source images during thumbnail generation.
    let decoded = reader.decode().map_err(|err| match err {
        image::ImageError::Unsupported(_) => "Unsupported image encoding",
        image::ImageError::Decoding(_) => "Corrupt or unsupported image",
        image::ImageError::Limits(_) => "Image exceeds decoder limits",
        image::ImageError::IoError(_) => "Cannot read image bytes",
        _ => "Cannot decode image",
    })?;
    let target_width = decoded.width().min(320);
    let target_height = decoded.height().min(220);
    let thumbnail = decoded.thumbnail(target_width, target_height).to_rgba8();
    let size = [thumbnail.width() as usize, thumbnail.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, thumbnail.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_png_decodes_to_thumbnail() {
        let folder = std::env::temp_dir().join(format!(
            "savek-thumbnail-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("test.png");
        let pixels = image::RgbaImage::from_pixel(10, 7, image::Rgba([31, 77, 118, 255]));
        pixels.save(&path).unwrap();
        let image = decode_thumbnail(&path).expect("small PNG should decode");
        assert_eq!(image.size, [10, 7]);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn large_png_is_downscaled_within_preview_bounds() {
        let folder = std::env::temp_dir().join(format!(
            "savek-large-thumbnail-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("large.png");
        let pixels = image::RgbaImage::from_pixel(640, 480, image::Rgba([31, 77, 118, 255]));
        pixels.save(&path).unwrap();
        let image = decode_thumbnail(&path).expect("large PNG should decode");
        assert!(image.size[0] <= 320);
        assert!(image.size[1] <= 220);
        assert!(image.size[0] < 640);
        assert!(image.size[1] < 480);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn non_image_bytes_report_decode_error() {
        let folder = std::env::temp_dir().join(format!(
            "savek-invalid-thumbnail-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("invalid.png");
        std::fs::write(&path, b"not an actual PNG").unwrap();
        assert!(decode_thumbnail(&path).is_err());
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn tiny_fake_jpeg_is_reported_as_unsupported() {
        let root = std::env::temp_dir().join(format!(
            "savek-fake-jpg-{}", std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("looks-like-photo.jpg");
        std::fs::write(&file, b"not actually a JPEG").unwrap();
        assert!(decode_thumbnail(&file).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_file_has_no_thumbnail() {
        assert!(decode_thumbnail(Path::new("not-found-thumbnail-5993.jpg")).is_err());
    }
}
