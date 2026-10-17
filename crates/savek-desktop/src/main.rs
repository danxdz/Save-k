//! Experimental Save-K visual library: a Picasa-inspired, local-only gallery.
//! No scanned file is moved, renamed, modified or uploaded.
mod thumbnails;
mod duplicate_view;

use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Vec2};
use savek_core::catalog::{Catalog, CatalogEntry, RefreshStats};
use savek_core::FileCategory;
use savek_core::duplicates::{analyze_photos, DuplicateReport};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use thumbnails::Thumbnails;

const PAGE_SIZE: usize = 120;
const ACCENT: Color32 = Color32::from_rgb(39, 119, 107);
const SOFT_TEXT: Color32 = Color32::from_rgb(118, 125, 137);
const DARK_TEXT: Color32 = Color32::from_rgb(43, 53, 62);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Gallery,
    List,
}

enum JobEvent {
    ScanFinished(Result<RefreshStats, String>),
    SearchFinished {
        token: u64,
        offset: usize,
        result: Result<Vec<CatalogEntry>, String>,
    },
    DuplicatesFinished(Result<DuplicateReport, String>),
}

struct LibraryApp {
    db_path: Option<PathBuf>,
    selected_folder: Option<PathBuf>,
    files: Vec<CatalogEntry>,
    selected_file: Option<CatalogEntry>,
    show_duplicates: bool,
    duplicates_running: bool,
    duplicate_report: Option<DuplicateReport>,
    duplicate_groups_visible: usize,
    query: String,
    filter: Option<FileCategory>,
    view_mode: ViewMode,
    tile_width: f32,
    scan_running: bool,
    page_loading: bool,
    has_more: bool,
    search_token: u64,
    status: String,
    error: Option<String>,
    sender: Sender<JobEvent>,
    receiver: Receiver<JobEvent>,
    thumbnails: Thumbnails,
}

