# Workspace Bootstrap

## Version

- Feature: `workspace-bootstrap`
- Version: `0.1.0`
- Status: Completed

## Problem

The repository depends on machine-local build tools, Rust subcommands, target
standard libraries, release credentials, and ignored llama.cpp runtime
artifacts. Those prerequisites are currently discovered one failure at a time,
so two machines with the same checkout can have different build outcomes.

## Goal

Provide one repository-root bootstrap entry point that can audit a machine and,
when explicitly requested, install or prepare public reproducible dependencies
without reading, generating, or replacing private signing credentials.

## First-Principles Model

- A checkout is reproducible only when versioned source and machine-local
  prerequisites are both explicit.
- Auditing must be read-only by default; installing tools is a separate,
  intentional action.
- Public dependencies can be automated, while private credentials remain under
  the developer's control.
- A successful bootstrap means a complete prerequisite set, not merely that one
  executable from each directory exists.

## Information Architecture

The audit is one ordered report with four conceptual sections: base development
tools, release/cross-build tools, bundled Runtime resources, and signing
environment. Each row has a stable identifier, human-readable label, readiness
state, and remediation hint.

## Data Contract

Each audited prerequisite is represented internally as:

| Field | Type | Purpose |
|-------|------|---------|
| `id` | string | Stable machine-readable prerequisite identifier |
| `label` | string | Human-readable audit row |
| `ok` | boolean | Whether the prerequisite is currently ready |
| `hint` | string | Secret-safe remediation shown when missing |
| `installer` | string or undefined | Supported public installation action |

The report may include signing environment variable names but never their
values.

## Behavioral Contract

1. `npm run bootstrap` performs a read-only audit and returns a non-zero status
   when a required build prerequisite is missing.
2. `npm run bootstrap -- --install` installs missing public prerequisites that
   have a supported installer, prepares pinned local polish runtimes, and
   installs workspace dependencies with the repository-pinned pnpm version.
3. The bootstrap reports development, multi-platform packaging, runtime, and
   signing prerequisites separately with actionable remediation.
4. Private Apple and updater signing values are checked only for presence. The
   bootstrap never prints their values or attempts to create or copy them.
5. Unsupported automatic installation leaves a clear manual action and a
   failing final audit instead of silently succeeding.
6. The root shell wrapper provides a useful Node.js error before delegating to
   the cross-platform Node implementation.

## Acceptance Criteria

1. A clean audit identifies missing commands, Rust targets, runtime resources,
   and signing environment names without exposing secret values.
2. Install mode executes only the commands associated with missing public
   prerequisites and then reruns the audit.
3. macOS install mode prepares both Apple Silicon and Intel llama.cpp runtimes
   from the release tag pinned by the repository.
4. Existing prerequisites are not reinstalled or redownloaded.
5. `package.json` exposes the workflow as `npm run bootstrap`.
6. Automated tests cover audit classification, install command selection,
   secret redaction, and pinned macOS runtime preparation.

## BDD Scenarios

### Scenario: audit a partially configured macOS machine

- Given Node, Cargo, and Homebrew are available
- And Cargo Tauri, an Intel Rust target, and macOS runtime artifacts are absent
- When the developer runs `npm run bootstrap`
- Then the audit lists each missing prerequisite and its remediation
- And exits non-zero without changing the machine

### Scenario: install public prerequisites

- Given supported public tools or runtimes are missing
- When the developer runs `npm run bootstrap -- --install`
- Then the bootstrap invokes the appropriate package manager or repository
  preparation command
- And reruns the audit after installation

### Scenario: signing secrets are absent

- Given one or more signing environment variables are not configured
- When bootstrap audits release prerequisites
- Then it prints only the missing environment variable names
- And never prints any configured secret value

## Verification

- `node --test scripts/bootstrap.test.mjs scripts/ensure-llama-server-runtime.test.mjs`
- `npm run bootstrap -- --help`
- `npm run bootstrap` on the current macOS machine to verify actionable output
- `node --test scripts/*.test.mjs`

## Verification Results

- Failing-first tests observed the missing bootstrap module and unsupported
  macOS Runtime preparation before implementation.
- Focused bootstrap and Runtime tests passed: 12 tests.
- The full repository script suite passed: 99 tests, including the localhost
  Runtime smoke test.
- `npm run bootstrap -- --help` passed and documented audit/install modes.
- The read-only audit on the current macOS machine correctly reported four
  remaining local prerequisites without exposing configured signing values.
- The shell syntax check and Markdown link validation passed.
