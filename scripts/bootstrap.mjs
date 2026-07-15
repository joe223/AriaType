#!/usr/bin/env node

import { existsSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';

import { existingRuntimeResources } from './prepare-tauri-runtime-resources.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const defaultRoot = resolve(__dirname, '..');

const APPLE_SIGNING_ENV = [
  'APPLE_SIGNING_IDENTITY',
  'APPLE_ID',
  'APPLE_PASSWORD',
  'APPLE_TEAM_ID',
];
const UPDATER_SIGNING_ENV = ['TAURI_UPDATER_PUBKEY'];

function resultSucceeded(result) {
  return !result.error && result.status === 0;
}

function commandAvailable(command) {
  return resultSucceeded(spawnSync(command, ['--version'], { stdio: 'ignore' }));
}

function cargoSubcommandAvailable(subcommand) {
  return resultSucceeded(spawnSync('cargo', [subcommand, '--version'], { stdio: 'ignore' }));
}

function rustTargetAvailable(target) {
  const result = spawnSync('rustc', ['--print', 'target-libdir', '--target', target], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'ignore'],
  });
  if (!resultSucceeded(result)) {
    return false;
  }
  const libDir = result.stdout.trim();
  return existsSync(libDir) && readdirSync(libDir).some((entry) => entry.endsWith('.rlib'));
}

function brewFormulaAvailable(formula) {
  return resultSucceeded(spawnSync('brew', ['--prefix', formula], { stdio: 'ignore' }));
}

export function createSystemProbe(rootDir = defaultRoot) {
  return {
    commandAvailable,
    cargoSubcommandAvailable,
    rustTargetAvailable,
    brewFormulaAvailable,
    runtimeResources(platform) {
      return existingRuntimeResources(platform, rootDir);
    },
  };
}

function configured(env, name) {
  return typeof env[name] === 'string' && env[name].trim().length > 0;
}

function item(id, label, ok, { hint = '', installer } = {}) {
  return { id, label, ok, hint, installer };
}

function commandItem(probe, id, label, command, hint) {
  return item(id, label, probe.commandAvailable(command), { hint });
}

function runtimeConfigured(env, names) {
  return names.some((name) => configured(env, name));
}

function signingItem(id, label, env, requiredNames, alternativeNames = []) {
  const missing = requiredNames.filter((name) => !configured(env, name));
  if (alternativeNames.length > 0 && !alternativeNames.some((name) => configured(env, name))) {
    missing.push(alternativeNames.join(' or '));
  }
  return item(id, label, missing.length === 0, {
    hint: missing.length === 0 ? '' : `Set ${missing.join(', ')}`,
  });
}

