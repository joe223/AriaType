---
title: Fix pnpm Lifecycle Installation
type: fix
status: completed
date: 2026-07-11
---

# Fix pnpm Lifecycle Installation Plan

## Overview

Restore reproducible workspace installation by removing the two blanket
transitive overrides that inject incompatible APIs into legacy binary
installers, then regenerate and verify the pnpm 8 lockfile.

## Problem Frame

Current state: `pngquant-bin@9.0.0` first fails its binary download because
`download@7.1.0` receives `got@11`, then its build fallback crashes because
`decompress-tar@4.1.1` receives `file-type@22`.

Desired state: each consumer resolves a compatible declared major and all
required install lifecycle scripts exit successfully.

## Scope Boundaries

In scope:

- root `got` and `file-type` override removal
- pnpm 8 lockfile regeneration
- install, graph-contract, and frontend verification

Out of scope:

- upgrading pnpm or migrating the lockfile format
- upgrading or replacing the image-optimization toolchain
- broad dependency modernization or unrelated security overrides
- bypassing lifecycle scripts

## Implementation Units

1. Record a failing graph check for the legacy consumers.
2. Remove only the incompatible root overrides.
3. Regenerate the lockfile with the repository-pinned pnpm 8.15.0.
4. Run the full install and relevant project verification.
5. Record evidence and complete the plan.

## System-Wide Impact

- The workspace may contain multiple `got` and `file-type` versions, matching
  the API ranges declared by their consumers.
- Production application behavior is unchanged; this affects dependency
  installation and build tooling only.

## Risks & Dependencies

- Lockfile regeneration may need registry access for newly selected legacy
  versions.
- Other blanket overrides remain unchanged and are outside this repair unless
  they independently block the verified install.
- Existing uncommitted product work must remain untouched.

## Verification Evidence

- Failing-first dependency graph check: failed with
  `decompress-tar@4.1.1 -> file-type@22.0.1` and
  `download@7.1.0 -> got@11.8.5`.
- `corepack pnpm install --no-frozen-lockfile`: passed with pnpm 8.15.0; all
  four image binary postinstall checks passed, including required
  `pngquant-bin@9.0.0`.
- Corrected dependency graph check: passed with `file-type@5.2.0` and
  `got@8.3.2`; the lockfile also contains `file-type@8.1.0` and `got@7.1.0`
  for their respective consumers.
- `pngquant --version`: passed and returned `3.0.3`.
- `corepack pnpm install --frozen-lockfile`: passed with the resolution step
  skipped and the workspace already up to date.
- Desktop frontend: 13 test files / 75 tests passed; production build passed.
- Shared package typecheck and full i18n validation passed.
- Website production build passed; lint exited successfully with one existing
  `<img>` performance warning in `src/app/[lang]/page.tsx`.
- Markdown link validation passed.
