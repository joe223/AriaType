# Settings Transfer Save Destination

## Version

- Feature: `settings-transfer`
- Version: `0.1.1`
- Status: Completed

## Problem

Settings export currently creates a WebView Blob download. The WebView chooses
the default download directory without presenting a native save panel, and the
success message does not tell the user where the backup was written.

## Goal

Make the export destination explicit: show the operating system's native save
panel, let the user choose the file name and directory, and report the final
saved path after the backend writes the backup.

## First-Principles Model

- File destination selection is a user interaction, not an implicit browser
  download policy.
- Backup generation and filesystem persistence remain backend-owned.
- Cancelling a save panel is a normal outcome, not an error.
- A successful export is only complete after the selected file has been
  written and synced.

## User Experience

1. Choosing **Export settings** closes the overflow menu and opens a native
   save panel with the existing dated JSON file name.
2. The panel filters for JSON files and lets the user change both directory and
   file name.
3. Cancelling the panel creates no file and shows no success or error toast.
4. A successful export shows a toast containing the final file path.
5. A dialog or write failure shows the existing export error toast.

## Data Contract

The export IPC command returns one of these backend-owned outcomes:

```ts
type SettingsExportResult =
  | { status: "saved"; file_path: string }
  | { status: "cancelled" };
```

The backend retains a pure export-to-path seam so the feature remains usable
without the React frontend. The Tauri command adapts the native dialog result
to that seam. Neither the serialized backup nor its contents may be logged.

## Acceptance Criteria

1. Export no longer creates a Blob URL or synthetic download anchor.
2. The backend opens the native save panel with a suggested `.json` file name.
3. A selected destination receives the redacted V1 settings document.
4. Existing files are replaced only after the operating system save panel
   confirms the user's choice and a same-directory temporary document has been
   fully written and synced; the platform then atomically replaces the target,
   leaving the previous file intact if replacement fails.
5. A destination without an extension receives `.json`, so the exported file
   remains selectable by the import control on every platform.
6. Cancellation returns `status: "cancelled"` without writing or showing a
   toast.
7. Success returns `status: "saved"` with the final display path, and every
   locale can render that path in the success toast.
8. No frontend filesystem capability or direct file-writing logic is added.

## BDD Scenarios

### Scenario: choose an export destination

- Given the user opens the settings transfer menu
- When they choose **Export settings**
- Then the native save panel opens with a dated JSON file name
- And the user can select a directory and file name

### Scenario: save a settings backup

- Given the user selects a destination in the save panel
- When the backend completes the export
- Then a redacted settings backup exists at that destination
- And the success toast identifies the final path

### Scenario: cancel export

- Given the native save panel is open
- When the user cancels it
- Then no backup is written
- And no success or error toast is shown

## Verification

- focused Rust export writer tests
- focused settings transfer component tests
- `cargo test`
- `cargo fmt -- --check`
- `cargo clippy --all-features -- -D warnings`
- `corepack pnpm --filter @ariatype/desktop test`
- `corepack pnpm --filter @ariatype/desktop build`
- `corepack pnpm check:i18n`
- native save-panel smoke test when a desktop display is available

## Verification Results

- The failing-first React test produced two expected failures: the old Blob
  flow could not report a selected path and treated cancellation as an error.
- Eight focused Rust export tests pass, covering redaction, suggested file
  names, cancellation-before-write, IPC serialization, `.json` normalization,
  overwrite, failed replacement preservation, and artifact cleanup.
- Eight focused React transfer tests pass, including path feedback, settled
  cancellation, and backend write failure behavior.
- The final Rust library suite passed 536 tests; the complete Rust test command
  also passed all integration binaries outside the restricted sandbox.
- Desktop frontend verification passed 13 files / 77 tests, TypeScript, the
  production build, shared typecheck, i18n, formatting, and Markdown links.
- Strict all-feature Clippy remains blocked only by the two pre-existing
  warnings in `src/stt_engine/models.rs` and `src/lib.rs`; the same command
  passed with those two lints allowed.
- The native panel is a Rust-side Tauri adapter and cannot be driven by the
  headless unit suites. Its save/cancel outcomes are covered at the adapter
  seam; an interactive packaged-app smoke test remains recommended.
