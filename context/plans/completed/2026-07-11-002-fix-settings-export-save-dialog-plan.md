---
title: Fix Settings Export Save Dialog
type: fix
status: completed
date: 2026-07-11
---

# Fix Settings Export Save Dialog Plan

## Overview

Replace the implicit WebView download with a backend-driven native save panel,
atomic export replacement, explicit cancellation outcome, and path-aware
feedback.

## Problem Frame

Current state: the frontend synthesizes an anchor click from a Blob, so the
WebView silently selects its download directory and the user cannot see or
choose the destination.

Desired state: the operating system asks where to save, the backend owns the
write, cancellation is silent, and success identifies the saved path.

## Scope Boundaries

In scope:

- native save panel for settings export
- backend export-to-path helper with a synced temporary file and platform
  atomic replacement
- saved/cancelled IPC result
- localized success feedback containing the path
- focused Rust and React regression tests

Out of scope:

- changing the V1 backup envelope or redaction rules
- changing settings import
- adding frontend filesystem access
- modifying Tauri capability files

## Implementation Units

1. Update the versioned behavior contract and create failing frontend/backend
   regression tests.
2. Add the Rust-side native dialog adapter and atomic export writer.
3. Replace Blob download handling with typed saved/cancelled result rendering.
4. Reuse the localized success message, add bounded path rendering, and update
   verification documentation.
5. Run focused and system-level verification.

## System-Wide Impact

- Desktop builds gain the native dialog plugin on supported platforms.
- Export changes from returning document content to returning only an outcome
  and local path, reducing sensitive data exposure to the WebView.
- Import behavior and backup schema remain unchanged.

## Risks & Dependencies

- Native dialog APIs differ between desktop platforms and must use the plugin's
  cross-platform path abstraction.
- Native panels cannot be fully exercised in headless unit tests; the dialog
  result adapter and export writer need independent deterministic coverage.
- Existing unrelated strict Clippy warnings may remain visible.

## Verification Evidence

- Failing-first React check: 2 failed / 6 passed. The old implementation could
  not show a selected path and reported cancellation as an export error.
- Focused backend export tests: 8/8 passed. Coverage includes redaction,
  suggested names, no write after cancellation, exact IPC serialization,
  extension normalization, safe overwrite, atomic-replace failure, and
  temporary artifact cleanup.
- Focused settings transfer component tests: 8/8 passed, including a deferred
  cancellation result so negative toast assertions run after the handler
  settles.
- Final `cargo test --lib`: 536/536 passed outside the restricted sandbox. The
  complete `cargo test` command also passed all integration test binaries.
- `cargo fmt -- --check`: passed.
- `cargo clippy --all-features -- -A clippy::unnecessary_sort_by -A
  clippy::collapsible_match -D warnings`: passed. Without the two allowances,
  strict Clippy remains blocked by existing warnings in
  `src/stt_engine/models.rs:192` and `src/lib.rs:788`.
- Desktop frontend: 13 test files / 77 tests passed; TypeScript and the Vite
  production build passed.
- Shared typecheck, i18n validation, and Markdown link validation passed.
- `tauri-plugin-dialog@2.7.1` is registered only on the Rust side; no frontend
  dialog/filesystem dependency or capability permission was added.
- The native save panel itself is not automatable in the headless suites. The
  dialog adapter follows the Tauri v2 blocking-save contract from an async
  command, and an interactive packaged-app smoke test remains recommended.
