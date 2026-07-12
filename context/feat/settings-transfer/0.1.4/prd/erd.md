# Settings Transfer Full-Fidelity Raw Backup

## Version

- Feature: `settings-transfer`
- Version: `0.1.4`
- Status: Completed

## Problem

The current export path builds a portable envelope from an in-memory
`AppSettings` clone and deliberately clears credentials, provider endpoints,
local runtime commands, the selected audio device, and other values before it
writes the backup. Import then restores those values from the target machine.
That behavior is safe for casual sharing, but it is not a faithful backup: a
user cannot move their complete working configuration to another installation.

The current native save picker also adds an unnecessary decision to a backup
operation whose destination can be predictable. Users need one action that
copies the actual persisted configuration to Downloads and clearly identifies
the resulting path.

## Goal

Make settings transfer a full-fidelity migration mechanism:

- export the persisted application `settings.json` bytes unchanged into the
  operating system Downloads directory
- import that raw `AppSettings` document and apply every value from it,
  including credentials and device-specific values
- continue accepting the legacy redacted V1 envelope and preserve its original
  target-value restoration semantics
- warn users before export that the backup can contain credentials and private
  configuration and must be shared carefully

## First-Principles Model

- A full backup is a copy of the persisted source of truth, not a reconstructed
  projection of in-memory state.
- Export holds the `AppState.settings` lock while it copies the persisted file,
  so a concurrent settings update cannot race the backup boundary.
- Byte-for-byte identity rules out serialization, normalization, redaction, or
  envelope construction during export.
- A raw backup represents the source installation completely. Importing it
  replaces target settings; merging target credentials into it would make the
  result non-faithful.
- Raw input is configuration, not a startup artifact. It must satisfy the
  established settings schema directly and must not run or log startup
  migrations that could silently reinterpret explicit current values.
- A legacy redacted envelope does not contain enough information for complete
  replacement. Its existing target credential and device-value restoration is
  therefore required for backward compatibility.
- A file containing secrets can still be the correct migration artifact, but
  the UI must communicate that risk before the file is created.
- The backend owns source-path resolution, Downloads resolution, copying,
  format detection, validation, and persistence. The frontend renders paths,
  warnings, confirmation, and restart outcomes.

## Information Architecture

The existing low-prominence settings overflow menu remains the entry point.
Its two actions remain **Export settings** and **Import settings**.

- Export no longer opens a save picker. It writes a `.json` backup directly
  inside the operating system Downloads directory.
- The existing export-success toast continues to display the final absolute
  destination path.
- The menu note explicitly states that exported settings may contain
  credentials and private or device-specific configuration and should be
  shared carefully.
- Import keeps the existing native file picker, replacement confirmation, and
  environment-aware restart behavior.
- Import confirmation tells the user to select only a trusted file because a
  full-fidelity backup can apply local runtime commands.

The warning remains visually secondary to the actions, consistent with the
current menu, but it must be readable and available in every desktop locale.

## Data Contract

### New raw backup format

The exported file is the application's persisted `settings.json` itself. Its
top-level object is `AppSettings`; it has no transfer envelope:

```json
{
  "shortcut_profiles": {},
  "audio_device": "Studio Microphone",
  "cloud_stt_configs": {
    "provider": {
      "api_key": "source-api-key",
      "app_id": "source-app-id",
      "base_url": "https://source.example/v1"
    }
  },
  "local_polish_runtime": {
    "api_key": "source-runtime-key",
    "base_url": "http://127.0.0.1:8000/v1",
    "server_command": "/path/to/runtime",
    "server_args_json": "[]"
  }
}
```

The example is abbreviated for readability. A real export preserves every
byte and every persisted `AppSettings` field from the source file, including
credentials, provider configuration, custom dictionary content, local runtime
configuration, and device-specific selections.

Because export does not create an envelope, it does not emit `format`,
`schema_version`, `secrets_included`, or
`device_specific_fields_included`. This is a format distinction, not a V1
schema change, so no schema version is bumped.

The first backup destination is `Downloads/ariatype-settings.json`. If that
path already exists, the backend chooses the first available deterministic
numeric suffix: `ariatype-settings-2.json`,
`ariatype-settings-3.json`, and so on. Existing backups are never replaced.

