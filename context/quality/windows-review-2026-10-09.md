# Windows Platform Review — 2026-10-09

## Scope and verification boundary

Review the current desktop startup, permissions, global shortcuts, audio capture,
local STT/polish runtime packaging, and text delivery paths. The observable goal
is to identify Windows defects with concrete triggers and traceable evidence.
The two review passes were read-only; the repair section below records subsequent
product changes. No Git commands were run.

A second independent pass on the same date revalidated all four original input
findings and found six additional defects. Four of the additional defects are
in shared backend/UI code and affect Windows as well as other desktop platforms.
The second pass divided input, audio lifecycle, and release/runtime review,
then checked the conclusions against current callers and source probes.

The review ran on macOS ARM64. Windows installation, native Win32 hooks,
WASAPI, WebView2 focus, and real target applications were not executed. The
findings below distinguish source-level reproduction from native integration
verification; passing portable checks does not establish Windows E2E coverage.

## Repair status

The requested repair implements all ten findings under the
[Windows Stability Repair contract](../feat/windows-stability/0.1.0/prd/erd.md).
The original findings and pre-fix evidence below are historical observations.

| Finding | Repair and regression boundary |
|---------|--------------------------------|
| Stale clipboard | All delivery callers use the backend clipboard writer; errors propagate |
| Multiline Return/Tab | Windows inserts literal text with one clipboard paste |
| Own Ctrl+V captured | Enigo tags injected input; the hook bypasses only this app's injected events |
| F13-F20 unusable | Windows key decoding covers the entire accepted F1-F20 range |
| Missing updater signatures | Workflow uploads `.exe.sig` and `.msi.sig`; a real manifest-generator fixture checks the downloaded set |
| Failed audio lost | Each consumer owns original PCM, path, sample rate, and channels through stop/finalize |
| False startup success | A session acknowledges success only after stream build/play; stop errors detach and clear activity |
| Stale device format | Each new session receives its own acknowledged format; old cache is not a readiness signal |
| Duplicate prefix fallback | Single-paste delivery avoids partial keyboard fallback; streaming errors latch and block later automatic insertion |
| Auto-start only a flag | Backend registers OS startup, quotes Windows paths, refreshes on startup, and restores raw registry entries if persistence fails |

Integration also separates recording identity from retry task allocation and
protects newer recordings from stale completion/cancellation cleanup. Failed
delivery is explicit and does not automatically replay text that may already
have reached the target application. Held modifiers are released before paste
and cleanup failures are returned.

Repair verification on macOS ARM64:

- `cargo +stable test --quiet`: 861 passed, 23 ignored, zero failures; includes
  601 library tests. Repeated after the three equivalent existing-code Clippy
  adjustments with the same result.
- `cargo +stable clippy --all-features -- -D warnings`: passed. A dependency
  future-incompatibility notice remains for `block 0.1.6`.
- `cargo +stable fmt -- --check`: passed.
- `pnpm --filter @ariatype/desktop test`: 82 passed across 13 files.
- `pnpm --filter @ariatype/shared typecheck` and `pnpm check:i18n`: passed.
- Release/update/build script contract selection: 36 passed. The two newly
  added upload-contract tests failed before the workflow repair and passed after.
- Startup actual-source tests: 23 passed; registry rollback cases failed first.
  Recorder actual-source tests: three regressions failed before repair and all
  four tests passed afterward. Full Cargo tests supersede these temporary probes.
- Root `npm run build`: executed, exit 1. Both macOS targets stopped at
  `codesign` with `no identity found`; read-only keychain inspection confirms
  only Apple Development is installed, not the configured Developer ID identity.
  The first Windows compile exposed adapter visibility errors, then repaired.
- `npm run build -- --skip-mac-arm --skip-mac-intel`: exit 0 after the visibility
  repair. Fresh Windows x64 release binary, NSIS installer, updater signature,
  release manifests, and website export were generated. Version stays 1.1.1.
- Windows release artifacts live under
  `apps/desktop/src-tauri/target/release/github-release/`; the updater manifest
  contains `windows-x86_64` and the copied installer/signature exactly match the
  NSIS bundle output. Installer SHA256:
  `62bd54077210e19152215b5878d745236721c4f32c1a258f05e11bc82e39e29e`.
- The actual installer signature verifies with the build's updater public key
  using locked `minisign-verify 0.2.5`; a one-byte-tampered control is rejected.
  This is a Tauri updater signature. Cross-build skips Windows Authenticode
  signing, and no native install/login/target-application test was run.
