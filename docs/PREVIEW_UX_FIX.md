# Gallery preview UX and local decoder diagnostics

This branch refines existing gallery error handling. It is **not** a new image codec or an embedded STEP renderer.

## Image cards
- Failed previews show a short `No preview` label in each card, so long errors cannot overlap neighboring cards.
- Hover a failed card to see its filename/path and failure information. The selected photo's Details panel shows the full image decoder message.
- `Copy decoder error` copies the local diagnostic text without requiring a file upload. It does **not** copy the photo itself.
- `Retry previews` clears the RAM-only cache; the next visible tiles will be decoded again, useful after files are restored or changed.
- `Open original in default image viewer…` is an explicit user action. It can open the file in the Windows-associated program, which may support formats Save-K does not. Privacy depends on that external program's settings.

## Why extensions are not enough
A `.jpg` or `.png` file might contain valid but unsupported image encoding, be truncated/empty, have a different actual file signature, exceed configured limits, or be missing. Save-K now identifies common image signatures from the first 16 bytes (JPEG, PNG, GIF, BMP, WebP, TIFF) and includes the actual decoder error in the Details panel. An unsuccessful decode does *not* prove the original is corrupt.

## STEP/CAD files
`.stp`/`.step` are categorized as CAD, but the current UI has **no embedded 3D geometry renderer**. The existing CAD panel can inspect a bounded STEP Part 21 header or ask Windows to open the model in a registered external application.

A true internal 3D preview requires an additional Open CASCADE (OCCT) integration: parse STEP B-Rep, tessellate geometry into bounded meshes, handle assemblies/names/colors, render local viewport with orbit/pan/zoom, cache thumbnails, and test resource exhaustion on malformed/huge files. This is a distinct development task. No CAD file bytes should go to a cloud converter.

## Test on Windows

```powershell
cd "$env:USERPROFILE\Downloads\Save-k-gallery"
git fetch origin
git switch --track origin/feature/preview-ux-fix
cargo check -p savek-desktop
cargo test --workspace
cargo run -p savek-desktop
```

If the branch already exists locally, `git switch feature/preview-ux-fix` then `git pull --ff-only`.

### Acceptance
- Failed JPG/PNG tile labels stay within each card at small thumbnail sizes.
- Select a failed tile and use `Copy decoder error`; file remains untouched.
- Open a supported JPG/PNG locally and confirm working image thumbnails still work.
- `Retry previews` refreshes memory-only state without rescanning or modifying any source file.
- Explicitly invoking the Windows viewer launches a locally configured app; it must not happen automatically.
- Existing STEP category remains visible; no claim that STEP geometry is rendered inside Save-K.
- Run Rust tests and manual gallery smoke test before merge; no GitHub Actions or deployment.
