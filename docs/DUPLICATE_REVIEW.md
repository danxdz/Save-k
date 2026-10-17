# Duplicate photo review — first local prototype

**Status:** experimental source change, not a validated release. No NAS work.

## What it does

1. In the desktop gallery, click **Find duplicates** to start an explicit, local, read-only analysis of indexed photos. The worker runs off the UI thread.
2. Candidate sources come from the existing local SQLite catalog, with a hard limit of 50,000 indexed photo entries.
3. **Exact copies:** only files with the same cataloged size need BLAKE3 streaming hashes. Identical file contents are grouped together even if names differ. Missing, changed, or linked files are skipped.
4. **Possible visual matches:** decoded images are resized to a 9x8 grayscale fingerprint; a 64-bit dHash uses horizontal pixel differences. It compares nearby candidates using four 16-bit index bands, with Hamming distance <= 3, approximate aspect ratio difference <= 3%, and mean brightness difference <= 24.
5. Results appear as review-only groups with thumbnails, filenames, sizes and copy-path actions. Large groups expose a virtualized path list.

## Privacy and safety

- No file is deleted, moved, renamed, decrypted, transmitted or overwritten.
- The algorithm reads image bytes from local paths **only after the user clicks Find duplicates**.
- No online server, OpenCV, upload service, tracking/analytics or ad SDK.
- The persistent SQLite catalog is **not encrypted** in this prototype and already contains file paths. Duplicate results themselves are held in RAM, not persisted.
- Visual matches are **suggestions**, not verified duplicates. Unrelated logos, screenshots, CAD drawings, low-detail images, or burst shots can match incorrectly.
- No delete button is offered, even for exact duplicate groups.

## Limitations

- dHash is not full pHash or AI recognition. Cropped, mirrored, rotated, brightness-adjusted and heavily recompressed images may be missed. The app currently does not normalize EXIF orientation.
- Visual matching can report indirect groups connected by intermediate fingerprints; not every photo in a group necessarily matches every other.
- Supported decoders: JPEG, PNG, BMP, WebP, GIF and TIFF. HEIC, RAW and some videos will not be visually compared.
- Hashing and image decoding may take a while on large folders; the first version has a spinner but no cancellation or progress percentage. Avoid starting huge jobs until that is addressed.
- The visual candidate check has a 300,000 comparison budget; if reached, the UI warns that suggestions are incomplete.
- Existing source files can change concurrently; hashes verify a file's size/timestamps before and after reading, but race-proof filesystem isolation is not implemented.
- The SQLite catalog can include files no longer present on disk; these are skipped, not removed.
- Data and file paths must remain on the local device; avoid testing on sensitive folders until Windows validation is complete.

## Try it on Windows (PowerShell)

```powershell
cd "$env:USERPROFILE\Downloads\Save-k-gallery"
git fetch origin
git switch --track origin/feature/duplicate-review-v1
cargo check -p savek-desktop
cargo test --workspace
cargo run -p savek-desktop
```

If the branch already exists locally, use `git switch feature/duplicate-review-v1` and `git pull --ff-only` instead.

Open **Review duplicates** via the left sidebar and click **Find duplicates**. For initial testing, index a small throwaway folder with copied JPEGs and PNG/BMP variants first, rather than a huge personal photo collection.

## Validation checklist

- [ ] `cargo check -p savek-desktop` succeeds on Windows.
- [ ] `cargo test --workspace` succeeds, including exact byte hashes and cross-format fingerprints.
- [ ] Two identical photo files with different names appear as **Exact copies**, not also as a duplicate visual-only group.
- [ ] A PNG and BMP containing the same pixels are suggested under **Possible visual matches**.
- [ ] Unrelated photos aren't misleadingly presented as confirmed copies.
- [ ] Known sample photo source hashes remain unchanged before and after analysis.
- [ ] Missing / oversized / unreadable media show aggregate skipped counts, without crashing.
- [ ] The user can inspect and copy paths of every file in a large duplicate group.
- [ ] No delete/move controls exist; no network traffic is necessary to analyze photos.

Related Monday task: https://adapteit.monday.com/boards/5105974441/pulses/3281480017
