# Settings Transfer Native Import Picker

## Version

- Feature: `settings-transfer`
- Version: `0.1.2`
- Status: Completed

## Problem

The settings import action is implemented with a hidden HTML file input. On
macOS, clicking that input enters Wry's WebKit upload-panel adapter and aborts
while creating `NSOpenPanel`, terminating AriaType before a file can be
selected. Existing component tests dispatch the input's change event directly,
so they do not exercise the crashing user-click path.

## Goal

Open a backend-owned native JSON file picker, read the selected backup through
a bounded backend path, and preserve the existing confirmation, import, and
restart experience without using WebKit's file-upload panel.

## First-Principles Model

- Selecting a local backup is a native desktop interaction, not a browser file
  upload.
- Cancellation is an expected outcome and must not mutate state or produce
  feedback.
- Untrusted file size and encoding must be rejected before the document reaches
  import confirmation or settings validation.
- The backend owns filesystem access and the import size boundary.
- Backup contents and the selected path are sensitive and must not be logged.
- A native API that can return `nil` must cross an optional/raw boundary; a
  non-null foreign-function binding must not turn a recoverable panel failure
  into a process abort.

## User Experience

1. Choosing **Import settings** closes the overflow menu and disables the
   transfer action while the native picker is active.
2. The picker is parented to the settings window and filters for JSON files.
3. Cancelling returns to settings without a toast or confirmation dialog.
4. Selecting a readable backup opens the existing replacement confirmation.
5. Confirming uses the existing validated import and relaunch flow; declining
   leaves all settings unchanged.
6. Picker, size, encoding, read, or import failures use the existing import
   error toast.

## Data Contract

The native selection IPC command returns one backend-owned outcome:

```ts
type SettingsImportSelectionResult =
  | { status: "selected"; document: string }
  | { status: "cancelled" };
```

The backend reads no more than 2 MiB plus one sentinel byte. A selected file
larger than 2 MiB or containing invalid UTF-8 is rejected. Selection does not
mutate settings; the existing `import_settings(document)` command remains the
only mutation boundary after user confirmation.

## Acceptance Criteria

1. The settings UI contains no HTML file input for import and never enters the
   WebKit file-upload panel.
2. The Rust backend opens a native JSON picker parented to the current window.
   macOS uses a main-thread raw AppKit adapter that handles nil, cancel, and
   abort responses explicitly; other desktop platforms reuse the dialog
   plugin.
3. Cancellation returns `status: "cancelled"`, causes no confirmation, import,
   relaunch, or toast, and leaves settings unchanged.
4. The backend rejects non-regular files, input larger than 2 MiB with a
   bounded read, and invalid UTF-8 before returning a document to the frontend.
5. A selected document is confirmed before the existing import command runs.
6. Declining confirmation leaves settings unchanged; confirming imports and
   follows the existing restart contract.
7. IPC and component logs contain neither the selected path nor document,
   including failure paths.
8. No frontend dialog/filesystem dependency or Tauri capability permission is
   added.

## BDD Scenarios

### Scenario: open the native import picker

- Given the settings transfer menu is open
- When the user chooses **Import settings**
- Then the backend opens the native JSON picker
- And the WebView does not create an HTML file-upload panel

### Scenario: cancel selection

- Given the native import picker is open
- When the user cancels it
- Then no confirmation or import occurs
- And no toast is shown

### Scenario: confirm a valid selection

- Given the backend has read a valid backup within the size limit
- When the user confirms replacement
- Then the existing import command validates and persists the document
- And AriaType follows the existing restart contract

### Scenario: reject an unsafe selection

- Given a selected file exceeds 2 MiB, is not valid UTF-8, or cannot be read
- When the backend handles the selection
- Then it returns an import-selection error without exposing file contents
- And current settings remain unchanged

## Verification

- focused Rust selection, cancellation, bounded-read, UTF-8, and IPC-shape tests
- focused component tests beginning at the real Import menu click
- focused IPC redaction tests for successful and rejected selection results
- `cargo test`
- `cargo fmt -- --check`
- `cargo clippy --all-features -- -D warnings`
- `corepack pnpm --filter @ariatype/desktop test`
- `corepack pnpm --filter @ariatype/desktop build`
- `corepack pnpm --filter @ariatype/shared typecheck`
- `corepack pnpm check:i18n`
- interactive macOS smoke: open, cancel, select valid, and select oversized

## Root-Cause Evidence

The macOS crash report at
`~/Library/Logs/DiagnosticReports/ariatype-2026-07-12-000747.ips` records
`SIGABRT` on the main thread. Its stack runs from WebKit `runOpenPanel` through
Wry `run_file_upload_panel` to `NSOpenPanel::openPanel`, followed by
`objc2`'s non-null retain failure and `abort`. No settings decode, persistence,
or import command appears on the faulting stack.

The locked Wry and RFD versions both use objc2's non-null
`NSOpenPanel::openPanel` binding. The macOS adapter therefore does not route
through either of those open-panel implementations; it constructs the panel
through raw AppKit, checks the returned pointer, and maps panel aborts to a
normal command error.

## Verification Results

- Failing-first frontend run: 9 failed / 5 passed. It proved that the old DOM
  still contained an HTML file input, real Import clicks did not invoke a
  backend selector, and the IPC wrapper did not exist.
- Focused final frontend/IPC run: 14/14 passed. Coverage begins at the menu
  click and includes cancellation, confirmation ordering, import/relaunch,
  picker failure, and sensitive-log redaction.
- Focused Rust checks pass for selected/cancelled IPC shape, nil/OK/cancel/abort
  panel outcomes, exact and oversized 2 MiB boundaries, invalid UTF-8, and
  non-blocking FIFO rejection.
- Final Rust verification passed 545 library tests plus every integration test
  outside the restricted sandbox.
- Desktop frontend verification passed 13 files / 81 tests, TypeScript, and the
  production build. Shared typecheck, i18n, Markdown links, Rust formatting,
  and whitespace checks also pass.
- Strict all-feature Clippy remains blocked only by the two pre-existing
  warnings in `src/stt_engine/models.rs` and `src/lib.rs`; the same command
  passes with those two lints allowed.
- No frontend file/dialog dependency or capability permission was added.
- The AppKit sheet itself cannot be driven by the headless suites. A packaged
  macOS smoke test should still cover open, cancel, valid selection, and an
  invalid or oversized selection; a nil panel now produces the generic import
  error instead of terminating AriaType.