### Legacy envelope compatibility

Import continues accepting the existing V1 envelope:

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

The legacy flags remain `false` because those artifacts were redacted. They
do not describe the new raw backup, which has no envelope flags.

### Format-specific import semantics

| Input | Detection | Imported values | Target restoration |
|-------|-----------|-----------------|--------------------|
| Raw settings | Top-level `AppSettings` object without the transfer envelope marker | All validated source values, including credentials and device-specific fields | None |
| Legacy V1 envelope | `format = "ariatype-settings"` and `schema_version = 1` | Redacted portable preferences | Preserve the existing legacy target credentials and device-specific values |

Both paths retain the existing size limit, semantic settings validation,
atomic persistence, settings-changed event, and restart result. Backup
contents and import documents must never be logged.

### Raw settings field gate

Raw settings import requires every established `AppSettings` top-level field
except these six fields, whose defaults are already documented by the current
settings schema:

- `stay_in_tray`
- `window_context_enabled`
- `pill_size`
- `pill_background_color`
- `pill_background_opacity`
- `correction_memory_enabled`

Only those six fields may be absent and receive their current `AppSettings`
defaults. All other established top-level fields must be present. Unknown
top-level fields are rejected.

Every nested object must also satisfy its complete established field set, with
no unknown fields. This applies to shortcut profiles, cloud STT provider
configurations, cloud polish provider configurations, local polish runtime
settings, and custom polish templates.

Raw import deserializes explicit values as the current settings schema and
then applies the existing transfer semantic validation. It does not invoke or
log startup migration routines. Consequently, an identity raw document keeps
its explicit current-schema values instead of being rewritten by an unrelated
legacy startup migration.

## Acceptance Criteria

1. Export holds the `AppState.settings` lock while it reads the current
   application settings path and writes those source bytes unchanged directly
   inside the operating system Downloads directory.
2. Export does not clone or serialize `AppSettings`, construct a transfer
   envelope, normalize values, redact fields, or open a save picker.
3. A byte comparison between the persisted source `settings.json` and the
   exported destination is identical, including whitespace and trailing
   newline behavior.
4. The exported raw document consequently retains every persisted
   `AppSettings` field and value, including API keys, app IDs, provider URLs,
   local runtime commands and arguments, custom dictionary content, and the
   selected audio device.
5. The first destination is `ariatype-settings.json`; subsequent exports use
   the first available numeric suffix beginning with
   `ariatype-settings-2.json` and never overwrite an earlier backup.
6. A successful export returns the final destination path, and the existing
   success toast displays that path. Failure to resolve Downloads, read the
   source file, or write the destination produces the existing localized
   export error feedback without claiming success.
7. Import recognizes a raw `AppSettings` document, validates it before any
   mutation, and atomically persists its values without redacting them or
   restoring any credential, provider, runtime, dictionary, or device value
   from the target installation.
8. Raw import requires every established top-level and nested schema field,
   rejects unknown top-level and nested fields, and permits only
   `stay_in_tray`, `window_context_enabled`, `pill_size`,
   `pill_background_color`, `pill_background_opacity`, and
   `correction_memory_enabled` to be absent and receive defaults.
9. Raw import preserves explicit current-schema values and does not invoke or
   log startup migration routines.
10. Import continues accepting valid legacy V1 redacted envelopes and retains
   their existing target-value restoration behavior. Unsupported envelopes
   and invalid settings remain rejected before mutation.
11. No transfer schema version is incremented: raw settings are a distinct
   envelope-free input shape, while the existing V1 envelope contract remains
   unchanged.
12. Before export, the settings transfer UI explicitly warns that the file may
   contain credentials and private or device-specific settings and should be
   shared carefully. Every desktop locale provides the warning.
13. Import confirmation explicitly instructs the user to select only a trusted
    file because imported local runtime commands may be applied.
14. Import payloads, exported bytes, credentials, private setting values, and
    startup migration messages for raw imports are never written to
    application logs, and no Tauri capability file is modified.

## BDD Scenarios

### Scenario: export an exact settings-file backup

- Given the persisted application `settings.json` contains credentials,
  device-specific values, custom whitespace, and a trailing newline
