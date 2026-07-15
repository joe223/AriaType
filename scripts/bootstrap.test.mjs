import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  auditWorkspace,
  formatAuditReport,
  installMissingPrerequisites,
} from './bootstrap.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function createProbe({
  commands = [],
  cargoSubcommands = [],
  rustTargets = [],
  runtimeResources = {},
} = {}) {
  return {
    commandAvailable(command) {
      return commands.includes(command);
    },
    cargoSubcommandAvailable(command) {
      return cargoSubcommands.includes(command);
    },
    rustTargetAvailable(target) {
      return rustTargets.includes(target);
    },
    runtimeResources(platform) {
      return runtimeResources[platform] ?? [];
    },
    brewFormulaAvailable(formula) {
      return commands.includes(formula);
    },
  };
}

test('audit reports all missing macOS build prerequisites without exposing secrets', () => {
  const env = {
    APPLE_ID: 'developer@example.com',
    APPLE_PASSWORD: 'do-not-print-this',
    APPLE_TEAM_ID: 'TEAM123456',
    APPLE_SIGNING_IDENTITY: 'Developer ID Application: Example',
    TAURI_SIGNING_PRIVATE_KEY: 'private-key-contents',
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: 'another-secret',
    TAURI_UPDATER_PUBKEY: 'public-key',
  };
  const probe = createProbe({
    commands: ['node', 'corepack', 'cargo', 'rustc', 'tar', 'brew', 'ninja', 'nsis', 'llvm'],
    cargoSubcommands: ['xwin'],
    rustTargets: ['aarch64-apple-darwin'],
    runtimeResources: {
      windows: ['bin/windows/llama-server.exe'],
    },
  });

  const audit = auditWorkspace({ platform: 'darwin', env, probe });
  const missingIds = audit.items.filter((item) => !item.ok).map((item) => item.id);

  assert.deepEqual(missingIds, [
    'cargo-tauri',
    'rust-target-x86_64-apple-darwin',
    'runtime-macos-arm64',
    'runtime-macos-x64',
  ]);
  const report = formatAuditReport(audit);
  assert.match(report, /cargo tauri/);
  assert.match(report, /x86_64-apple-darwin/);
  assert.doesNotMatch(report, /do-not-print-this|private-key-contents|another-secret/);
});

test('install mode selects only missing supported public prerequisites', async () => {
  const calls = [];
  const audit = {
    items: [
      { id: 'cargo-tauri', ok: false, installer: 'cargo-tauri' },
      { id: 'cargo-xwin', ok: true, installer: 'cargo-xwin' },
      { id: 'ninja', ok: false, installer: 'brew:ninja' },
      { id: 'llvm', ok: false, installer: 'brew:llvm' },
      { id: 'runtime-macos-arm64', ok: false, installer: 'runtime:macos' },
      { id: 'runtime-macos-x64', ok: false, installer: 'runtime:macos' },
      { id: 'apple-signing', ok: false },
    ],
  };

  const result = await installMissingPrerequisites(audit, {
    rootDir: root,
    run: async (command, args) => calls.push([command, ...args]),
  });

  assert.deepEqual(calls, [
    ['brew', 'install', 'ninja', 'llvm'],
    ['cargo', 'install', 'tauri-cli', '--version', '^2.0.0', '--locked'],
    ['node', resolve(root, 'scripts/ensure-llama-server-runtime.mjs'), '--platform', 'macos'],
    ['corepack', 'pnpm', 'install', '--frozen-lockfile'],
  ]);
  assert.deepEqual(result.manual, ['apple-signing']);
});

test('root package and shell wrapper expose bootstrap workflow', () => {
  const packageJson = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8'));
  const wrapper = readFileSync(resolve(root, 'bootstrap.sh'), 'utf8');

  assert.equal(packageJson.scripts.bootstrap, 'node scripts/bootstrap.mjs');
  assert.match(wrapper, /command -v node/);
  assert.match(wrapper, /scripts\/bootstrap\.mjs/);
});
