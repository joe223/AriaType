---
title: Fix Settings Import Picker Crash
type: fix
status: completed
date: 2026-07-12
---

# Fix Settings Import Picker Crash Plan

## Overview

Replace the crashing WebView file-upload picker with a backend-native JSON
picker and bounded reader while preserving the existing confirmation, import,
and relaunch workflow.

## Problem Frame

Previous state: clicking **Import settings** invoked a hidden HTML file input.
On macOS, Wry's upload-panel implementation aborted while constructing
`NSOpenPanel`. Tests injected a file directly into the hidden input and
therefore missed the real user-click boundary.

Delivered state: the user click invokes a typed backend command, native picker
cancellation is silent, untrusted files are bounded and decoded in Rust, and
only confirmed documents reach the existing mutation command.

## Scope Boundaries

In scope:

- native backend JSON picker for import
- bounded 2 MiB file reading and UTF-8 validation
- typed selected/cancelled IPC result
- removal of the hidden HTML file input
- ordering, cancellation, error, and redaction regression tests
- crash evidence and interactive smoke-test guidance

Out of scope:

- changing the V1 backup schema or validation rules
- changing what settings are redacted or preserved
- changing the settings menu visual design
- modifying Tauri capability files
- fixing unrelated Clippy warnings

## Implementation Units

1. Capture the crash boundary and specify the native-picker behavior.
2. Add failing tests that begin at the real Import menu click and lock the
   backend selection/read contract.
3. Add the Rust native picker adapter, selected/cancelled result, bounded read,
   and typed IPC wrapper.
4. Remove the hidden file input and connect selection to the existing
   confirmation/import/relaunch sequence.
5. Verify focused and system-wide behavior, record results, and complete the
   spec and plan.

All implementation units are complete.

## System-Wide Impact

- File selection and reading moved from WebKit/React to the headless backend.
- The frontend temporarily receives the selected document only to show the
  existing confirmation before invoking the existing import mutation command.
- macOS uses a nil-safe raw AppKit adapter because the locked Wry and RFD open
  paths share the aborting non-null binding. Windows and Linux reuse the
  already registered dialog plugin.
- No frontend filesystem/dialog dependency or capability permission was added.

## Risks & Dependencies

- A native panel cannot be fully driven by JSDOM or Rust unit tests. The
  adapter has deterministic tests for nil, OK, cancel, and abort outcomes, but
  a macOS interactive smoke remains the platform-lifecycle proof.
- File selection runs on the macOS main thread without holding settings state;
  file reading runs on the blocking runtime.
- Input uses metadata preflight, Unix non-blocking open, opened-handle metadata,
  and a 2 MiB plus one-byte read ceiling to cover special files and growth
  races.
- Selection results and errors use sensitive logging semantics because they can
  contain imported settings.

## Verification Evidence

- Root cause: the 2026-07-12 macOS diagnostic report records main-thread
  `SIGABRT` in WebKit/Wry `run_file_upload_panel` while calling
  `NSOpenPanel::openPanel`; the settings import command is absent from the
  faulting stack.
- Failing-first frontend/IPC run: 9 failed / 5 passed against the old hidden
  input implementation.
- Focused final frontend/IPC run: 14/14 passed.
- Focused Rust selection, reader, and picker outcome tests pass, including FIFO
  rejection, exact/oversized limits, invalid UTF-8, nil, cancel, and abort.
- Final `cargo test` passed 545 library tests and every integration binary
  outside the restricted sandbox.
- Final desktop frontend suite passed 13 test files / 81 tests. Desktop
  TypeScript and production build, shared typecheck, i18n, Markdown links,
  Rust formatting, and whitespace checks pass.
- Strict all-feature Clippy reports only the two pre-existing warnings in
  `src/stt_engine/models.rs:192` and `src/lib.rs:789`. The same command passes
  with those two lints allowed.
- Interactive macOS panel automation is unavailable in the headless suite.
  Open, cancel, valid selection, and invalid/oversized selection remain the
  recommended packaged-app smoke; panel construction failure is now a normal
  import error rather than an abort.