- `npm run build:website` and `pnpm --filter @ariatype/website lint`: passed.
  Website export contains English and Chinese entry pages. Runtime resource
  verification and capability hash comparison passed. Existing linker PDB,
  bundle-size, and frontend dependency warnings remain non-fatal.
- Post-visibility actual-module startup tests: 23 passed; formatting passed.

Capability files remain unchanged. The repair adds no frontend product logic.

One adjacent preexisting follow-up was found during integration: cloud client
creation/connection failure exits in `commands/audio/capture.rs` can return
while capture remains active. Those branches are separate from the reviewed
stream-build/play and stop-error defects; cloud startup recovery needs its own
failing-first behavioral repair. Native WASAPI, hooks, target-app paste, Windows
login, and installer/update checks also remain pending.

## Findings

### P1 — Clipboard delivery pastes stale content

**Trigger:** Deliver more than 400 Unicode scalar values, or hit keyboard
simulation failure for shorter text.

**Observed control flow:**

1. `text_injector::insert_text` calls the platform injector with `&|| {}`.
2. `WindowsInjector::insert` calls this empty callback before clipboard paste.
3. No code writes the delivered text to the Windows clipboard on this path.
4. The injector sends Ctrl+V and therefore pastes the previous clipboard content,
   or nothing when the clipboard has no compatible text.

This affects regular delivery, retry delivery, and the `insert_text` IPC command.
The separate `copy_to_clipboard` command is not called by those backend paths.

**Locations:**

- `apps/desktop/src-tauri/src/text_injector/mod.rs:24-27`
- `apps/desktop/src-tauri/src/text_injector/windows.rs:19-26,35-39`
- `apps/desktop/src-tauri/src/commands/audio/shared.rs:108-113,164-169`
- `apps/desktop/src-tauri/src/commands/text.rs:7-17`

**Proof:** An isolated probe executes the production insertion body and the
production Windows injector with simulated clipboard/keyboard I/O. A 401-character
transcript produces `Paste("OLD_CLIPBOARD_CONTENT")`, failing the assertion that
the pasted value equals the current transcript.

**Repair direction:** Supply a real backend clipboard implementation and report
delivery failure through the existing command/event boundary. Keep all delivery
entry points on the same implementation and test both long-text and fallback
paths. Do not depend on a frontend clipboard event handler.

### P2 — Short multiline text sends interactive Enter/Tab keys

**Trigger:** Deliver text of at most 400 characters containing `\n` or `\t`, such
as multiline polish output, to an application where Enter submits a message or
Tab changes the focused control.

**Observed control flow:** Windows routes all short text through `Enigo::text`.
The locked Enigo 0.6.1 Windows implementation queues `Key::Return` for `\n` and
`Key::Tab` for `\t`. A chat composer can submit the first line before the rest is
inserted, and a form can move the cursor to another field. The actual effect is
target-application dependent. The macOS injector already routes multiline text
through clipboard paste.

**Location:** `apps/desktop/src-tauri/src/text_injector/windows.rs:29-30,54-57`.

**Proof:** The source probe observes
`Text("first line\nsecond line\tthird column")`, failing the assertion that
multiline delivery avoids the interactive keyboard route. The cached dependency
source was inspected at `enigo-0.6.1/src/win/win_impl.rs:276-297` to verify its
Enter/Tab behavior. Native chat/form reproduction remains required.

**Repair direction:** Route multiline/control-character content through a
literal-text delivery strategy after repairing the clipboard path. Verify in a
plain editor and in an Enter-to-send composer, without submitting a real message.

### P2 — The hook consumes the app's own clipboard-paste shortcut

**Trigger:** A user configures an accepted recording shortcut of `Ctrl+V`, then
delivery uses the clipboard path.

**Observed control flow:** The injector synthesizes Ctrl+V. The Windows keyboard
hook reads only `vkCode`; it does not inspect injected-event flags or the
injector's extra-info marker. The matcher emits `ProfilePressed` for the generated
V-down event and the main runner swallows it, preventing the paste from reaching
the target application. It can also dispatch recording actions.

**Locations:**

- `apps/desktop/src-tauri/src/shortcut/platform/windows.rs:130-148`
- `apps/desktop/src-tauri/src/text_injector/windows.rs:124-133`

