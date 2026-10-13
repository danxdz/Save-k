# Save-K / syncSave

Local-first file organizer and privacy-focused toolbox for Windows, Android and iOS.

> Project status: **foundation / planning**. No production-ready app is available from this repository yet.

## Intended capabilities

- Universal Picasa-style library for user-selected photos, videos and documents.
- Read-only discovery and indexing of local, external and mounted NAS/SMB folders.
- Verified backups and restores, with an optional encrypted vault.
- Non-destructive watermarking, redaction and file conversion tools.
- Shared Rust processing core and platform-specific interfaces.

## Safety principles

1. Never move, rename, delete or rewrite source files during indexing.
2. Never touch NAS folders without the user's explicit selection and permissions.
3. Backups must be verified before reporting success; restore is independently tested.
4. Encrypted data requires authenticated encryption and an explicit recovery design before rollout.
5. Core offline functions must not depend on accounts, advertisements or cloud APIs.
6. User documents, file names and vault keys must never be sent to advertising systems.

## Development

The `main` branch is the stable baseline; feature work starts on `dev` and is reviewed before merging. No GitHub Actions or deployment infrastructure is enabled by default.

- Planning and acceptance criteria: [syncSave Monday board](https://adapteit.monday.com/boards/5105974441).
- Proposed MVP and architecture details will live under `docs/` on the development branch.

**Legacy Save-K code has not yet been imported.** Its behavior and data formats must be independently inspected and tested before any migration.