- When the user chooses **Export settings**
- Then no save picker opens
- And AriaType creates a backup directly in the operating system Downloads
  directory
- And it uses `ariatype-settings.json` or the first available numeric suffix
- And the destination bytes are identical to the persisted source bytes
- And the success toast identifies the final destination path

### Scenario: import a raw full-fidelity backup

- Given the target installation has different provider credentials, runtime
  commands, dictionary content, and microphone selection
- And the user selects a valid raw `AppSettings` backup from another
  installation
- When they confirm the import
- Then the backend validates and atomically persists the raw backup settings
- And each source credential and device-specific value replaces the target
  value
- And no target-only provider entry or private value is restored into the
  imported settings

### Scenario: import a legacy redacted envelope

- Given the user selects a valid V1 backup created by an earlier AriaType
  release
- And that envelope declares that secrets and device-specific fields were not
  included
- When they confirm the import
- Then the portable preferences are imported
- And the current target credentials and device-specific values are preserved
  according to the V1 contract

### Scenario: reject a partial or unknown raw settings object

- Given a raw settings document omits any required established field
- Or it contains an unknown top-level or nested field
- When import is attempted
- Then the document is rejected before disk or memory mutation
- But omission of any of the six documented defaulted top-level fields remains
  compatible

### Scenario: communicate sensitive-backup risk

- Given the user opens the settings transfer menu
- Then the menu warns that exported settings may contain credentials and
  private or device-specific configuration
- And the warning tells the user to share the file carefully

### Scenario: import only a trusted full-fidelity backup

- Given the user selects a raw settings document
- When AriaType asks them to confirm replacement
- Then the dialog says to use only a trusted file
- And it explains that local runtime commands from the file may be applied

### Scenario: reject invalid input without mutation

- Given the user selects malformed JSON, invalid raw settings, or an
  unsupported transfer envelope
- When import is attempted
- Then the user sees an error
- And the current in-memory and on-disk settings remain unchanged

## Verification

- failing-first Rust test proving the current export differs from the source
  bytes and redacts sensitive values
- Rust byte-identity tests with noncanonical whitespace and trailing bytes
- Rust raw-import tests proving source credentials, endpoints, runtime values,
  dictionary content, audio selection, and provider maps replace target values
- Rust compatibility tests proving legacy V1 redacted envelopes still restore
  target-only values
- Rust rejection tests for malformed raw settings and unsupported envelopes
- frontend tests proving export invokes no save-picker flow, reports the final
  Downloads path, and renders the sensitive-backup warning
- all desktop locales and i18n validation
- full Rust tests, formatting, all-feature Clippy, desktop tests, shared
  typecheck, production build, and Markdown link validation
- interactive smoke confirming direct Downloads placement, path feedback, raw
  credential retention, and format-specific import behavior

## Verification Results

- The initial failing-first Rust check produced the expected unresolved
  `export_settings_file_to_directory` symbol before the direct-copy seam was
  implemented.
- An additional failing-first raw-import check demonstrated that a partial raw
  identity document was accepted before the strict field gate was added.
- Focused export verification passed 5/5 tests, covering byte identity,
  sensitive value retention, deterministic collision naming, and copy errors.
- The final focused settings backend suite passed 49/49 tests, including raw
  replacement, strict top-level and nested field gates, six-field default
  compatibility, startup-migration isolation, and legacy V1 restoration.
- Focused settings transfer component verification passed 11/11 tests.
- The complete desktop frontend suite passed 13 files / 82 tests.
- Full `cargo test` outside the restricted sandbox passed 549 library tests
  plus every integration test suite.
- Desktop and shared TypeScript checks, locale validation for all 10 desktop
  locales, `cargo fmt -- --check`, Markdown link validation, and the Vite
  production build passed.
- Strict all-feature Clippy remains blocked only by the pre-existing
  `unnecessary_sort_by` warning at `src/stt_engine/models.rs:192` and
  `collapsible_match` warning at `src/lib.rs:789`; the same command passes with
  those two lints allowed.
- An interactive native Downloads/export/import smoke test was not run. It
  remains recommended to confirm operating-system directory resolution and
  the end-to-end native picker flow on a displayed desktop.
