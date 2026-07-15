---
title: Workspace Bootstrap
type: feat
status: completed
date: 2026-07-12
---

# Workspace Bootstrap Plan

## Overview

Add a repository-root audit and opt-in installation workflow so build-machine
requirements are validated and prepared consistently before the release build.

## Problem Frame

Current state: developers learn about Homebrew tools, Cargo plugins, Rust
targets, ignored runtime binaries, and signing variables through sequential
build failures or knowledge copied from another machine.

Desired state: one command produces a complete, secret-safe audit, while an
explicit install mode prepares all supported public dependencies and reports
the remaining private or unsupported actions.

## Scope Boundaries

In scope:

- cross-platform Node audit logic and root shell entry point
- macOS public tool and Cargo plugin installation
- pinned macOS and Windows local polish runtime preparation
- workspace dependency installation through pinned pnpm
- presence-only release signing checks
- npm script, tests, and contributor documentation

Out of scope:

- generating, importing, or validating private signing identities
- replacing an existing Rust installation automatically
- building or publishing release artifacts
- changing application runtime behavior

## Implementation Units

1. Add failing tests for audit results, command selection, redaction, and CLI
   registration.
2. Extend pinned llama.cpp runtime preparation to support macOS assets.
3. Implement the injectable bootstrap audit and installer orchestration.
4. Add the root shell wrapper and npm script.
5. Update contributor and feature documentation.
6. Run focused and repository-wide script verification and record evidence.

## System-Wide Impact

- Release prerequisites are now discoverable before the expensive build starts.
- Ignored runtime resources remain outside source control but become
  reproducibly recoverable from the pinned upstream release.
- Existing build commands and application behavior remain unchanged.

## Risks & Dependencies

- Public tool installation requires network access and may require Homebrew to
  update its package metadata.
- Homebrew Rust cannot add non-host standard-library targets; bootstrap must
  request a rustup-managed toolchain rather than replacing Rust automatically.
- Upstream llama.cpp release asset naming is discovered from GitHub release
  metadata and must remain covered by deterministic tests.

## Verification Evidence

- Failing-first focused tests: missing `scripts/bootstrap.mjs` and unsupported
  macOS Runtime preparation were both observed.
- Focused tests: 12 passed.
- Full `scripts/*.test.mjs` suite: 99 passed.
- `sh -n bootstrap.sh`: passed.
- `npm run bootstrap -- --help`: passed.
- Current-machine read-only audit: correctly reported `cargo-tauri`, Intel Rust
  target, and both macOS Runtime artifacts as missing.
- `pnpm check:md-links`: passed.
