# Settings Transfer Development Restart Safety

## Version

- Feature: `settings-transfer`
- Version: `0.1.3`
- Status: Completed

## Problem

Importing a valid settings backup persists the document and then asks the
process plugin to relaunch AriaType. In a packaged build, the relaunched binary
loads bundled frontend assets. Under `tauri dev`, however, the binary renders
the configured `devUrl` while the original Tauri CLI owns the Vite
`BeforeDevCommand` lifecycle.

The process-plugin relaunch starts only a new AriaType binary; it does not
rerun `npm run dev`, the Tauri CLI, or its Vite child. The old binary exits, the
original development command returns, Vite is stopped, and the detached binary
opens `http://localhost:1422` without a server. The resulting window is blank
even though the imported settings document is valid and the backend starts
successfully.

## Goal

Preserve automatic relaunch for packaged builds while making development
imports safe: persist the validated settings, keep the development process
alive, and tell the developer to stop and rerun the development command to
apply every imported setting.

## First-Principles Model

- Import persistence and process relaunch are separate outcomes. A relaunch
  orchestration limitation must not make a successful import appear corrupt.
- The backend owns runtime policy because it knows whether it was compiled for
  the Tauri development or custom-protocol environment.
- The frontend renders the backend result; it must not infer restart behavior
  from Vite environment variables or duplicate runtime policy.
- `tauri::is_dev()` is the canonical environment signal. Debug assertions are
  not equivalent to the Tauri development runtime.
- A process-plugin relaunch restarts the application binary, not the external
  command tree that supplies a development server.
- Packaged behavior must remain unchanged because bundled frontend assets do
  not depend on the development server.

## Information Architecture

The existing low-prominence settings transfer menu remains unchanged. Import
confirmation and error feedback remain in their current locations. Only the
success feedback differs by backend-selected restart strategy:

- automatic: show the existing imported feedback and relaunch
- manual: show imported feedback with a localized instruction to stop and
  rerun the development command, without calling relaunch

No new persistent control, capability, or settings field is introduced.

## Data Contract

The import result extends the existing restart contract with an explicit
backend-owned strategy:

```ts
type SettingsImportRestartStrategy = "automatic" | "manual";

interface SettingsImportResult {
  restart_required: boolean;
  restart_strategy: SettingsImportRestartStrategy;
}
```

For the current import operation, `restart_required` remains `true`.
Development builds return `restart_strategy: "manual"`; packaged builds return
`restart_strategy: "automatic"`.

## Acceptance Criteria

1. The Rust backend selects the restart strategy with `tauri::is_dev()` and
   serializes it in the typed import result.
2. A successful import under `tauri dev` persists the validated document but
   does not invoke the process-plugin relaunch command.
3. Development success feedback states that the settings were imported and
   instructs the developer to stop and rerun the development command to apply
   every setting.
4. The current Tauri CLI and Vite development server remain alive after a
   development import.
5. A successful packaged-build import preserves the existing automatic
   relaunch behavior.
6. Automatic-relaunch failure preserves the existing localized restart-error
   feedback.
7. Backup validation, persistence, redaction, import confirmation, and picker
   behavior remain unchanged.
8. Every desktop locale defines the development restart instruction, and no
   Tauri capability file is modified.

## BDD Scenarios

### Scenario: import settings during development

- Given AriaType is running under `tauri dev`
- And the user confirms a valid settings backup
- When the backend validates and persists the document
- Then the import result requires a manual restart
- And the frontend does not invoke automatic relaunch
- And feedback instructs the developer to stop and rerun the development
  command
- And the current Vite server remains available

### Scenario: import settings in a packaged build

- Given AriaType is running with bundled frontend assets
- And the user confirms a valid settings backup
- When the backend validates and persists the document
- Then the import result requires an automatic restart
- And the frontend invokes the existing relaunch command

### Scenario: automatic relaunch fails

- Given an imported result requests automatic restart
- When the relaunch command fails
- Then the existing localized restart-error feedback is shown
- And the successful settings persistence is not rolled back

### Scenario: reload imported settings after a manual development restart

- Given a development import completed successfully
- When the developer stops and reruns the development command
- Then the backend loads the imported settings
- And the frontend renders normally from the running Vite server

## Verification

- failing-first component test proving a manual strategy currently calls
  relaunch
- focused component tests for manual, automatic, and relaunch-failure outcomes
- Rust unit tests for development and packaged strategy selection
- exact Rust serialization test for the import result contract
- desktop test suite and production build
- Rust test, formatting, and all-feature Clippy checks
- shared typecheck, desktop locale validation, and Markdown link validation
- interactive development smoke: import, confirm the current UI remains
  rendered, then stop and rerun the development command

## Root-Cause Evidence

- The imported file at the application support settings path parses as valid
  JSON and contains the expected top-level settings structures.
- Application logs record `settings_imported`, followed by
  `app_exit_requested`, then a new process recording `logging_initialized`,
  `settings_loaded`, and `setup_completed` without an import error or panic.
- After the original `npm run dev` command returned, no process was listening
  on TCP port 1422 and a request to `127.0.0.1:1422` was refused.
- The development Tauri configuration points the WebView at
  `http://localhost:1422`.
- The locked process-plugin relaunch delegates to Tauri's application restart,
  which spawns the current executable. It does not restart the parent Tauri
  CLI or the Vite `BeforeDevCommand` that the CLI owns.

## Verification Results

- Failing-first frontend verification produced the expected 1 failed / 10
  passed result: a manual restart strategy still invoked relaunch. The Rust
  failing-first check also produced the expected compile RED because the
  strategy symbols and import-result field did not yet exist.
- Focused final verification passed 11/11 frontend tests and 2/2 Rust restart
  strategy and serialization tests, covering manual, automatic, and automatic
  relaunch-failure behavior.
- The full desktop frontend suite passed 13 files / 82 tests.
- Full `cargo test` outside the restricted sandbox passed 548 library tests
  plus every integration test suite.
- Desktop and shared TypeScript checks passed. The Vite production build also
  passed.
- Locale validation passed for all 10 desktop locales, and Markdown link
  validation passed.
- `cargo fmt -- --check` passed.
- Strict all-feature Clippy remains blocked only by the two pre-existing
  warnings at `src/stt_engine/models.rs:192` (`unnecessary_sort_by`) and
  `src/lib.rs:789` (`collapsible_match`). The same all-feature check passes
  with those two lints allowed.
- The automated suites prove that development results do not call relaunch,
  but they do not drive a live native import through `tauri dev`. An
  interactive development import smoke test remains recommended to confirm
  that the current UI and Vite server stay available.
