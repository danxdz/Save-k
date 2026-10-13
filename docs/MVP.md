# Save-K MVP definition (2026-10-13)

## Product goal
A fast, approachable, local-first library to organize existing photos, videos and documents across selected Windows folders, removable disks and mounted NAS/SMB shares. Save-K must preserve originals and must never silently restructure a user's directories.

## Phase A: Safe library foundation (first demonstrable slice)
- Choose one or more source folders explicitly; allow mapped SMB/NAS folders as ordinary paths.
- Enumerate files without modifying them, following **no symlinks**.
- Record file path, size and best-effort modification time; category by extension for initial browsing.
- Report unreadable files and directories as warnings, without aborting entire scan.
- Tests proving indexing does not alter, rename or delete originals, does not traverse symbolic links and handles disconnected sources.

Acceptance: a Rust library with a read-only file scanner, automated targeted tests and a runnable demo/UI consumer in a later task. No claim of complete NAS functionality yet.

## Phase B: Usable Windows library
- SQLite catalog on the user's PC and resumable background indexing.
- On-device thumbnails, timeline, search, favorite tags and offline-source status.
- Search across sources without moving originals; remove a catalog source without deleting files.
- Duplicate recommendations must not delete, rename or move files without a review/approval step.

## Phase C: Backup and private vault
- Reliable incremental backup with separate integrity verification and restore drills.
- Authenticated encryption, key management/recovery, and secure temporary-file handling.
- Investigate legacy password/SKR1 unlock behavior only when original implementation and test fixtures are available.
- No silent migration, and never re-encrypt source data in place.

## Phase D: Local toolbox
- Non-destructive PDF/image watermark templates and recipient-specific exports.
- Safe PDF merge/split and **true redaction**, not visual overlays that leave recoverable text.
- Image conversion, resize, metadata removal; explicit output destination and preview.

## Phase E: Mobile + growth
- Android and iOS file-selection workflows, local library browsing and secure exports with OS-specific permissions.
- Shared Rust core where practical; mobile UI/toolchain to be selected by spike.
- Core offline tools free; ads only in compliant optional contexts, no private file metadata to ad networks.
- Web acquisition and optional cloud/AI premium functions only after privacy and unit economics are reviewed.

## Non-goals for the MVP
Full remote NAS browsing, AI photo search, cloud sync, automatic duplicate deletion, destructive bulk organization, third-party API integration, ad-gated access to files, production deployment.

## Done criteria
Targeted tests passing, inspectable code review, actual device/system validation as appropriate, user data safe under failure scenarios, Monday task updated. A PR or drafted specification alone does not make an implementation complete.
