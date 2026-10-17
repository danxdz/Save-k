# Manual local indexing prototype

This is **not** a complete Windows UI, NAS sync service, or secure vault.

## Prerequisites
- Rust stable and Cargo
- Selected folders accessible to the current operating-system user
- A separate local writable directory for the catalog database (not within a scanned folder)

## Try it (on Windows PowerShell)

```powershell
cargo test -p savek-core
cargo run -p savek-cli -- --db "$env:LOCALAPPDATA\\SaveK\\library.sqlite" scan "D:\\Photos"
cargo run -p savek-cli -- --db "$env:LOCALAPPDATA\\SaveK\\library.sqlite" search "holiday"
```

Create the `$env:LOCALAPPDATA\SaveK` directory first, if absent. For a mapped NAS drive, replace `D:\Photos` with the selected share folder. Do not scan a mapped share that you do not have permission to read.

## What this does
- Reads basic metadata (path, filename, size, timestamp, extension-derived category).
- Writes only to the **local** SQLite database you explicitly designate.
- Refreshes a single source in one transaction after a complete scan.
- If the selected source is disconnected/unreadable or scan warnings occur, the previous catalog snapshot remains.
- Search works against the offline catalog even when source files are not presently accessible.
- It never reads file bodies or changes, deletes, renames or copies indexed originals.

## Current limitations / follow-up tasks
- SQL metadata is plaintext, not encrypted. Treat the catalog as sensitive.
- This prototype is synchronous and collects the scan into memory; do **not** use on enormous or untrusted NAS trees yet. Background jobs, cancellation, resource limits and resilient incremental updates are still required.
- Only paths representable as Unicode are accepted for catalog records.
- Not a filesystem security boundary against concurrent malicious filesystem changes. Root symlinks, reparse points and Unix nested device boundaries are skipped as a best effort.
- On interrupted/partial scans, no snapshot is updated; a successful rescan may remove entries for files that were deleted from the source (only database records, not originals).
- No SMB discovery, credential handling, thumbnail generation, duplicate detection, SQLite migration versioning or GUI yet.
- Scan metadata may change as files are being modified. File IDs, thumbnail caches, privacy controls and further tests are outstanding.
- Windows build and tests **must** pass locally before this change can merge. This environment does not have a Rust toolchain. There is intentionally no GitHub Actions workflow.
