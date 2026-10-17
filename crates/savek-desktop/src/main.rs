//! Experimental Windows-first desktop UI.
//! Indexes only user-selected folders; never changes indexed originals.
use eframe::egui::{self, RichText};
use savek_core::catalog::{Catalog, CatalogEntry, RefreshStats};
use savek_core::FileCategory;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

const RESULT_LIMIT: usize = 1000;

enum JobEvent {
    ScanFinished(Result<RefreshStats, String>),
    SearchFinished {
        query: String,
        result: Result<Vec<CatalogEntry>, String>,
    },
}

struct LibraryApp {
    db_path: Option<PathBuf>,
    selected_folder: Option<PathBuf>,
    files: Vec<CatalogEntry>,
    query: String,
    filter: Option<FileCategory>,
    scan_running: bool,
    status: String,
    error: Option<String>,
    sender: Sender<JobEvent>,
    receiver: Receiver<JobEvent>,
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
            query: String::new(),
            filter: None,
            scan_running: false,
            status: "No folder selected. Choose a local folder to begin.".into(),
            error,
            sender,
            receiver,
        };
        app.start_search(ctx);
        app
    }

    fn start_scan(&mut self, ctx: &egui::Context) {
        if self.scan_running {
            return;
        }
        let (Some(path), Some(database)) = (
            self.selected_folder.clone(),
            self.db_path.clone(),
        ) else {
            self.error = Some("Choose a folder and ensure local catalog storage is available.".into());
            return;
        };
        self.scan_running = true;
        self.error = None;
        self.status = format!("Scanning {} (read-only)…", path.display());
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let outcome = Catalog::open(&database)
                .and_then(|mut catalog| catalog.refresh_source(&path))
                .map_err(|e| e.to_string());
            let _ = sender.send(JobEvent::ScanFinished(outcome));
            repaint.request_repaint();
        });
    }

    fn start_search(&mut self, ctx: &egui::Context) {
        let Some(database) = self.db_path.clone() else {
            return;
        };
        let query = self.query.trim().to_owned();
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = Catalog::open(&database)
                .and_then(|catalog| catalog.search(&query, RESULT_LIMIT))
                .map_err(|e| e.to_string());
            let _ = sender.send(JobEvent::SearchFinished { query, result });
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
                                "Indexed {} files. Skipped {} links and {} nested mounts.",
                                stats.indexed_files, stats.skipped_links, stats.skipped_mounts
                            );
                            self.error = None;
                            self.start_search(ctx);
                        }
                        Err(message) => {
                            self.error = Some(format!(
                                "Scan not saved: {message}. Previous catalog snapshots were kept."
                            ));
                            self.status = "Scan incomplete. Existing catalog preserved.".into();
                        }
                    }
                }
                JobEvent::SearchFinished { query, result } => {
                    // Ignore responses to older queries typed while jobs ran.
                    if query != self.query.trim() {
                        continue;
                    }
                    match result {
                        Ok(files) => {
                            self.files = files;
                        }
                        Err(message) => self.error = Some(format!("Catalog search: {message}")),
                    }
                }
            }
        }
    }

    fn choose_folder(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose a folder to index (read-only)")
            .pick_folder()
        {
            self.selected_folder = Some(folder);
            self.status = "Folder selected. Click Scan to index it.".into();
            self.error = None;
        }
    }

    fn controls(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("Save-K");
                ui.label(RichText::new("Private local library · experimental").weak());
            });
            ui.add_space(7.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Choose folder…").clicked() {
                    self.choose_folder();
                }
                if ui.add_enabled(!self.scan_running, egui::Button::new("Scan folder")).clicked() {
                    self.start_scan(ctx);
                }
                if self.scan_running {
                    ui.spinner();
                }
                if let Some(path) = &self.selected_folder {
                    ui.label(RichText::new(path.display().to_string()).small());
                }
            });
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label("Search:");
                let response = ui.add_sized(
                    [300.0, 25.0],
                    egui::TextEdit::singleline(&mut self.query).hint_text("Filename or path"),
                );
                let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if ui.button("Find").clicked() || enter {
                    self.start_search(ctx);
                }
                ui.label(RichText::new("Search is local; filenames are not uploaded.").weak().small());
            });
            ui.add_space(8.0);
        });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("category_sidebar")
            .default_width(155.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Library");
                ui.add_space(6.0);
                for (category, title) in [
                    (None, "All files"),
                    (Some(FileCategory::Photo), "Photos"),
                    (Some(FileCategory::Video), "Videos"),
                    (Some(FileCategory::Document), "Documents"),
                    (Some(FileCategory::Audio), "Audio"),
                    (Some(FileCategory::Archive), "Archives"),
                    (Some(FileCategory::Other), "Other"),
                ] {
                    ui.selectable_value(&mut self.filter, category, title);
                }
                ui.separator();
                ui.label(RichText::new("Files stay in their original folders.").small().weak());
                ui.label(RichText::new("Catalog metadata is currently unencrypted.").small().weak());
            });
    }

    fn library(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let visible: Vec<&CatalogEntry> = self.files.iter()
                .filter(|file| self.filter.is_none() || self.filter == Some(file.category))
                .collect();
            ui.horizontal(|ui| {
                ui.heading("Files");
                ui.label(format!("{} results (first {RESULT_LIMIT} maximum)", visible.len()));
            });
            ui.label(RichText::new(&self.status).weak());
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::from_rgb(200, 70, 60), error);
            }
            ui.separator();
            if visible.is_empty() {
                ui.label("No matching files. Select a folder and scan it, or change your search.");
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, 30.0, visible.len(), |ui, row_range| {
                    for row in row_range {
                        let file = visible[row];
                        ui.horizontal(|ui| {
                            ui.label(category_label(file.category));
                            ui.label(
                                RichText::new(
                                    Path::new(&file.path)
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .unwrap_or(&file.path)
                                ).strong()
                            );
                            ui.label(RichText::new(format_size(file.size_bytes)).weak());
                            if ui.small_button("Copy path").clicked() {
                                ui.ctx().copy_text(file.path.clone());
                            }
                        });
                        ui.label(RichText::new(&file.path).weak().small());
                        ui.separator();
                    }
                });
        });
    }
}

impl eframe::App for LibraryApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_events(ctx);
        self.controls(ctx);
        self.sidebar(ctx);
        self.library(ctx);
    }
}

fn category_label(category: FileCategory) -> &'static str {
    match category {
        FileCategory::Photo => "PHOTO",
        FileCategory::Video => "VIDEO",
        FileCategory::Audio => "AUDIO",
        FileCategory::Document => "DOC",
        FileCategory::Archive => "ZIP",
        FileCategory::Other => "FILE",
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
        .map_err(|e| format!("Cannot create local catalog directory: {e}"))?;
    Ok(folder.join("library.sqlite"))
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1050.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Save-K Local Library",
        options,
        Box::new(|cc| Ok(Box::new(LibraryApp::new(&cc.egui_ctx)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn human_readable_sizes() {
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(2048), "2.0 KB");
    }
}