export function auditWorkspace({
  platform = process.platform,
  env = process.env,
  probe = createSystemProbe(),
  nodeVersion = process.versions.node,
} = {}) {
  const items = [];
  const nodeMajor = Number.parseInt(nodeVersion.split('.')[0], 10);
  items.push(item('node', `Node.js >= 18 (current: ${nodeVersion})`, nodeMajor >= 18, {
    hint: 'Install Node.js 18 or newer',
  }));
  items.push(commandItem(probe, 'corepack', 'Corepack', 'corepack', 'Install a Node.js distribution that includes Corepack'));
  items.push(commandItem(probe, 'cargo', 'Cargo', 'cargo', 'Install a supported Rust toolchain'));
  items.push(commandItem(probe, 'rustc', 'Rust compiler', 'rustc', 'Install a supported Rust toolchain'));
  items.push(commandItem(probe, 'tar', 'tar archive tool', 'tar', 'Install tar with the platform package manager'));

  const cargoAvailable = probe.commandAvailable('cargo');
  items.push(item('cargo-tauri', 'cargo tauri', cargoAvailable && probe.cargoSubcommandAvailable('tauri'), {
    hint: 'cargo install tauri-cli --version "^2.0.0" --locked',
    installer: cargoAvailable ? 'cargo-tauri' : undefined,
  }));

  if (platform === 'darwin') {
    items.push(commandItem(probe, 'homebrew', 'Homebrew', 'brew', 'Install Homebrew before using --install'));
  }

  if (platform !== 'win32') {
    items.push(item('cargo-xwin', 'cargo xwin', cargoAvailable && probe.cargoSubcommandAvailable('xwin'), {
      hint: 'cargo install cargo-xwin',
      installer: cargoAvailable ? 'cargo-xwin' : undefined,
    }));
    for (const [id, label, command, formula] of [
      ['ninja', 'Ninja', 'ninja', 'ninja'],
      ['llvm', 'LLVM', 'llvm-config', 'llvm'],
      ['nsis', 'NSIS', 'makensis', 'nsis'],
    ]) {
      const brewAvailable = platform === 'darwin' && probe.commandAvailable('brew');
      const ok = probe.commandAvailable(command)
        || (brewAvailable && probe.brewFormulaAvailable(formula));
      items.push(item(id, label, ok, {
        hint: platform === 'darwin'
          ? `brew install ${formula}`
          : `Install ${label} with the system package manager`,
        installer: brewAvailable ? `brew:${formula}` : undefined,
      }));
    }
  }

  if (platform === 'darwin') {
    for (const target of ['aarch64-apple-darwin', 'x86_64-apple-darwin']) {
      const ok = probe.rustTargetAvailable(target);
      const rustupAvailable = probe.commandAvailable('rustup');
      items.push(item(`rust-target-${target}`, `Rust target ${target}`, ok, {
        hint: rustupAvailable
          ? `rustup target add ${target}`
          : `Install a rustup-managed toolchain, then run rustup target add ${target}`,
        installer: !ok && rustupAvailable ? `rustup:${target}` : undefined,
      }));
    }

    const macResources = probe.runtimeResources('macos');
    const macRuntimeConfigured = runtimeConfigured(env, ['ARIATYPE_LLAMA_SERVER_MACOS_PATH']);
    const armRuntimeOk = macRuntimeConfigured
      || runtimeConfigured(env, ['ARIATYPE_LLAMA_SERVER_MACOS_ARM64_PATH'])
      || macResources.includes('bin/apple-silicon/llama-server')
      || macResources.includes('bin/universal/llama-server');
    const x64RuntimeOk = macRuntimeConfigured
      || runtimeConfigured(env, ['ARIATYPE_LLAMA_SERVER_MACOS_X64_PATH'])
      || macResources.includes('bin/intel/llama-server')
      || macResources.includes('bin/universal/llama-server');
    items.push(item('runtime-macos-arm64', 'macOS arm64 local polish runtime', armRuntimeOk, {
      hint: 'Run npm run bootstrap -- --install or configure ARIATYPE_LLAMA_SERVER_MACOS_ARM64_PATH',
      installer: 'runtime:macos',
    }));
    items.push(item('runtime-macos-x64', 'macOS x64 local polish runtime', x64RuntimeOk, {
      hint: 'Run npm run bootstrap -- --install or configure ARIATYPE_LLAMA_SERVER_MACOS_X64_PATH',
      installer: 'runtime:macos',
    }));
    items.push(signingItem('apple-signing', 'Apple release signing environment', env, APPLE_SIGNING_ENV));
  }

  const windowsResources = probe.runtimeResources('windows');
  const windowsRuntimeOk = runtimeConfigured(env, [
    'ARIATYPE_LLAMA_SERVER_WINDOWS_X64_PATH',
    'ARIATYPE_LLAMA_SERVER_WINDOWS_PATH',
  ]) || windowsResources.includes('bin/windows/llama-server.exe');
  items.push(item('runtime-windows-x64', 'Windows x64 local polish runtime', windowsRuntimeOk, {
    hint: 'Run npm run bootstrap -- --install',
    installer: 'runtime:windows',
  }));

  items.push(signingItem(
    'updater-signing',
    'Tauri updater signing environment',
    env,
    UPDATER_SIGNING_ENV,
    ['TAURI_SIGNING_PRIVATE_KEY', 'TAURI_SIGNING_PRIVATE_KEY_PATH']
  ));

  return {
    ok: items.every((entry) => entry.ok),
    platform,
    items,
  };
}

