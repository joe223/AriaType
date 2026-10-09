---
title: Repair Windows Platform Review Findings
type: fix
status: active
date: 2026-10-09
---

# Repair Windows Platform Review Findings

## Overview and problem frame

Repair all ten confirmed findings from the Windows review and build the current
version with root `npm run build`. The behavior contract is
[Windows Stability Repair](../../feat/windows-stability/0.1.0/prd/erd.md).

## Scope boundaries

Product fixes and regression coverage for input, recording, login startup, and
release artifacts. Preserve existing source changes. No Git operations, version
bump, capability edits, release publication, or unrelated feature work.

## Implementation units

- [x] Input: failing tests, backend clipboard wiring, literal Windows paste,
  explicit delivery errors, own-event filtering, and F13-F20 mapping.
- [x] Recording: failing tests, per-session startup acknowledgement/format,
  unconditional stop-error cleanup, and owned failed-audio save metadata.
- [x] Release: failing artifact-contract test, native installer signature uploads.
- [x] Startup: failing registration contract tests, backend OS registration and
  configuration reconciliation without frontend control flow.
- [ ] Verify: affected tests, Rust checks, frontend/shared/i18n checks, root
  `npm run build`, output inspection, and documented native-test limitations.
- [x] Extract: update repair/spec status, review evidence, quality index, and
  canonical build guidance if current commands have misleading behavior.

## System-wide impact

Common recording state and retry ownership change across desktop platforms.
Text-delivery callers need an explicit clipboard/error boundary. Login startup
is an OS side effect controlled by backend settings. Release signing remains
unchanged; only artifact collection is repaired.

## Risks and dependencies

- Host is macOS ARM64; native Windows WASAPI/hook/login/target-app E2E is absent.
- Cargo dependencies/toolchains and cross-build/signing prerequisites must be
  verified. Missing dependencies must not be hidden with old binaries.
- Build scripts clean generated outputs; capability cleanup currently targets
  absent `e2e.json`. Preserve `default.json` and any subsequently appearing file.
- Parallel owners: input/injection, audio lifecycle, release scripts, and root
  startup/docs/build. Coordinate before editing another owner's files.

## Verification evidence

The [review repair section](../../quality/windows-review-2026-10-09.md#repair-status)
records failing-first evidence and current checks. Actual full Cargo tests pass:
861 passed, 23 existing ignored, zero failures (601 library tests). All-features
Clippy and formatting pass after three equivalent old-code lint adjustments.
Desktop Vitest: 82 passed; shared typecheck/i18n passed. Release/update/build
script contract selection: 36 passed.

Root `npm run build` was invoked with the rustup stable toolchain and all three
required targets. macOS ARM/Intel signing failed before compilation because
the configured Developer ID Application identity is absent. Read-only
`security find-identity -v -p codesigning` confirms only an Apple Development
identity is available; it cannot substitute for Developer ID distribution.
The first Windows compile also reported private adapter re-export errors.
Visibility was narrowed to the existing settings boundary and the build rerun:
`npm run build -- --skip-mac-arm --skip-mac-intel` completed with exit 0.
Current version 1.1.1 artifacts are under
`apps/desktop/src-tauri/target/release/github-release/`:

- `AriaType_1.1.1_x64-setup.exe` and matching `.exe.sig`
- `latest.json` and `latest.updater.json` (Windows x64)

The binary is PE32+ x86-64; the NSIS launcher is PE32 as expected. Copied release
files match the freshly built NSIS outputs. Updater cryptographic verification
passes with the build's public key and rejects a tampered control. Windows
Authenticode signing is skipped by the cross-bundler. Native Windows E2E was
not run. Actual-module startup tests after the visibility repair: 23 passed;
formatting passed.

Website build/export and lint pass, with `docs/en/index.html` and
`docs/zh/index.html` present. Current runtime resource verification passes.
Capability `default.json` is unchanged and `e2e.json` was absent before build
cleanup. No Git operations occurred.

The implementation units are complete, but the plan remains active: the default
full signed macOS build requires the missing Developer ID certificate/private
key, and native Windows behavior remains unverified. The canonical contributing
guide now explains root build targets, cache cleanup, release output paths, and
installer updater-signature requirements. An adjacent preexisting cloud client
startup-cleanup defect is recorded separately in the review repair section.
