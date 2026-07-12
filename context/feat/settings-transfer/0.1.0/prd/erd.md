# Settings Transfer

## Version

- Feature: `settings-transfer`
- Version: `0.1.0`
- Status: Completed

## Problem

AriaType persists user preferences in a local `settings.json`, but users
cannot create a portable backup or restore those preferences through the UI.
Manually copying the application data file is undiscoverable and can expose
credentials or device-specific runtime commands.

## Goal

Provide a quiet, trustworthy way to export and import application settings
without adding another primary settings section or exposing secrets.

## User Experience

- Place a subtle three-dot overflow button beside the close button in the
  existing settings modal header.
- The overflow menu contains two secondary actions: **Export settings** and
  **Import settings**.
- The menu uses the current rounded, neutral, low-emphasis desktop visual
  language. It must not compete with the active settings section.
- Import uses the existing native-looking confirmation dialog and clearly
  states that current settings will be replaced and AriaType will restart.
- Success and failure use the existing toast system.

## Backup Scope

Included:

- shortcuts and trigger modes
- language, appearance, audio, transcription, and privacy preferences
- model selections and cloud provider preferences
- custom polish prompts and templates

Excluded:

- API keys, provider credentials, and app IDs
- cloud and local provider endpoints, local runtime commands, runtime
  arguments, and audio device identifiers
- transcription history, recordings, downloaded model files, logs, caches,
  learned correction memory, and manually maintained dictionary files

When a redacted backup is imported, credentials, endpoints, and
device-specific settings already present on the target device are preserved.
Provider entries that only exist on the target are retained intact so their
credentials still have a valid configuration owner. A backup must never be
logged as an IPC argument.

## Data Contract

Backend export returns a suggested file name and a JSON document:

```json
{
  "format": "ariatype-settings",
  "schema_version": 1,
  "app_version": "1.0.5",
  "exported_at": "2026-07-10T12:00:00Z",
  "secrets_included": false,
  "device_specific_fields_included": false,
  "settings": {}
}
```

The backend owns serialization, redaction, version checks, semantic
validation, credential preservation, persistence, and the settings-changed
event. The frontend only selects or downloads a file, confirms replacement,
and requests an application relaunch.

## Import Invariants

1. Import accepts at most 2 MiB.
2. Malformed JSON, a mismatched format, an incomplete or unknown V1 field, or
   an unsupported schema version is rejected before memory or disk changes.
3. Shortcut strings are canonicalized, must be valid, and must be unique.
4. Dictate and Riff profile action constraints remain valid.
5. Closed-value settings such as theme, pill position, indicator mode, and
   denoise mode must be recognized values.
6. Pill size, color, and opacity must stay within supported ranges.
7. Secrets, endpoints, and local runtime commands in the imported document
   are ignored; target-device values are retained.
8. The new settings file is written through a same-directory temporary file
   and replacement rollback path before in-memory state changes.
9. A successful import emits exactly one `settings-changed` event and returns
   `restart_required = true` so all runtime services reload consistently.

## BDD Scenarios

### Scenario: export a portable settings backup

- Given the user has customized settings and configured cloud credentials
- When they choose **Export settings**
- Then AriaType downloads a versioned JSON backup
- And the backup contains their preferences
- And the backup contains no API keys, app IDs, provider endpoints, runtime
  commands, or audio device identifier

### Scenario: import a valid backup

- Given the user selects a valid AriaType settings backup
- When they confirm the replacement
- Then the backend validates and atomically persists the settings
- And existing target-device credentials remain unchanged
- And AriaType relaunches with the restored preferences

### Scenario: reject an invalid backup safely

- Given the user selects malformed JSON or an unsupported backup version
- When import is attempted
- Then the user sees an error
- And the current in-memory and on-disk settings remain unchanged

## Acceptance Criteria

1. The import/export entry is available from every section of the current
   settings modal but remains visually secondary.
2. Export produces the documented V1 envelope and redacts all sensitive and
   device-specific fields.
3. Import validates before mutation, preserves target secrets, writes safely,
   updates state once, and requests a restart.
4. Import payloads and exported content are never written to application logs.
5. All user-facing text is translated in every supported locale.
6. Rust tests cover format/version checks, redaction, round-trip behavior,
   validation, secret preservation, size limits, and atomic file replacement.
7. Frontend tests cover menu visibility, export, import confirmation, size
   rejection, success, and error feedback.

## Verification

- `cargo test`
- `cargo fmt -- --check`
- `cargo clippy --all-features -- -D warnings`
- `pnpm --filter @ariatype/desktop test`
- `pnpm --filter @ariatype/desktop build`
- `pnpm check:i18n`
- visual verification of the settings overflow menu in the desktop WebView