impl LibraryApp {
    fn new(ctx: &egui::Context) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (db_path, error) = match local_catalog_path() {
            Ok(path) => (Some(path), None),
            Err(message) => (None, Some(message)),
        };
        let mut app = Self {
            db_path,
            selected_folder: None,
            files: Vec::new(),
            selected_file: None,
            show_duplicates: false,
            duplicates_running: false,
            duplicate_report: None,
            duplicate_groups_visible: 20,
            query: String::new(),
            filter: Some(FileCategory::Photo),
            view_mode: ViewMode::Gallery,
            tile_width: 174.0,
            scan_running: false,
            page_loading: false,
            has_more: false,
            search_token: 0,
            status: "Choose a folder to add photos to your local library.".into(),
            error,
            sender,
            receiver,
            thumbnails: Thumbnails::new(ctx),
        };
        app.start_search(ctx, false);
        app
    }

    fn start_scan(&mut self, ctx: &egui::Context) {
        if self.scan_running {
            return;
        }
        let (Some(folder), Some(database)) = (
            self.selected_folder.clone(),
            self.db_path.clone(),
        ) else {
            self.error = Some("Select a folder and ensure local catalog storage is available.".into());
            return;
        };
        self.scan_running = true;
        self.duplicate_report = None;
        self.error = None;
        self.status = format!("Indexing {} without changing original files…", folder.display());
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let outcome = Catalog::open(&database)
                .and_then(|mut catalog| catalog.refresh_source(&folder))
                .map_err(|error| error.to_string());
            let _ = sender.send(JobEvent::ScanFinished(outcome));
            repaint.request_repaint();
        });
    }

    fn start_duplicate_analysis(&mut self, ctx: &egui::Context) {
        if self.duplicates_running || self.scan_running {
            return;
        }
        let Some(database) = self.db_path.clone() else {
            self.error = Some("Local catalog is not accessible.".into());
            return;
        };
        self.duplicates_running = true;
        self.show_duplicates = true;
        self.selected_file = None;
        self.duplicate_report = None;
        self.duplicate_groups_visible = 20;
        self.error = None;
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = Catalog::open(&database)
                .and_then(|catalog| catalog.photos_for_duplicate_review(50_000))
                .map(|photos| analyze_photos(&photos))
                .map_err(|error| error.to_string());
            let _ = sender.send(JobEvent::DuplicatesFinished(result));
            repaint.request_repaint();
        });
    }

    fn start_search(&mut self, ctx: &egui::Context, append: bool) {
        if append && (self.page_loading || !self.has_more) {
            return;
        }
        let Some(database) = self.db_path.clone() else {
            return;
        };
        let offset = if append { self.files.len() } else { 0 };
        if !append {
            self.files.clear();
            self.has_more = false;
            self.selected_file = None;
        }
        self.search_token = self.search_token.wrapping_add(1);
        let token = self.search_token;
        let query = self.query.trim().to_owned();
        let category = self.filter;
        self.page_loading = true;
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = Catalog::open(&database)
                .and_then(|catalog| catalog.search_filtered(&query, category, offset, PAGE_SIZE))
                .map_err(|error| error.to_string());
            let _ = sender.send(JobEvent::SearchFinished { token, offset, result });
            repaint.request_repaint();
        });
    }

    fn receive_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                JobEvent::ScanFinished(result) => {
                    self.scan_running = false;
                    match result {
                        Ok(stats) => {
                            self.status = format!(
                                "{} files indexed. Originals unchanged. Skipped {} links, {} mounts.",
                                stats.indexed_files, stats.skipped_links, stats.skipped_mounts
                            );
                            self.error = None;
                            self.thumbnails.clear();
                            self.start_search(ctx, false);
                        }
                        Err(message) => {
                            self.error = Some(format!(
                                "Scan failed: {message}. Previous local catalog kept."
                            ));
                            self.status = "No files were moved or deleted.".into();
                        }
                    }
                }
                JobEvent::DuplicatesFinished(result) => {
                    self.duplicates_running = false;
                    match result {
                        Ok(report) => {
                            self.duplicate_report = Some(report);
                            self.error = None;
                        }
                        Err(message) => {
                            self.error = Some(format!("Duplicate review failed: {message}"));
                        }
                    }
                }
                JobEvent::SearchFinished { token, offset, result } => {
                    // Results from a previous query/filter must never replace a newer page.
                    if token != self.search_token {
                        continue;
                    }
                    self.page_loading = false;
                    match result {
                        Ok(page) => {
                            self.has_more = page.len() == PAGE_SIZE;
                            if offset == 0 {
                                self.files = page;
                                if !self.files.is_empty()
                                    && self.selected_folder.is_none()
                                    && self.status.starts_with("Choose a folder")
                                {
                                    self.status = "Showing previously indexed local files. Add a folder to scan more.".into();
                                }
                            } else {
                                self.files.extend(page);
                            }
                            self.error = None;
                        }
                        Err(message) => {
                            self.has_more = false;
                            self.error = Some(format!("Catalog search failed: {message}"));
                        }
                    }
                }
            }
        }
    }

    fn choose_folder(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose a local folder to index (read-only)")
            .pick_folder()
        {
            self.selected_folder = Some(folder);
            self.status = "Folder selected. Click Scan to add it to your library.".into();
            self.error = None;
        }
    }

    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("savek_header")
            .exact_height(123.0)
            .show(ctx, |ui| {
                ui.add_space(9.0);
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("Save-K").size(24.0).color(ACCENT).strong());
                    ui.label(RichText::new("PHOTO & FILE LIBRARY").size(11.0).color(SOFT_TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("LOCAL ONLY").strong().color(ACCENT).small());
                    });
                });
                ui.add_space(3.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Add folder…").clicked() {
                        self.choose_folder();
                    }
                    if ui.add_enabled(
                        !self.scan_running && !self.duplicates_running,
                        egui::Button::new("Scan selected folder")
                    ).clicked() {
                        self.start_scan(ctx);
                    }
                    if ui.add_enabled(
                        !self.duplicates_running && !self.scan_running,
                        egui::Button::new("Find duplicates"),
                    ).clicked() {
                        self.start_duplicate_analysis(ctx);
                    }
                    if self.scan_running || self.duplicates_running {
                        ui.spinner();
                    }
                    if let Some(folder) = &self.selected_folder {
                        ui.label(RichText::new(folder.display().to_string()).small().color(SOFT_TEXT));
                    }
                });
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label("Search");
                    let input = ui.add_sized(
                        [275.0, 24.0],
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Photo name, document, folder…"),
                    );
                    let enter = input.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if ui.button("Find").clicked() || enter {
                        self.start_search(ctx, false);
                    }
                    ui.separator();
                    ui.selectable_value(&mut self.view_mode, ViewMode::Gallery, "Gallery");
                    ui.selectable_value(&mut self.view_mode, ViewMode::List, "List");
                    if self.view_mode == ViewMode::Gallery {
                        ui.add(
                            egui::Slider::new(&mut self.tile_width, 130.0..=240.0)
                                .show_value(false)
                                .text("Size")
                        );
                    }
                });
            });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("library_navigation")
            .default_width(170.0)
            .min_width(160.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(13.0);
                ui.label(RichText::new("YOUR LIBRARY").strong().color(SOFT_TEXT).small());
                ui.add_space(12.0);
                for (category, label) in [
                    (Some(FileCategory::Photo), "Photos"),
                    (Some(FileCategory::Video), "Videos"),
                    (Some(FileCategory::Document), "Documents"),
                    (Some(FileCategory::Audio), "Audio"),
                    (Some(FileCategory::Archive), "Archives"),
                    (Some(FileCategory::Other), "Other files"),
                    (None, "All files"),
                ] {
                    if ui.selectable_label(!self.show_duplicates && self.filter == category, label).clicked() {
                        if self.filter != category || self.show_duplicates {
                            self.filter = category;
                            self.show_duplicates = false;
                            self.start_search(ctx, false);
                        }
                    }
                    ui.add_space(3.0);
                }
                ui.add_space(8.0);
                ui.separator();
                if ui.selectable_label(self.show_duplicates, "Review duplicates").clicked() {
                    self.show_duplicates = true;
                    self.selected_file = None;
                }
                ui.add_space(14.0);
                ui.separator();
                ui.label(RichText::new("PRIVATE BY DESIGN").color(ACCENT).strong().small());
                ui.label(RichText::new(
                    "Organize and preview without moving your originals."
                ).small().color(SOFT_TEXT));
                ui.add_space(8.0);
                ui.label(RichText::new(
                    "Catalog paths aren't encrypted yet."
                ).small().color(SOFT_TEXT));
            });
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        let Some(entry) = self.selected_file.clone() else {
            return;
        };
        egui::SidePanel::right("details")
            .default_width(250.0)
            .min_width(225.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.heading("Details");
                    if ui.small_button("Close").clicked() {
                        self.selected_file = None;
                    }
                });
                ui.add_space(8.0);
                let width = ui.available_width().min(240.0);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(width, 175.0),
                    Sense::hover(),
                );
                if entry.category == FileCategory::Photo {
                    self.thumbnails.paint(ui, &entry.path, rect);
                } else {
                    generic_preview(ui, entry.category, rect);
                }
                ui.add_space(14.0);
                let name = Path::new(&entry.path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Unnamed file");
                ui.label(RichText::new(name).strong().size(15.0));
                ui.add_space(8.0);
                ui.label(format!("Type: {}", category_name(entry.category)));
                ui.label(format!("Size: {}", format_size(entry.size_bytes)));
                ui.label(format!("Modified: {}", date_label(entry.modified_unix_seconds)));
                ui.add_space(8.0);
                ui.label(RichText::new(&entry.path).small().color(SOFT_TEXT));
                ui.add_space(8.0);
                if ui.button("Copy file path").clicked() {
                    ui.ctx().copy_text(entry.path.clone());
                }
                ui.separator();
                ui.label(RichText::new(
                    "This is a preview; your file hasn't been altered."
                ).small().color(SOFT_TEXT));
            });
    }

    fn contents(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(Color32::from_rgb(249, 250, 252)))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                if self.show_duplicates {
                    self.duplicates_page(ui);
                    return;
                }
                ui.horizontal(|ui| {
                    let section = match self.filter {
                        Some(FileCategory::Photo) => "Photos",
                        Some(FileCategory::Video) => "Videos",
                        Some(FileCategory::Document) => "Documents",
                        Some(FileCategory::Audio) => "Audio",
                        Some(FileCategory::Archive) => "Archives",
                        Some(FileCategory::Other) => "Other files",
                        None => "All files",
                    };
                    ui.heading(RichText::new(section).size(22.0).color(DARK_TEXT));
                    ui.label(RichText::new(format!(
                        "{} loaded{}", self.files.len(),
                        if self.has_more { " · more available" } else { "" }
                    )).small().color(SOFT_TEXT));
                    if self.page_loading {
                        ui.spinner();
                    }
                });
                ui.label(RichText::new(&self.status).small().color(SOFT_TEXT));
                if self.view_mode == ViewMode::Gallery {
                    ui.label(RichText::new("Grouped by file modification month. Camera capture dates are not indexed yet.").small().color(SOFT_TEXT));
                }
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::from_rgb(186, 55, 50), error);
                }
                ui.separator();
                if self.files.is_empty() {
                    ui.add_space(24.0);
                    if self.page_loading {
                        ui.label("Finding local files…");
                    } else {
                        ui.label("No files yet. Choose and scan a local folder.");
                    }
                    return;
                }
                match self.view_mode {
                    ViewMode::Gallery => self.gallery(ui, ctx),
                    ViewMode::List => self.list(ui, ctx),
                }
            });
    }

    fn gallery(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let files = self.files.clone();
        let groups = month_groups(&files);
        let width = self.tile_width;
        let height = 0.78 * width + 50.0;
        let columns = (((ui.available_width() + 12.0) / (width + 12.0)).floor()
            as usize).max(1);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(7.0);
                for (month, indices) in groups {
                    ui.label(RichText::new(month).strong().size(17.0).color(DARK_TEXT));
                    ui.add_space(7.0);
                    for chunk in indices.chunks(columns) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            for &index in chunk {
                                let file = &files[index];
                                let (rect, response) = ui.allocate_exact_size(
                                    Vec2::new(width, height),
                                    Sense::click(),
                                );
                                if ui.is_rect_visible(rect) {
                                    let selected = self.selected_file.as_ref()
                                        .is_some_and(|current| current.path == file.path);
                                    let background = if selected {
                                        Color32::from_rgb(222, 242, 237)
                                    } else {
                                        Color32::WHITE
                                    };
                                    ui.painter().rect_filled(rect, 8.0, background);
                                    if selected {
                                        ui.painter().rect_stroke(
                                            rect, 8.0, Stroke::new(2.0, ACCENT),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    let preview_rect = Rect::from_min_size(
                                        rect.min + egui::vec2(7.0, 7.0),
                                        egui::vec2(width - 14.0, width * 0.70),
                                    );
                                    if file.category == FileCategory::Photo {
                                        self.thumbnails.paint(ui, &file.path, preview_rect);
                                    } else {
                                        generic_preview(ui, file.category, preview_rect);
                                    }
                                    let title = Path::new(&file.path)
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .unwrap_or("Unnamed file");
                                    let max_chars = ((width - 20.0) / 7.0).floor() as usize;
                                    ui.painter().text(
                                        egui::pos2(rect.min.x + 10.0, preview_rect.max.y + 10.0),
                                        Align2::LEFT_TOP,
                                        truncate(title, max_chars.max(10)),
                                        FontId::proportional(12.0),
                                        DARK_TEXT,
                                    );
                                }
                                if response.clicked() {
                                    self.selected_file = Some(file.clone());
                                }
                                response.on_hover_text(&file.path);
                            }
                        });
                        ui.add_space(10.0);
                    }
                    ui.add_space(13.0);
                }
                self.more_button(ui, ctx);
            });
    }

    fn list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let files = self.files.clone();
        self.more_button(ui, ctx);
        egui::ScrollArea::vertical()
            .id_salt("file_list")
            .show_rows(ui, 28.0, files.len(), |ui, rows| {
                for index in rows {
                    let file = &files[index];
                    ui.horizontal(|ui| {
                        if ui.selectable_label(
                            self.selected_file.as_ref()
                                .is_some_and(|entry| entry.path == file.path),
                            Path::new(&file.path)
                                .file_name()
                                .and_then(|value| value.to_str())
                                .unwrap_or(&file.path),
                        ).clicked() {
                            self.selected_file = Some(file.clone());
                        }
                        ui.label(RichText::new(category_name(file.category))
                            .small().color(SOFT_TEXT));
                        ui.label(RichText::new(format_size(file.size_bytes))
                            .small().color(SOFT_TEXT));
                        ui.label(RichText::new(date_label(file.modified_unix_seconds))
                            .small().color(SOFT_TEXT));
                    });
                }
            });
    }

    fn more_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.has_more || self.page_loading {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(
                    self.has_more && !self.page_loading,
                    egui::Button::new("Load more files"),
                ).clicked() {
                    self.start_search(ctx, true);
                }
                if self.page_loading {
                    ui.spinner();
                }
            });
            ui.add_space(15.0);
        }
    }
}