**Proof:** The real key decoder and matcher, supplied Ctrl-down and V-down input
with a `Ctrl+V` profile, emit `ProfilePressed { profile_id: "dictate" }` and return
a swallowed event. The source callback has no branch distinguishing injected
input. Windows exposes this distinction through `LLKHF_INJECTED` and
`dwExtraInfo`. See [Microsoft's KBDLLHOOKSTRUCT contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct).

This conflict is independent of the stale-clipboard defect and remains after
the clipboard content is repaired. Native hook/injector integration was not run.

**Repair direction:** Identify and pass through the app's own synthetic input
before updating matcher state. Prefer an explicit event marker so unrelated
assistive input need not be indiscriminately discarded. Add an integration check
that clipboard delivery cannot trigger recording.

### P2 — Accepted F13–F20 bindings cannot trigger on Windows

**Trigger:** Import or configure a binding containing F13–F20, then send that key
from an extended keyboard or keyboard macro.

**Observed control flow:** The shared codec accepts F1–F20, but the Windows
decoder maps only VK_F1 through VK_F12. VK_F13 through VK_F20 reach
`_ => return None` before the matcher. Such bindings can be accepted and stored
while remaining unusable, and cannot be captured by the Windows hook.

**Locations:**

- `apps/desktop/src-tauri/src/shortcut/platform/windows.rs:300-311,385`
- `apps/desktop/src-tauri/src/shortcut/hotkey_codec.rs:344-359`
- `context/spec/hotkey.md` (function-key validation contract)

**Proof:** `canonicalize_hotkey_string("F13")` succeeds; decoding key-down for
VK_F13 returns `None` rather than `KeyPressed("F13")`. The same mapping omission
covers F14–F20. [Microsoft's virtual-key table](https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes)
defines these keys as 0x7C–0x83.

**Repair direction:** Implement the accepted key range or return an explicit
platform validation error before saving it. Cover every accepted function key
with decoder and capture tests.

## Additional findings from the second pass

### P1 — Windows updater signatures are omitted from release uploads

**Trigger:** Build and publish a signed tagged release through the current
GitHub Actions workflow.

`tauri.updater.conf.json` sets `bundle.createUpdaterArtifacts` to `true`.
The Windows build uses this overlay, and the signing wrapper changes only the
public key. The generated runtime overlay changes resources, not this setting.
For this configuration, Tauri creates native installers and their `.exe.sig` /
`.msi.sig` signatures. ZIP updater signatures belong to `"v1Compatible"`, which
the build does not select. See [Tauri's updater artifact contract](https://v2.tauri.app/plugin/updater/#building).

The Windows upload step includes the installers and ZIP signature patterns,
but omits `.exe.sig` and `.msi.sig`. The publish job requires a signed
`windows-x86_64` updater entry. Its manifest generator therefore skips the
unsigned Windows installer and throws before the release-creation step.

**Locations:**

- `.github/workflows/release.yml:170-175,216-226`
- `apps/desktop/src-tauri/tauri.updater.conf.json:4`
- `scripts/generate-release-manifests.mjs:109-113,140-143`

**Proof:** Run the actual manifest generator against a fixture containing signed
macOS updater files and the Windows installer matching the upload rules. It exits
1 with `Missing required updater platform(s): windows-x86_64`. Add the installer
signature to the same fixture: exit 0 and all three required platforms appear.
This proves the publication contract mismatch; no native installer was built.

**Repair direction:** Upload the native installer signatures and test the
workflow's complete uploaded file set against the required manifest platforms.

### P1 — Failed transcription cannot retain audio for retry

**Scope:** Shared recording lifecycle, including Windows.

**Trigger:** Stop a nonempty recording normally, then have STT `finish()` fail.

The stop command takes `state.streaming_stt` out of global state. Dropping that
session's sender allows the consumer's receive loop to end. After `finish()`,
the worker rereads global `streaming_stt` to save audio, although it no longer
contains the original session. Without a newer recording, the saved audio path
is always `None`. Failed history then lacks the audio that Retry requires, and
Retry returns `No audio file saved for this entry`.

The worker does not retain its original save path and raw buffer for this save
step. If another recording has started meanwhile, it can instead read that
newer session's audio. Successful transcription intentionally deletes saved
audio; this finding concerns failed-transcription recovery.

**Locations:**

- `apps/desktop/src-tauri/src/commands/audio/stop.rs:48`
- `apps/desktop/src-tauri/src/commands/audio/capture.rs:429-436,466-472,564`
- `apps/desktop/src-tauri/src/services/retry_transcription.rs:41-43`
- `apps/desktop/src-tauri/src/state/unified_state.rs:30` (retry-audio invariant)

**Proof:** A lifetime probe executes the production stop/lifecycle functions
and capture save block, with external I/O shims. Nonempty original PCM remains
available, but the save block returns `None` instead of `Some("recording_1.wav")`.

**Repair direction:** Preserve recording-specific save metadata and PCM for
the worker until it finalizes. Close the channel independently of removing the
only handle needed to save failed audio. Test stop → STT failure → history retry.

### P2 — Stream startup errors can be reported as successful recording

**Scope:** Shared recorder and stop lifecycle, including Windows.

**Trigger:** Device configuration resolves, but stream creation or `play()`
fails, for example because capture access or device initialization fails.

The recorder publishes rate/channel values before creating and playing the
stream. The caller considers those values proof of successful startup and can
return `Ok`, even though the worker has failed. The start command then emits
`Recording`. On stop, the lifecycle marks `is_recording=false`, but propagation
of the worker error returns before removing streaming state or emitting an
Error/Idle transition. The retained sender keeps the consumer waiting, and the
frontend retains its last Recording event. After both activity flags are false,
the cancellation path does not clean this stale state.

The recorder handle itself is removed by `stop()`: restarting is possible.
This is not a permanent recorder lockout.

**Locations:**

- `apps/desktop/src-tauri/src/audio/recorder.rs:175-180,239,261-266`
- `apps/desktop/src-tauri/src/commands/audio/start.rs:99-110`
- `apps/desktop/src-tauri/src/commands/audio/stop.rs:33,48`
- `apps/desktop/src-tauri/src/services/recording_lifecycle.rs:104-116,123-124`
- `context/architecture/data-flow.md:79` (RecordingFailed contract)

**Proof:** The production recorder functions with a simulated stream-build
failure return `start Ok((48000, 1))`, followed by
`stop Err("Windows microphone access denied")`. A separate control proves that
the recorder can restart. The actual Windows privacy-denial path was not run.

**Repair direction:** Acknowledge startup only after stream creation and play
succeed, send worker failures back to the caller, and ensure stop-error cleanup
closes the consumer and emits the terminal state.

### P2 — A new recording can use the previous device's audio parameters

**Scope:** Shared recorder, including Windows default-device changes.

**Trigger:** Finish one recording, then change the input device's sample rate
or channels and start another recording.

`last_sample_rate` and `last_channels` are never reset before spawning the next
worker. The initialization wait immediately accepts the previous values while
the new device is still resolving. The capture command stores these values in
the session. The PCM callback receives correct current values but does not
repair the session metadata. Stop therefore downmixes/resamples the pending tail
with stale parameters, affecting recognition. For a 48 kHz → 16 kHz change,
processing the tail as 48 kHz shortens it to one third of its intended duration.

**Locations:**

- `apps/desktop/src-tauri/src/audio/recorder.rs:150-152,261-266`
- `apps/desktop/src-tauri/src/commands/audio/capture.rs:222-226,269-277`
- `apps/desktop/src-tauri/src/commands/audio/stop.rs:50-55`

**Proof:** First session at 48 kHz, second configuration at 16 kHz with a
deterministic device-init delay: the second start returns `(48000, 1)` instead
of `(16000, 1)` using the production recorder functions.

**Repair direction:** Obtain an acknowledged format for each recording
generation and use it throughout capture, tail flush, and failed-audio saving.
Test consecutive sessions with different rates and channel counts.

### P2 — Clipboard fallback duplicates an already delivered text prefix

**Trigger:** Deliver 101–400 characters and have a later keyboard chunk fail
after an earlier chunk was inserted.

Windows sends 100-character chunks. A later error returns `false`, and the
caller falls back to pasting the entire transcript without accounting for the
already inserted prefix. Today that appends the old clipboard after the prefix.
Repairing only the empty clipboard callback still duplicates the prefix.
The locked Enigo implementation also treats a partial `SendInput` return as an
error; failure cannot be assumed to mean that zero characters reached the
target. See [Microsoft's SendInput return-count contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput).

**Locations:** `apps/desktop/src-tauri/src/text_injector/windows.rs:35-38,73-85`.

**Proof:** A 150-character transcript with simulated failure on text call 2
produces a 100-character prefix followed by the old clipboard. Supplying a
correct clipboard callback to the same production injector produces 250
characters: the first 100 plus the entire 150-character transcript.

**Repair direction:** Do not blindly retry an entire transcript after possible
partial delivery. Prefer a single literal-text delivery strategy or surface
uncertain delivery explicitly instead of appending a duplicate.

### P2 — Auto-start on login only persists a setting

**Scope:** Shared settings UI/backend, including Windows.

**Trigger:** Enable the visible Auto-start on login switch, then sign in again.

The UI calls `update_settings` for `auto_start`, and the backend only assigns
and saves the boolean. Source, dependency, installer, and startup searches found
no OS registration or consumer of this flag. No autostart plugin or custom
startup/registry script exists in the current configuration. Consequently this
switch cannot arrange Windows login startup, although the UI description promises it.

**Locations:**

- `apps/desktop/src/components/Home/GeneralSettings.tsx:205-207,420-428`
- `apps/desktop/src-tauri/src/commands/settings/mod.rs:1085-1088`
- `apps/desktop/src/i18n/locales/en.json:57-58`

**Proof:** Complete current-source/config searches show that all references to
`auto_start` are configuration, events, transfer, UI, or fixtures. This is a
source completeness finding; Windows logout/login was not executed.

**Repair direction:** Register/unregister login startup in the backend and
report OS failure before persisting a misleading successful state. Verify a
real login cycle on Windows.

## Verification evidence

### Fresh second-pass checks

Repository checks:

```sh
node --test scripts/release-workflow.test.mjs scripts/tauri-updater-config.test.mjs scripts/updater-manifest.test.mjs scripts/prepare-tauri-runtime-resources.test.mjs scripts/verify-tauri-runtime-resources.test.mjs scripts/ensure-llama-server-runtime.test.mjs scripts/build-all-platforms.test.mjs
```

Result: 64 passed after rerunning with temporary localhost access. The initial
run had 63 passed and one environmental `listen EPERM` failure. These passing
tests do not exercise the omitted signature-upload contract or functional
login startup.

Fresh source probes:

1. `/private/tmp/ariatype-input-review-20261009/probe`: 34 tests, 31 passed,
   three expected correct-behavior assertions failed for the original clipboard,
   multiline, and function-key bugs. The Ctrl+V conflict and both partial-chunk
   evidence checks passed. The injector/codec/matcher use current source, and
   the Windows decoder/swallowing functions are extracted from current source.
2. `/private/tmp/ariatype_recorder_probe.rs`: extracted production
   `start_streaming` and `stop` with simulated CPAL. One restart control passed;
   current-format and startup-error assertions failed.
3. `/private/tmp/ariatype_audio_lifetime_probe.rs`: extracted production
   lifecycle/stop functions and the capture save block with external boundary
   shims. The failed-recording retention assertion failed.

In total, these probes ran 38 checks: 32 passed and six expected regression
assertions failed. They remain failing because this is a review, not a repair.
The primary reviewer reran all three probe binaries and the release manifest
fixture against current source. Keyboard, audio device, IPC, and provider I/O
are simulated; none of these checks is native Windows E2E coverage.

Compile/run audio probes with the available local toolchain:

```sh
/Users/bytedance/.cargo/bin/rustup run stable rustc --edition=2021 --test /private/tmp/ariatype_recorder_probe.rs -o /private/tmp/ariatype_recorder_probe
/private/tmp/ariatype_recorder_probe --test-threads=1 --nocapture
/Users/bytedance/.cargo/bin/rustup run stable rustc --edition=2021 --test /private/tmp/ariatype_audio_lifetime_probe.rs -o /private/tmp/ariatype_audio_lifetime_probe
/private/tmp/ariatype_audio_lifetime_probe --nocapture
```

Release fixture: `/private/tmp/ariatype-windows-release-recheck-EFyYz5`.
Use the manifest command from the publish workflow with its three required
platforms. Excluding `AriaType_1.1.2_x64-setup.exe.sig` yields exit 1;
including it yields exit 0. Signatures and installer contents are fixtures;
this verifies discovery and workflow gating, not cryptographic validity.

### Portable repository checks

The following checks were run during the first pass, not rerun as part of the
second pass unless listed above.

Command:

```sh
node --test scripts/*runtime*.test.mjs scripts/release-workflow.test.mjs scripts/prepare-llama-server-release-assets.test.mjs scripts/build-all-platforms.test.mjs scripts/generate-release-manifests.test.mjs
```

Initial result: 69 passed, one failed with `listen EPERM` on `127.0.0.1` because
the sandbox prevented the test server from binding. Re-running the affected
`scripts/verify-tauri-runtime-resources.test.mjs` with local loopback access:
11 passed. All 70 distinct tests in the selected set therefore passed; the
initial failure was environmental, not evidence of a Windows product bug.

Command:

```sh
pnpm --filter @ariatype/desktop exec vitest run src/components/ui/__test__/hotkey-input.test.tsx src/components/Home/__tests__/OnboardingGuide.test.tsx
```

Result: two files passed, 43 tests passed.

### Isolated source probe

Temporary artifacts: `/private/tmp/ariatype-windows-audit-20261009/`.

- `prepare_probe.py` generates the adapters and probe.
- `probe.rs` includes the production Windows injector, codec, and matcher.
- The Windows key-decoder and swallowing-function bodies are copied verbatim,
  except the Win32 message constants are supplied locally to compile on macOS.
- The platform factory selects `WindowsInjector` to exercise its control flow.
- Keyboard/clipboard I/O and logging are simulated; the rdev `Key` enum is copied
  from the locally cached, locked 0.5.3 crate.
- `source-hashes.txt` records the inspected source versions.

Commands, from the temporary directory:

```sh
python3 prepare_probe.py
/Users/bytedance/.cargo/bin/rustup run stable rustc --edition=2021 --crate-name enigo --crate-type rlib enigo.rs -o libenigo.rlib
/Users/bytedance/.cargo/bin/rustup run stable rustc --edition=2021 --crate-name tracing --crate-type rlib tracing.rs -o libtracing.rlib
/Users/bytedance/.cargo/bin/rustup run stable rustc --edition=2021 --crate-name rdev --crate-type rlib rdev.rs -o librdev.rlib
/Users/bytedance/.cargo/bin/rustup run stable rustc --edition=2021 --test probe.rs --extern enigo=libenigo.rlib --extern tracing=libtracing.rlib --extern rdev=librdev.rlib -o probe
./probe --nocapture
```

Result: 32 tests executed; 28 existing source tests and one Ctrl+V conflict
evidence check passed. Three deliberate regression assertions failed:

1. Current transcript must be pasted instead of old clipboard contents.
2. Multiline delivery must avoid the interactive keyboard route.
3. An accepted F13 binding must reach the matcher.

These are source-level reproductions, not Windows E2E tests, and remain failing
because the request was a review rather than a repair.

### Unavailable verification

`cargo +stable test --offline --lib text_injector::windows::tests -- --nocapture`
could not load the `tauri-nspanel` dependency: the required checkout was missing,
and the sandbox denied creating it under the global Cargo cache. Full Cargo
tests, Windows compilation, clippy, and native Windows smoke tests were not
completed. Previously built binaries were not substituted for the current tree.

## Remaining Windows integration checks

1. Permissions: the Windows microphone check tests only whether a default input
   endpoint exists, not whether capture is authorized. With an attached device,
   disable desktop microphone access and verify the reported permission and
   recording error. CPAL 0.16.0 resolves the endpoint using
   `GetDefaultAudioEndpoint`; stream initialization occurs later.
   [Microsoft documents the separate desktop microphone-access setting](https://support.microsoft.com/en-us/windows/privacy/turn-on-app-permissions-for-your-microphone-in-windows).
   This remains a source-identified risk without native reproduction.
2. Native keyboard-hook recovery after suspend/resume and heavy load. Windows
   can silently remove a low-level hook on timeout, while this runner has no
   Windows-specific liveness probe. See [Microsoft's LowLevelKeyboardProc
   contract](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc).
3. Pill focus, repeated hide/show, clicks, mixed DPI, multiple monitors, and
   taskbar work-area positioning. `focused(false)` exists in the builder; this
   review did not establish an automatic focus-stealing defect.
4. Input into elevated targets. [SendInput's integrity-level restrictions](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
   require real target-process verification and user-visible failure handling.
5. Install/update/uninstall with NSIS and WebView2, real WASAPI device changes,
   and native local-model inference. Packaging-script tests use fixtures and do
   not prove these user workflows.

## Documentation extraction

The quality index previously described Windows text injection as unimplemented,
although an implementation exists. Its D grade remains appropriate because of
the delivery defects, but the reason now points to these concrete findings.
Future Windows verification should exercise the complete delivery entry point
and a target application's observable text. Modifier-cleanup unit tests and
installer-existence checks alone cannot verify successful dictation delivery.

The second pass exposes two further coverage gaps: a release artifact must
remain signed after upload/download, and recording metadata/audio must survive
the exact stop/error lifecycle. Portable fixture tests should reproduce these
boundaries; native Windows checks must still cover WASAPI, login startup,
Win32 hooks, and target applications. Quality grades now reflect the confirmed
audio, retry, startup-setting, and publication defects rather than old manual
verification claims.
