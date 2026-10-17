# Picasa-style gallery prototype

Status: **draft prototype; Windows build and tests not yet verified**. This is not a released organizer and does not include NAS.

## What changed
- The Windows app starts in **Photos / Gallery** instead of a plain metadata table.
- Visual photo tile grid with adjustable tile size, click-to-select details panel and a switchable file list.
- Thumbnails for JPEG, PNG, WebP, BMP, GIF and TIFF, decoded **on demand** off the UI thread. HEIC/HEIF/RAW and video previews are not implemented and show placeholders.
- A bounded worker queue and an in-memory thumbnail cache capped at 96 image textures, with no thumbnail files written to disk.
- Files grouped by **filesystem modification month**, not EXIF photo-taken date; EXIF indexing is future work.
- SQL-backed **category-specific, paginated** searches, newest modified items first. First 120 matches load, then select **Load more files**; the old list effectively limited matches to 1,000 across every category.
- Originals remain in place. The catalog still stores filenames and paths unencrypted in the local user app-data folder; don't treat this as a secure vault.

## Try on Windows
Use a small test photo folder for a first run. The branch depends on the unmerged Windows UI and SQLite catalog work.

```powershell
cd "$env:USERPROFILE\Downloads\SaveK-New"
git fetch origin
git switch --track origin/feature/picasa-gallery-v1
cargo check -p savek-desktop
cargo test --workspace
cargo run -p savek-desktop
```

If that feature branch is already checked out, use `git switch feature/picasa-gallery-v1` instead of `git switch --track ...`.

## Quick manual validation
1. Select a folder containing JPEG/PNG photos and a PDF; scan it.
2. Open **Photos**; confirm real thumbnails render and month headings correspond to file modification times.
3. Select a tile; confirm image preview, original path and file details appear.
4. Change thumbnail size, swap Gallery/List and try search.
5. Test another category, then **Load more files** with a directory exceeding 120 photos. Validate the next page is not a duplicate and all returned files match the filter.
6. Confirm indexing and browsing do not alter original file hashes.
7. Test a corrupt image, a removed image, a huge image and an unsupported HEIC/RAW file: UI must remain usable, placeholders appear.
8. With a large folder like 29,520 files, confirm UI stays responsive. NOTE: scanning itself remains synchronous in a worker and noncancelable; catalog scan collects metadata in RAM.

## Current limitations
- No EXIF date taken, real album management, map, favorites, video frame thumbnails, photo editor or full-screen zoom.
- Gallery layout groups the currently loaded pages; month headings can repeat as later pages load.
- SQLite metadata/paths are plaintext. No backup, encrypted vault, mobile apps, PDF utilities or QR tools are included in this slice.
- Large image decoders have best-effort allocation limits, not a memory-security guarantee. Use only trusted local photos during early testing.
- The background scanner is still memory-bound and non-cancelable; avoid huge/untrusted sources until these safeguards are implemented.
- This PR builds on the Windows UI branch and its catalog dependency, not directly on `dev`.
- No Rust compiler is installed in the tool environment; successful Rust compilation on Windows must be demonstrated by the user before merging.

Monday work item: https://adapteit.monday.com/boards/5105974441/pulses/3281475926
