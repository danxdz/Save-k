# Save-K 1.0 MVP scope — revised 2026-10-17

## Status and ownership
**Not ready / not released.** The repository contains a basic Rust file scanner and a draft local SQLite catalog (PR #1) with no validated Windows/mobile app. This document is the product target, not evidence of implemented functionality.

Approved project: [danxdz/Save-k](https://github.com/danxdz/Save-k). Monday source of truth: [syncSave / Save-K Full App](https://adapteit.monday.com/boards/5105974441).

## Decision
Ship a **local-first Windows, Android, and iOS utility app** with a useful first version of all core modules below. **Defer all NAS-specific integration** until after the 1.0 MVP. No network share discovery, SMB credentials, NAS background sync or NAS companion in 1.0.

## Required user-facing modules (each must be usable, not a placeholder)

1. **Library / organizer (Picasa-inspired)**
   - Select local folders / platform-authorized documents and photos; indexes must never modify originals.
   - Photos, videos, documents, and common other file types in one searchable library.
   - Thumbnails/previews, date-based browsing, favorites, folders and filters.
   - Duplicate detection may *report* candidates; no silent deletion, move or rename.
   - On Android/iOS respect scoped storage and sandbox/photo permissions; platform capabilities can differ transparently.

2. **Backup and restore**
   - User-selected local destinations; verified file copy and restore workflows.
   - At least basic incremental backup with integrity checking and meaningful failure feedback.
   - No silent overwrite or deletion; dry-run preview for potentially destructive restores.
   - Backup format documented and tested with corruption, interrupted write and restore cases.

3. **Private encrypted vault**
   - Password-protected vault using vetted authenticated encryption and KDF libraries.
   - Clear wrong-password and corruption feedback; recovery policy explicit and tested.
   - Never store plaintext secret data or keys in a catalog/analytics/ad system.
   - Inspect old Save-K SKR1 behavior only if legacy source/test fixtures become available; do not imply compatibility.

4. **PDF / document tools**
   - Merge and split; configurable text/image watermarks with recipient and date presets.
   - Export always creates a new copy; keep an untouched original.
   - Real redaction only when removal of hidden text/metadata can be verified by tests; otherwise defer *redaction* specifically, rather than shipping cosmetic masking.

5. **Image tools**
   - Resize, crop, convert, compress and remove metadata from exports.
   - Preview requested changes, preserve originals and explicitly choose output location.

6. **QR & everyday tools**
   - Generate and scan QR codes using local processing and OS camera consent when needed.
   - No forced online service for ordinary QR workflows.

7. **Apps / usability**
   - Windows installable, straightforward UI with no mandatory account.
   - Android installable APK/test package and iOS installable test build, using OS permissions and safe file pickers.
   - Common interaction model and shared library/processing core where technically feasible; visible differences where the platforms restrict access.
   - Accessible errors, cancelable long operations, simple defaults, no mandatory advanced settings.

## Monetization constraints
- Free offline core features must work without advertising or network access.
- If ads ship in 1.0, show only in permitted optional surfaces after privacy/consent review; never pass file names, paths, thumbnails, decrypted content or vault keys to ad networks.
- Advertising integration is **not a launch blocker** for the core MVP; the sustainable ad-funded service catalog is a subsequent commercialization milestone.

## Out of scope for MVP 1.0
NAS/SMB discovery, mounted-share support commitments, NAS companion/server, cloud syncing, AI image search, automatic file reorganizing/deleting, ads required to access local data, advanced video editing, production server operations or shared infrastructure changes.

## Staged implementation and dependencies
1. **Foundation**: select platform UI approach, stable Rust core API, job cancellation, metadata catalog, bounded indexing, safe path handling and tests.
2. **Windows vertical slice**: organizer GUI, preview/search, backup/restore and encryption proof with data integrity tests.
3. **Tools**: PDF/image/QR processing as reusable non-destructive modules with targeted tests.
4. **Android & iOS**: native file pickers/permissions, integrate validated core modules, installable test packages, supported feature parity.
5. **Beta/release**: cross-platform verification, restoration tests, accessibility/performance checks, release signing, installers/test distribution and privacy review.

## Definition of done
**MVP ready** means users can install and complete every essential 1.0 workflow on supported platforms, validation tests pass on target platforms, encrypted and backup formats are documented and recoverable, critical data-loss bugs are fixed, security and permission boundaries are checked, and staged builds are reviewed. An open PR, draft code or documentation does not mean the MVP is ready.

## Repo state at this revision
- `dev`: base Rust read-only metadata scanner (not a UI).
- Draft PR #1: SQLite metadata catalog + manual CLI (not merged, Rust/Windows validation pending).
- All other user-facing 1.0 modules listed above require implementation and tests.