impl eframe::App for LibraryApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.thumbnails.poll(ctx);
        self.receive_events(ctx);
        self.header(ctx);
        self.sidebar(ctx);
        self.inspector(ctx);
        self.contents(ctx);
    }
}

fn generic_preview(ui: &egui::Ui, category: FileCategory, rect: Rect) {
    ui.painter().rect_filled(rect, 6.0, Color32::from_rgb(229, 235, 239));
    if ui.is_rect_visible(rect) {
        ui.painter().text(
            rect.center(), Align2::CENTER_CENTER, category_name(category),
            FontId::proportional(19.0), Color32::from_rgb(101, 113, 124),
        );
    }
}

fn category_name(category: FileCategory) -> &'static str {
    match category {
        FileCategory::Photo => "Photo",
        FileCategory::Video => "Video",
        FileCategory::Audio => "Audio",
        FileCategory::Document => "Document",
        FileCategory::Archive => "Archive",
        FileCategory::Other => "File",
    }
}

fn month_groups(files: &[CatalogEntry]) -> Vec<(String, Vec<usize>)> {
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let label = month_label(file.modified_unix_seconds);
        match groups.last_mut() {
            Some((month, indices)) if *month == label => indices.push(index),
            _ => groups.push((label, vec![index])),
        }
    }
    groups
}

