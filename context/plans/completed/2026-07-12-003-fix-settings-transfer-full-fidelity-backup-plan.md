---
title: Fix Settings Transfer Full-Fidelity Backup
type: fix
status: completed
date: 2026-07-12
---

# Fix Settings Transfer Full-Fidelity Backup Plan

## Overview

Replace the redacted envelope export with an exact copy of the persisted
`settings.json` in the operating system Downloads directory, and add
format-specific import semantics so raw backups migrate every value while
legacy redacted V1 envelopes remain usable.

## Problem Frame

Current state: export serializes an in-memory clone into a V1 envelope after
clearing credentials, provider endpoints, runtime commands, dictionary
content, and device-specific values. It opens a native save picker. Import
always treats the document as a redacted envelope and restores private values
from the target machine.

Delivered state: one export action copies the existing persisted settings file
byte-for-byte to Downloads and reports its path. Raw settings import replaces
the target configuration completely, including credentials and
device-specific values. Existing V1 redacted backups retain their original
merge behavior.

## Scope Boundaries

In scope:

- byte-identical copy of the persisted application settings file
- backend-owned operating system Downloads resolution and deterministic
  `ariatype-settings.json`, `ariatype-settings-2.json`, and later collision
  naming
- holding the settings state lock across the persisted-file copy boundary
- removal of the export save-picker interaction
- raw `AppSettings` import with complete source-value replacement
- strict raw top-level and nested field gates with compatibility for only the
  six documented defaulted top-level fields
- raw import isolation from startup migrations and migration logs
- backward-compatible legacy V1 redacted-envelope import
- explicit localized warning about credentials and private settings
- trusted-file import confirmation that identifies local runtime command risk
- preservation of export destination-path feedback
- failing-first backend and frontend regression coverage

Out of scope:

- history, recordings, downloaded models, logs, caches, or other files outside
  `settings.json`
- encryption, password protection, cloud sync, or automatic sharing
- editing raw backup content during export
- changing the legacy V1 envelope schema or incrementing its version
- changing import picker, atomic persistence, settings-changed event, or
  development versus packaged restart policy
- modifying Tauri capability files
- fixing unrelated warnings

## Implementation Units

1. Add failing-first backend tests that compare source and destination bytes,
   prove sensitive fields remain present, distinguish raw and legacy input,
   and lock format-specific target restoration.
2. Replace the export projection/save-dialog path with a backend seam that
   copies the persisted settings file unchanged to a collision-safe path under
   the operating system Downloads directory and returns that path.
3. Split import decoding into raw `AppSettings` and legacy V1 envelope paths:
   validate both before mutation, apply raw values without target restoration,
   and preserve the legacy merge behavior.
4. Update the transfer component tests and UI copy so no export picker is
   expected, destination-path feedback remains, and the menu warns users about
   credentials and private or device-specific data in the file.
5. Add every locale string, run focused and system-wide verification, perform
   documentation gardening, record evidence, and move this plan to completed.

All implementation units are complete.

## System-Wide Impact

- The persisted application settings file becomes the canonical export
  artifact; export no longer constructs a separate representation.
- The export command holds `AppState.settings` while copying the persisted
  file so the backup cannot race a concurrent settings update.
- Export now contains credentials and executable runtime configuration, so
  user-facing risk communication becomes part of the feature contract.
- Import becomes format-aware. Raw settings are replacement backups; legacy
  envelopes remain redacted migration documents with target-value
  preservation.
- Raw settings require a complete, known schema except for the six established
  defaulted top-level fields, and they bypass startup migration routines.
- The frontend remains reactive: it triggers export, displays the backend path
  and warning, and does not choose a destination or interpret backup content.
- The existing restart result and development-safe manual restart strategy are
  unaffected.

## Risks & Dependencies

- Downloads resolution failure remains a normal error; implementation does
  not fall back to an undisclosed path.
- Deterministic numeric collision handling prevents exports from overwriting
  earlier backups.
- Raw and envelope detection is explicit, so an invalid envelope cannot bypass
  envelope validation by falling through to raw settings decoding.
- Only the six established defaulted top-level fields may be absent from raw
  settings. All other top-level fields and every nested schema are complete
  and unknown-field-free.
- Importing executable paths and credentials is intentional for full-fidelity
  migration. Validation does not execute them, logs do not expose them, and
  the confirmation instructs users to select only trusted files.
- The 2 MiB import limit and atomic persistence continue covering both input
  formats.
- Automated tests prove bytes and paths with temporary directories. Native
  Downloads placement still benefits from an interactive smoke test.

## Verification Evidence

- The initial failing-first Rust check produced the expected unresolved
  `export_settings_file_to_directory` symbol before implementation.
- A second failing-first check showed that a partial raw identity document was
  accepted before the strict raw field gate existed.
- Focused export tests passed 5/5.
- The final settings backend suite passed 49/49 tests.
- Focused settings transfer component tests passed 11/11.
- The full desktop suite passed 13 files / 82 tests.
- Full `cargo test` outside the restricted sandbox passed 549 library tests
  plus every integration test suite.
- Desktop and shared TypeScript checks, all 10 desktop locales, Rust
  formatting, Markdown links, and the Vite production build passed.
- Strict all-feature Clippy remains blocked only by the pre-existing
  `unnecessary_sort_by` warning at `src/stt_engine/models.rs:192` and
  `collapsible_match` warning at `src/lib.rs:789`; it passes with those two
  lints allowed.
- An interactive native Downloads/export/import smoke test was not run and
  remains recommended.
