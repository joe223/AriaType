---
title: Fix Settings Import Development Relaunch
type: fix
status: completed
date: 2026-07-12
---

# Fix Settings Import Development Relaunch Plan

## Overview

Make successful settings import environment-aware at the backend contract:
packaged builds continue to relaunch automatically, while development builds
keep the Tauri CLI and Vite server alive and provide explicit manual-restart
guidance.

## Problem Frame

Previous state: the import UI invoked the process-plugin relaunch after every
successful import. During `tauri dev`, that restarted only the application
binary. The original Tauri CLI exited and stopped its Vite child, so the new
binary loaded the configured development URL with no server and displayed a
blank window.

Delivered state: the backend returns an explicit restart strategy.
Development imports persist successfully without automatic relaunch and
explain how to restart the development command; packaged imports retain the
existing automatic behavior.

## Scope Boundaries

In scope:

- backend-owned development versus packaged restart strategy
- typed import result serialization
- manual development feedback using the existing toast pattern
- automatic and manual strategy regression tests
- localization in every supported desktop locale
- preservation of automatic-relaunch error feedback

Out of scope:

- restarting or supervising the external Vite/Tauri command tree from AriaType
- changing backup validation, persistence, redaction, or schema
- changing the native import picker or confirmation flow
- changing the visual placement of settings transfer controls
- modifying Tauri capability files
- fixing unrelated warnings

## Implementation Units

1. Specify the runtime boundary and capture the observed blank-window evidence.
2. Add failing component and Rust contract tests for manual and automatic
   restart strategies.
3. Extend the backend import result with a strategy selected by
   `tauri::is_dev()`.
4. Render manual-restart guidance without relaunch in development while
   preserving packaged relaunch and its failure feedback.
5. Add localized guidance, run focused and system-wide verification, record
   results, and complete the spec and plan.

All implementation units are complete.

## System-Wide Impact

- The Rust settings command is the source of truth for post-import restart
  policy.
- The frontend remains reactive: it renders the returned strategy and does not
  inspect its build environment.
- Packaged builds continue loading bundled assets after automatic relaunch.
- Development builds retain the externally owned Vite server until the
  developer intentionally stops the command.
- No settings schema or capability boundary changed.

## Risks & Dependencies

- A debug build is not necessarily a Tauri development build; the
  implementation uses `tauri::is_dev()` rather than debug assertions.
- The frontend and backend result types change together so an absent or
  misspelled strategy cannot silently select the wrong behavior.
- Unit tests lock strategy selection and relaunch calls, but keeping an actual
  Vite process alive still requires an interactive development smoke test.
- Imported runtime settings are not all expected to apply until the developer
  performs the instructed manual restart.

## Verification Evidence

- The imported application-support document parsed successfully and contained
  the expected settings structures.
- Logs showed successful import, intentional process exit, and successful
  backend setup in the relaunched process, with no crash or decode error.
- The blank window coincided with port 1422 having no listener while the
  development configuration still pointed the WebView at that URL.
- Source inspection confirmed application restart spawns only the current
  executable and does not reconstruct the Tauri CLI/Vite process tree.
- Failing-first frontend verification produced the expected 1 failed / 10
  passed result. The Rust failing-first check produced the expected missing
  strategy-symbol and import-result-field compile errors.
- Focused final verification passed 11/11 frontend tests and 2/2 Rust strategy
  and serialization tests.
- The full desktop suite passed 13 files / 82 tests. Full `cargo test` outside
  the restricted sandbox passed 548 library tests plus every integration
  suite.
- Desktop and shared TypeScript checks, all 10 desktop locales, Markdown links,
  Rust formatting, and the Vite production build passed.
- Strict all-feature Clippy remains blocked only by the pre-existing
  `unnecessary_sort_by` warning at `src/stt_engine/models.rs:192` and
  `collapsible_match` warning at `src/lib.rs:789`; it passes with those two
  lints allowed.
- Automated tests do not drive a live native import under `tauri dev`. An
  interactive development import smoke remains recommended and is not claimed
  as completed automation.
