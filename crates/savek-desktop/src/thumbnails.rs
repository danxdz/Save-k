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
    Unavailable,
}
struct Request {
    generation: u64,
    path: String,
}
struct ResultMessage {
    generation: u64,
    path: String,
    image: Option<ColorImage>,
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

    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(message) = self.results.try_recv() {
            if message.generation != self.generation {
                continue;
            }
            self.pending = self.pending.saturating_sub(1);
            let state = match message.image {
                Some(image) => ThumbState::Loaded(ctx.load_texture(
                    format!("savek-thumb-{}", message.path),
                    image,
                    TextureOptions::LINEAR,
                )),
                None => ThumbState::Unavailable,
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
            Some(ThumbState::Unavailable) => {
                placeholder(painter, rect.center(), "No preview");
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
fn decode_thumbnail(path: &Path) -> Option<ColorImage> {
    if std::fs::metadata(path).ok()?.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(DECODE_EDGE_LIMIT);
    limits.max_image_height = Some(DECODE_EDGE_LIMIT);
    limits.max_alloc = Some(MAX_SOURCE_ALLOC);
    reader.limits(limits);
    // image::DynamicImage::thumbnail may enlarge tiny inputs when the
    // requested bounds are larger than the source. Never upscale the
    // decoded bitmap; scaling for display belongs in the UI renderer.
    let decoded = reader.decode().ok()?;
    let target_width = decoded.width().min(320);
    let target_height = decoded.height().min(220);
    let thumbnail = decoded.thumbnail(target_width, target_height).to_rgba8();
    let size = [thumbnail.width() as usize, thumbnail.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, thumbnail.as_raw()))
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
    fn unavailable_file_has_no_thumbnail() {
        assert!(decode_thumbnail(Path::new("not-found-thumbnail-5993.jpg")).is_none());
    }
}