export function formatAuditReport(audit) {
  const lines = [`AriaType bootstrap audit (${audit.platform})`, ''];
  for (const entry of audit.items) {
    lines.push(`${entry.ok ? 'OK' : 'MISSING'}  ${entry.label}`);
    if (!entry.ok && entry.hint) {
      lines.push(`         ${entry.hint}`);
    }
  }
  const missingCount = audit.items.filter((entry) => !entry.ok).length;
  lines.push('', missingCount === 0
    ? 'All required prerequisites are ready.'
    : `${missingCount} prerequisite(s) need attention.`);
  return lines.join('\n');
}

function runInherited(command, args, { cwd = defaultRoot } = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { cwd, stdio: 'inherit', shell: false });
    child.on('error', reject);
    child.on('exit', (code) => {
      if (code === 0) {
        resolvePromise();
      } else {
        reject(new Error(`${command} ${args.join(' ')} exited with code ${code}`));
      }
    });
  });
}

export async function installMissingPrerequisites(audit, {
  rootDir = defaultRoot,
  run = runInherited,
} = {}) {
  const missing = audit.items.filter((entry) => !entry.ok);
  const installers = new Set(missing.map((entry) => entry.installer).filter(Boolean));
  const manual = missing.filter((entry) => !entry.installer).map((entry) => entry.id);

  const brewFormulas = [...installers]
    .filter((installer) => installer.startsWith('brew:'))
    .map((installer) => installer.slice('brew:'.length));
  if (brewFormulas.length > 0) {
    await run('brew', ['install', ...brewFormulas], { cwd: rootDir });
  }
  if (installers.has('cargo-tauri')) {
    await run('cargo', ['install', 'tauri-cli', '--version', '^2.0.0', '--locked'], { cwd: rootDir });
  }
  if (installers.has('cargo-xwin')) {
    await run('cargo', ['install', 'cargo-xwin'], { cwd: rootDir });
  }
  for (const installer of installers) {
    if (installer.startsWith('rustup:')) {
      await run('rustup', ['target', 'add', installer.slice('rustup:'.length)], { cwd: rootDir });
    }
  }
  for (const platform of ['macos', 'windows']) {
    if (installers.has(`runtime:${platform}`)) {
      await run('node', [resolve(rootDir, 'scripts/ensure-llama-server-runtime.mjs'), '--platform', platform], { cwd: rootDir });
    }
  }
  await run('corepack', ['pnpm', 'install', '--frozen-lockfile'], { cwd: rootDir });
  return { manual };
}

function printHelp() {
  console.log(`Usage: npm run bootstrap -- [--install]\n\nOptions:\n  --install  Install supported public prerequisites, prepare runtimes, and install workspace packages\n  --help     Show this help\n\nWithout --install, bootstrap performs a read-only audit.`);
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes('--help') || args.includes('-h')) {
    printHelp();
    return;
  }
  const unknown = args.filter((arg) => arg !== '--install');
  if (unknown.length > 0) {
    throw new Error(`Unknown bootstrap option(s): ${unknown.join(', ')}`);
  }

  let audit = auditWorkspace();
  console.log(formatAuditReport(audit));
  if (args.includes('--install')) {
    console.log('\nInstalling supported public prerequisites...\n');
    const result = await installMissingPrerequisites(audit);
    if (result.manual.length > 0) {
      console.log(`\nManual prerequisites remain: ${result.manual.join(', ')}`);
    }
    audit = auditWorkspace();
    console.log(`\n${formatAuditReport(audit)}`);
  }
  if (!audit.ok) {
    process.exitCode = 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
