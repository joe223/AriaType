---
title: Settings Transfer
type: feat
status: completed
date: 2026-07-10
---

# Settings Transfer Plan

## Overview

Add a low-emphasis settings overflow menu that exports a redacted,
versioned backup and imports untrusted backup documents through backend-owned
validation and persistence.

## Problem Frame

Current state: settings are only accessible through individual IPC updates and
the private application data file.

Desired state: users can back up and restore portable preferences without
exposing credentials or manually editing files.

## Scope Boundaries

In scope:

- current `AppSettings` preferences
- redacted versioned JSON format
- atomic import with target secret preservation
- settings modal overflow UI, confirmation, relaunch, toasts, and i18n

Out of scope:

- history, recordings, model binaries, correction memory, and dictionaries
- exporting credentials, provider endpoints, or executable runtime commands
- adding a native file-dialog capability

## Implementation Units

1. Add pure Rust backup serialization, parsing, validation, redaction, merge,
   and atomic-write helpers with failing-first tests.
2. Expose thin Tauri export/import commands and register typed IPC wrappers
   that never log import content.
3. Add a `SettingsTransferMenu` to the settings modal title bar using the
   existing Floating UI, confirmation, button, and toast patterns.
4. Add all locale strings and update documentation indexes.
5. Run backend, frontend, i18n, formatting, build, and visual checks.

## System-Wide Impact

- Settings persistence gains a versioned transfer boundary.
- Import updates `AppState.settings`, emits the existing event, then relaunches
  to reinitialize hotkeys, tray, audio, engines, and local runtime safely.
- No capability or platform permission changes are required.

## Risks & Dependencies

- WebView Blob downloads require visual/E2E confirmation.
- Imported shortcut defaults may differ by platform; custom values are kept.
- Full frontend verification requires workspace dependencies to be installed.
- Existing unrelated Clippy warnings may block the strict all-feature command.

## Verification Evidence

- `cargo test`: passed the complete Rust suite outside the restricted sandbox,
  including 529 library tests and all integration test binaries.
- `cargo test commands::settings::__test__::settings_`: 9/9 transfer tests
  passed, covering redaction, target preservation, schema validation, shortcut
  validation, size limits, and interrupted-write recovery.
- `cargo fmt -- --check` and `cargo check`: passed.
- `cargo clippy --lib -- -A clippy::unnecessary_sort_by -A
  clippy::collapsible_match -D warnings`: passed. Strict Clippy without the two
  allowances remains blocked by pre-existing warnings in
  `src/stt_engine/models.rs:192` and `src/lib.rs:787`.
- Desktop frontend: 13 test files / 75 tests passed; `tsc --noEmit`, the full
  i18n checker, and the Vite production build passed.
- A local browser visual preview was attempted, but the in-app browser policy
  blocked the localhost URL. Component-level interaction and layout contracts
  were verified through the focused frontend tests and source review.
