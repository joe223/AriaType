# Windows Stability Repair

## Version and status

Version: 0.1.0. Status: Active. Requested repair of the ten findings in the
[Windows review](../../../../quality/windows-review-2026-10-09.md).

## Observable outcome

Dictation delivers the current transcript literally on Windows, supported
shortcuts remain usable, recording startup and stop report truthful states,
failed transcription retains its own retry audio, login startup reflects the
saved setting, and signed Windows updater artifacts survive release upload.
Shared recording and startup fixes apply to all supported desktop platforms.

## Contracts

1. All delivery entry points supply a working backend clipboard writer. A
   clipboard failure prevents paste and returns an explicit error.
2. Windows literal text delivery preserves newlines and tabs without sending
   interactive Return/Tab, and never blindly retries an already delivered
   keyboard prefix. Prefer one clipboard paste over speculative partial retry.
3. The app's own injected input bypasses shortcut matching and capture; physical
   input and unrelated assistive input retain existing behavior.
4. Every accepted Windows function key F1 through F20 reaches the matcher.
5. Recording startup succeeds only after the current session's stream is built
   and playing; each session receives its own sample rate and channels.
6. Stop-error paths close the consumer, clear activity/session state, and emit
   Error then Idle. Restart remains possible.
7. Failed transcription saves the original recording's PCM/path and can retry
   after stop; it never saves another session's buffer. Successful audio cleanup
   remains intentional.
8. Updating `auto_start` registers/unregisters OS login startup in the backend.
   Failed registration is returned without persisting a successful setting.
   Failed settings persistence restores the original OS registration and leaves
   the in-memory setting unchanged. Windows executable paths are quoted; removal
   includes Task Manager-disabled entries. Startup refreshes the executable path
   and reconciles configuration, including imported settings.
9. Release uploads contain the signatures required by the selected Tauri v2
   updater format, and the downloaded artifact set generates all required
   updater platforms.

## Boundaries

Use existing command, recorder, lifecycle, injector, and settings seams. Keep
OS effects in backend adapters. No frontend business logic, capability-file
changes, Git operations, version bump, or release publication is requested.

## Acceptance and verification

- Regression checks fail before the corresponding fix and pass after it.
- Exercise clipboard write/paste failures, literal multiline delivery, injected
  event filtering, F13-F20 decoding, stream startup failure, consecutive device
  formats, failed-audio retry ownership, and login-registration failures.
- Verify the release workflow upload patterns against real manifest generation.
- Run Rust tests, clippy, formatting, relevant frontend tests, shared typecheck,
  i18n checks, and the requested root `npm run build`.
- Record unavailable Windows native E2E separately from source and cross-build
  results. Never treat a stale executable or adapter-only probe as a native pass.

## Implementation and verification status

All ten reviewed findings are implemented with regression coverage. Full Rust
tests: 861 passed, 23 existing ignored; Clippy and formatting pass. Frontend:
82 tests passed, shared typecheck and i18n passed. Windows x64 NSIS/updater
artifacts and website were freshly built; updater cryptographic verification
passed and a tampered control failed as expected. See the
[repair evidence](../../../../quality/windows-review-2026-10-09.md#repair-status)
for exact commands and boundaries.

This specification remains active because native Windows acceptance has not
been executed and default `npm run build` cannot complete its macOS targets
without the configured Developer ID signing identity. The Windows-only root
build with `--skip-mac-arm --skip-mac-intel` completed successfully. No version
bump or release publication occurred.
