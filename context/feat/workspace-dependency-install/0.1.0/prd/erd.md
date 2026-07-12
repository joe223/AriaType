# Workspace Dependency Installation

## Version

- Feature: `workspace-dependency-install`
- Version: `0.1.0`
- Status: Completed

## Problem

The root dependency overrides force legacy image-optimization installers onto
incompatible major versions of `got` and `file-type`. As a result,
`pngquant-bin@9.0.0` cannot download its prebuilt binary and its source-build
fallback crashes before extraction, so a workspace install exits with
`ELIFECYCLE`.

## Goal

Make a normal install with the repository-pinned pnpm version complete all
required lifecycle scripts without bypassing scripts or migrating the package
manager and lockfile format.

## First-Principles Model

- A transitive security override is safe only when it preserves the API range
  declared by every affected consumer.
- Legacy installer dependencies must resolve their own compatible majors.
- The lockfile, not an already-populated package store, is the reproducible
  proof of the dependency graph.
- Required lifecycle failures must remain visible; `--ignore-scripts` is not a
  valid repair.

## Resolution Contract

1. `packageManager` remains `pnpm@8.15.0` and the lockfile remains version 6.
2. Root overrides do not force every `got` or `file-type` consumer to one
   incompatible major.
3. `decompress-tar@4.1.1` resolves `file-type` 5.x.
4. `download@7.1.0` resolves `got` 8.x.
5. `pngquant-bin@9.0.0` finishes its required postinstall script.

## Acceptance Criteria

1. `corepack pnpm install --no-frozen-lockfile` exits successfully on the
   supported macOS development environment.
2. No install command needs `--force`, `--ignore-scripts`, or optional-package
   promotion/demotion to succeed.
3. The regenerated lockfile encodes consumer-compatible `got` and `file-type`
   versions rather than the two blanket overrides.
4. The desktop frontend build, shared package typecheck, and i18n validation
   still pass after dependency resolution changes.

## BDD Scenarios

### Scenario: install from the corrected dependency graph

- Given the repository uses its pinned pnpm 8.15.0 toolchain
- When dependencies are installed with lifecycle scripts enabled
- Then the required `pngquant-bin` postinstall completes
- And the install exits with status zero

### Scenario: legacy consumers receive compatible APIs

- Given the image-optimization dependency chain contains legacy consumers
- When pnpm resolves their transitive dependencies
- Then `decompress-tar@4.1.1` receives `file-type` 5.x
- And `download@7.1.0` receives `got` 8.x

## Verification

- dependency graph contract check against installed package links
- `corepack pnpm install --no-frozen-lockfile`
- `corepack pnpm --filter @ariatype/desktop build`
- `corepack pnpm --filter @ariatype/shared typecheck`
- `corepack pnpm check:i18n`

## Verification Results

- The failing-first graph check observed `file-type@22.0.1` and `got@11.8.5`.
- After lockfile regeneration, the same check observed `file-type@5.2.0` and
  `got@8.3.2` and exited successfully.
- The full lifecycle install completed, including `pngquant`, `optipng`,
  `gifsicle`, and `jpegtran` prebuild checks.
- `pngquant --version` returned `3.0.3`.
- Frozen installation, desktop tests/build, shared typecheck, i18n validation,
  website build/lint, and Markdown link validation completed successfully.
