//! Review-only UI: every action here is non-destructive. Source photos remain
//! untouched, and no matching decision is made automatically.
use super::{format_size, LibraryApp, Thumbnails, ACCENT, SOFT_TEXT};
use eframe::egui::{self, RichText, Sense};
use savek_core::duplicates::DuplicateGroup;
use std::path::Path;

impl LibraryApp {
    pub(super) fn duplicates_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Review duplicate photos");
        ui.label(RichText::new(
            "Read-only analysis. No files will be moved, overwritten, or deleted."
        ).color(ACCENT));
        ui.add_space(5.0);
        if self.duplicates_running {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Checking local photo contents. This can take a while…");
            });
            return;
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(185, 55, 45), error);
        }
        let Some(report) = self.duplicate_report.as_ref() else {
            ui.add_space(12.0);
            ui.label("Click Find duplicates above to analyze indexed photos.");
            ui.label(RichText::new(
                "Exact copies use BLAKE3; possible similar photos use a conservative visual fingerprint."
            ).small().color(SOFT_TEXT));
            return;
        };
        let total_groups = report.exact_groups.len() + report.visual_groups.len();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{} indexed photos checked", report.photo_count)).strong());
            ui.separator();
            ui.label(format!("{} exact groups", report.exact_groups.len()));
            ui.separator();
            ui.label(format!("{} possible visual groups", report.visual_groups.len()));
        });
        if report.unreadable_files > 0 || report.unsupported_previews > 0 {
            ui.label(RichText::new(format!(
                "{} files skipped for exact hashing; {} not decoded for visual comparison.",
                report.unreadable_files,
                report.unsupported_previews
            )).small().color(SOFT_TEXT));
        }
        if report.comparison_limit_reached {
            ui.colored_label(
                egui::Color32::from_rgb(172, 99, 32),
                "Visual analysis reached its comparison limit: suggestions are incomplete."
            );
        }
        ui.label(RichText::new(
            "Visual matches can be false positives, especially logos and CAD drawings. Always inspect before making any decision."
        ).small().color(SOFT_TEXT));
        ui.separator();
        if total_groups == 0 {
            ui.add_space(12.0);
            ui.label("No duplicates found by these matching methods.");
            return;
        }
        let visible = self.duplicate_groups_visible;
        egui::ScrollArea::vertical()
            .id_salt("duplicate_review_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut displayed = 0;
                for (group_no, group) in report.exact_groups.iter().enumerate() {
                    if displayed >= visible {
                        break;
                    }
                    draw_group(ui, &mut self.thumbnails, group, "Exact copies", group_no + 1);
                    displayed += 1;
                }
                for (group_no, group) in report.visual_groups.iter().enumerate() {
                    if displayed >= visible {
                        break;
                    }
                    draw_group(ui, &mut self.thumbnails, group, "Possible visual matches", group_no + 1);
                    displayed += 1;
                }
                if visible < total_groups {
                    ui.add_space(10.0);
                    if ui.button("Show 20 more groups").clicked() {
                        self.duplicate_groups_visible = self.duplicate_groups_visible.saturating_add(20);
                    }
                }
            });
    }
}

fn draw_group(
    ui: &mut egui::Ui,
    thumbnails: &mut Thumbnails,
    group: &DuplicateGroup,
    category: &str,
    number: usize,
) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{category} #{number}")).strong());
            ui.label(RichText::new(format!("{} files", group.photos.len())).small().color(SOFT_TEXT));
        });
        ui.horizontal_wrapped(|ui| {
            for photo in group.photos.iter().take(8) {
                ui.vertical(|ui| {
                    ui.set_width(142.0);
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(130.0, 90.0),
                        Sense::hover(),
                    );
                    thumbnails.paint(ui, &photo.path, rect);
                    let title = Path::new(&photo.path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unnamed file");
                    let shortened: String = title.chars().take(21).collect();
                    let title = if title.chars().count() > 21 {
                        format!("{shortened}…")
                    } else {
                        shortened
                    };
                    ui.label(RichText::new(title).small()).on_hover_text(&photo.path);
                    ui.label(RichText::new(format_size(photo.size_bytes)).small().color(SOFT_TEXT));
                    if ui.small_button("Copy path").clicked() {
                        ui.ctx().copy_text(photo.path.clone());
                    }
                });
            }
        });
        if group.photos.len() > 8 {
            ui.label(format!("… and {} more files in this candidate group.", group.photos.len() - 8));
        }
    });
    ui.add_space(8.0);
}
