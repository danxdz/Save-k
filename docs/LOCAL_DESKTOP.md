# Save-K Windows library prototype

**Status:** experimental code, not a release build. This is the first GUI slice of the organizer, not the full Save-K 1.0 MVP.

## Requirements
- Windows 10/11 with access to user-selected local folders
- Rust stable and Cargo
- No login, server, NAS or cloud account

## Build and run
From the repository root on the feature branch containing both the UI and catalog:
```powershell
cargo fmt --check
cargo test --workspace
cargo run -p savek-desktop
```
Choose folder, Scan folder, Find by filename or path, filter by category, or use Copy path.

## Privacy and safety
- The scan reads filesystem metadata, not file bodies.
- It does not rename, delete, encrypt or alter source files.
- Its only persistent storage is a SQLite file in the current user's local application-data directory at SaveK/library.sqlite.
- **File names and paths in the catalog are plaintext**. The encrypted vault is not implemented.
- No ad SDK, analytics, cloud service or upload of indexed metadata.

## Known limitations
- Searchable list only. No image thumbnails, Picasa timeline or file preview yet.
- No backup/restore, vault, PDF/image editor, QR utility, mobile apps or installer yet.
- Scan runs on a worker thread but still collects entries in memory and cannot be canceled. **Use small local test folders only**.
- Search shows up to 1,000 results. Sources require manual rescans.
- Inaccessible paths may fail; a failed or incomplete scan must preserve the prior catalog snapshot.
- Filesystem changes during scanning or unusual path encodings are not robustly supported.
- No NAS integration for MVP. Windows build/tests and hands-on GUI checks are required before merge.

## Windows acceptance checklist
1. Open the app with a clean user profile and choose a local test folder containing JPG, MP4 and PDF files.
2. Scan and verify the items appear in correct categories; test filename/path searches and Copy path.
3. Hash sample original files before/after scan and verify hashes are unchanged.
4. Close/reopen app; verify existing catalog results remain searchable.
5. Unmount a test removable drive after a successful scan, try to rescan, and ensure retained results survive.
6. Try denied permissions and symbolic links/junctions with no change to the original files.
7. Run Cargo formatting, targeted tests and GUI smoke tests on Windows.

Related Monday task: https://adapteit.monday.com/boards/5105974441/pulses/3281471238