fn month_label(timestamp: Option<i64>) -> String {
    timestamp
        .and_then(|seconds| chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0))
        .map(|time| time.format("%B %Y").to_string())
        .unwrap_or_else(|| "Unknown modification date".into())
}

fn date_label(timestamp: Option<i64>) -> String {
    timestamp
        .and_then(|seconds| chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0))
        .map(|time| time.format("%d %b %Y").to_string())
        .unwrap_or_else(|| "Unknown".into())
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_owned()
    } else {
        let leading: String = value.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{leading}…")
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn local_catalog_path() -> Result<PathBuf, String> {
    let folder = dirs::data_local_dir()
        .ok_or("Could not locate local application data directory")?
        .join("SaveK");
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("Cannot create local catalog directory: {error}"))?;
    Ok(folder.join("library.sqlite"))
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 780.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Save-K — Local Photo Library",
        options,
        Box::new(|cc| {
            let mut visuals = egui::Visuals::light();
            visuals.selection.bg_fill = Color32::from_rgb(193, 228, 217);
            visuals.selection.stroke = Stroke::new(1.0, ACCENT);
            cc.egui_ctx.set_visuals(visuals);
            Ok(Box::new(LibraryApp::new(&cc.egui_ctx)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_truncated_on_unicode_character_boundaries() {
        assert_eq!(truncate("short", 8), "short");
        assert_eq!(truncate("abécdé", 4), "abé…");
    }
    #[test]
    fn modification_dates_group_correctly() {
        assert_eq!(month_label(Some(0)), "January 1970");
        assert_eq!(month_label(None), "Unknown modification date");
    }
}
