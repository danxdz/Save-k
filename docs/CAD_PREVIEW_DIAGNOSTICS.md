# Preview diagnostics and STEP/CAD file recognition

Status: experimental, Windows compilation and runtime validation required. **Not a native 3D CAD viewer.**

## Why some JPG or PNG files show no preview

Save-K currently categorizes files by their extensions. The image decoder checks the underlying bytes; an extension alone does not guarantee a valid supported image.

Thumbnail failures now distinguish:
- Missing, unreadable, or non-regular files
- Symlink or Windows reparse point (not followed)
- Source file larger than 40 MB
- Unsupported encoding / corrupt image
- Decoder size or allocation limit (very large images)

Select a failed photo in the gallery, then see its Details panel for a more specific reason. The decoder accepts JPEG, PNG, BMP, WebP, GIF and TIFF; not HEIC/RAW or all JPEG variants. JPEG/PNG can still fail depending on encoding, image contents, or dimensions. Do not report a decoder error as a corrupt file without inspecting it separately.

## CAD files

- `.step`, `.stp`, `.iges`, `.igs`, `.stl`, `.obj` are recognized as **3D CAD files** instead of Other.
- On opening an existing local catalog, SQLite metadata records for these extensions are upgraded once, in a transaction; **no source file bytes are read or changed** during that migration.
- The sidebar has a **3D CAD files** section and CAD items show a labeled placeholder.
- Selecting `.step` or `.stp` allows clicking **Inspect STEP header**. Only the first 32 KiB are read, with a simple STEP Part 21 signature check and best-effort metadata extraction (file name, schema, description). This is **not a geometry parser**.
- The **Open in default CAD viewer…** button explicitly launches the operating system's associated program (e.g. FreeCAD, CAD Assistant, your CAD software). If no suitable program is registered, Windows may show an Open With prompt.
- The external program may have its own network or data-sharing settings; Save-K itself does not upload the files. Only open files in viewers you trust.

## Native STEP preview (future engineering task)

Use Open CASCADE to parse B-Rep geometry, safely tessellate into a bounded triangle mesh and render inside Save-K with orbit, zoom and pan. Add on-demand generated thumbnails, assembly tree and color support after Windows and license/security/performance evaluation. A static icon/header or external app launch must **never** be represented as built-in CAD geometry preview.

## Windows testing

```powershell
cd "$env:USERPROFILE\Downloads\Save-k-gallery"
git fetch origin
git switch --track origin/feature/cad-files-preview-diagnostics
cargo check -p savek-desktop
cargo test --workspace
cargo run -p savek-desktop
```

If that branch already exists locally, use `git switch feature/cad-files-preview-diagnostics` then `git pull --ff-only`.

1. Inspect a JPG/PNG that previously showed No preview. Click it and read the Details panel diagnosis.
2. Test a valid small JPG and PNG: both should still generate thumbnails.
3. Test an intentionally invalid `.jpg` file with text bytes: the viewer should report unsupported/corrupt encoding, not crash.
4. If you have STEP/STP files in your saved catalog, check they now show in 3D CAD files after restart, with no rescanning required.
5. Select a STEP file and inspect the bounded header, then (optionally) open it in a trusted installed external CAD viewer.
6. Check file hashes before/after preview; source files should not be modified by Save-K.
7. No in-app rotation, mesh processing, or STEP thumbnails yet. The future OCCT integration needs a separate task and testing.

No production deployment, NAS access, background web upload, deletion, backup modification, or encryption behavior is introduced.
