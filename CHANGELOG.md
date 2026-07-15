# Changelog

All notable changes to the desktop application will be documented in this file.

## Unreleased

### Features

- Support export settings (394876c)

### Bug Fixes

- Avoid comment shortcut conflicts in dictation (89420e2)

## v1.0.5 (2026-07-06)

### Features

- Support updater (a98fdb0)

### Bug Fixes

- Allow fn modifier chord hotkeys (802ad3a)

## v1.0.4 (2026-07-02)

### Bug Fixes

- Release modifiers after windows paste injection (e1f2319)
- Stt sidecar should not lanuch terminal (6744698)
- Llm sidecar should not lanuch terminal (1e8f2b7)
- Select popover can not be clicked (fde0e5d)

## v1.0.2 (2026-06-27)

### Bug Fixes

- Learn compact corrections safely (5392943)
- Pill/tooltip position in windows (4dd9664)
- Preserve modal close transitions (2e85e3e)
- Allow production changelog loading (8347bed)

## v1.0.1 (2026-06-15)

### Features

- Refine transcription polish and UX (6f31bfd)
- Expose correction dictionaries (474a157)
- Make settings and local polish easier to use (6f9e07e)

### Bug Fixes

- Preserve modal close transitions (2e85e3e)
- Allow production changelog loading (8347bed)
- Surface settings nav items that need attention (0782ca1)

## v0.6.5 (2026-06-08)

### Features

- Make polish model risk diagnosable (9e54520)

### Bug Fixes

- Keep local polish bounded and downloads complete (4741850)

## v0.6.4 (2026-06-03)

### Bug Fixes

- Make Windows shortcuts usable across platforms (6c8a06a)
- Keep tray window reachable from dock (8b87607)

## v0.6.3 (2026-05-25)

### Features

- Support windows (adb73ec)

## v0.6.2 (2026-05-22)

### Bug Fixes

- Prevent shortcut slash leaks (66d534a)
- Keep recording feedback and shortcut state trustworthy (82b1759)

## v0.6.1 (2026-05-19)

### Bug Fixes

- Keep idle tooltips from showing pill body (53fa411)

## v0.6.0 (2026-05-19)

### Features

- Verify cloud setup and keep polish output plain text (e9a807f)
- Customize pill appearance (a83e1f4)
- Support deliberate recording finish triggers (e9ee0d2)
- Add voice-writing polish templates (acde99b)

### Bug Fixes

- Adapt cloud polish timeout for long requests (aae00e2)
- Context aware visual terms (ad8d5b7)

## v0.5.1 (2026-05-07)

### Features

- Add window context capture via OCR to improve polish accuracy (ac6a493)
- Optimize UI for better experience (14eaad3)
- Optimize onboarding guide (48c59d5)
- Optimize text inject performance (ee9cb14)

### Bug Fixes

- Add default value for pill_size field (46bcc21)

## v0.4.0 (2026-04-25)

### Features

- Implement multi-shortcut profiles and update UI (388d909)
- Add changelog viewer in about page (813614b)
- Add audio command boundary (aa0aada)
- Add single-instance plugin and handle silent recordings (a8144cd)
- Add inhouse dev variant with custom icons and hotkey labels (b970923)

### Bug Fixes

- Update changelog page layout (928b0da)
- Shortcut not working when no permission (6a97b91)
- Prevent changelog fetch loop and improve UI (79c07fa)

## v0.3.0 (2026-04-13)

### Features

- Improve audio chunking (9e2f54d)
- Add recording cancellation and enhanced retry experience (a4ae864)
- Add transcription retry functionality (ac33fca)
- Refactor hotkey (c2a744d)

## v0.2.0 (2026-04-11)

### Features

- Add model file size validation and metal headers (5c0743d)

## v0.1.2 (2026-04-08)

### Features

- Add custom template management and improve VAD (fdc3940)

### Bug Fixes

- Return committed transcript from ElevenLabs finish() and add query params (84c3a40)

## v0.1.1 (2026-04-06)

### Features

- Add Gemma 2B IT local model support (2a3769b)
- Add history dashboard, cloud service UI and improve design (bc1bc8a)

### Bug Fixes

- Eliminate flash of unstyled content on app startup (c092c9b)

## v0.1.0-beta.8 (2026-03-09)

### Bug Fixes

- Embed beep audio files at compile time (e8ceda6)
