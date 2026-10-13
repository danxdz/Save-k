# Architectural direction

Status: **proposal**, not an implemented end-to-end architecture.

## Bounded modules
- `savek-core` (Rust): read-only source enumeration, metadata models; later index jobs, integrity verification, crypto and transforms behind explicit interfaces.
- Desktop UI (to decide): the legacy app used Rust/egui, which may be reusable if code/data compatibility is verified. Consider a Rust native UI or a Flutter shell without prematurely rewriting a proven core.
- Android/iOS clients (to decide): OS-provided file pickers/permissions with a shared processing core through a stable bridge when feasible. Do not assume unrestricted NAS or storage access.
- Local metadata DB (future): SQLite catalog + thumbnail cache, with schema migrations and source identifiers resilient to offline drives.
- NAS: start with user-mounted SMB shares as ordinary read-only paths; discovery, auth, reconnect/backoff, remote companion and offline catalog are distinct later tasks.

## Security/permission boundaries
- A scan receives explicit source paths; it never writes to the selected sources.
- Never follow symlinks or reparse-point directories automatically. Detect mount, link and path boundaries before recursive operations.
- Do not store SMB credentials in the catalog; use OS credential stores, explicit permission grants and least privilege.
- When encryption is added, use audited AEAD and vetted KDF implementations. No homemade cryptography. Design key loss/recovery and authenticated manifests first.
- No document bytes, sensitive filenames or keys are sent to advertisements or analytics.
- Jobs that change user files must be separate from the read-only index and require preview, explicit approval and recovery strategy.

## Branch and review workflow
`dev` -> reviewed staging branch/release candidate (if added) -> `main` when ready. No automated deploys or GitHub Actions without explicit request.

## Legacy Save-K migration
This repository was initialized empty. The user's previous Windows app reportedly used Rust/egui and its password/SKR1 handling needs investigation. Nothing can be imported safely until the old source, storage formats and representative **non-sensitive** test fixtures are available.

## Open decisions
UI toolkit, platform bridge, backup format compatibility, catalog schema, ad policy, licensing, and release signing. These decisions get explicit technical spikes and Monday acceptance criteria before implementation.
