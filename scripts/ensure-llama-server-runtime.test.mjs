import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

import {
  DEFAULT_LLAMA_CPP_RELEASE_TAG,
  configuredRuntimePathEnv,
  detectPinnedLlamaCppReleaseTag,
  ensureLlamaServerRuntime,
  officialLlamaServerAssetName,
  officialLlamaServerAssetUrl,
} from './ensure-llama-server-runtime.mjs';

function createFixture() {
  const root = mkdtempSync(join(tmpdir(), 'ariatype-ensure-runtime-'));
  const tauriDir = resolve(root, 'apps/desktop/src-tauri');
  mkdirSync(tauriDir, { recursive: true });
  return { root, tauriDir };
}

test('configuredRuntimePathEnv prefers Windows x64 env', () => {
  assert.equal(
    configuredRuntimePathEnv('windows', {
      ARIATYPE_LLAMA_SERVER_WINDOWS_X64_PATH: '/tmp/llama-server.exe',
      ARIATYPE_LLAMA_SERVER_WINDOWS_PATH: '/tmp/generic.exe',
    }),
    'ARIATYPE_LLAMA_SERVER_WINDOWS_X64_PATH'
  );
});

test('detectPinnedLlamaCppReleaseTag falls back when workflow file is absent', () => {
  const { root } = createFixture();
  assert.equal(detectPinnedLlamaCppReleaseTag(root), DEFAULT_LLAMA_CPP_RELEASE_TAG);
});

test('detectPinnedLlamaCppReleaseTag reads the release workflow pin', () => {
  const { root } = createFixture();
  const workflowPath = resolve(root, '.github/workflows/release.yml');
  mkdirSync(dirname(workflowPath), { recursive: true });
  writeFileSync(workflowPath, 'env:\n  LLAMA_CPP_RELEASE_TAG: b1234\n');

  assert.equal(detectPinnedLlamaCppReleaseTag(root), 'b1234');
});

test('official Windows asset name and URL use the pinned release tag', () => {
  assert.equal(
    officialLlamaServerAssetName('windows', 'b9568'),
    'llama-b9568-bin-win-cpu-x64.zip'
  );
  assert.equal(
    officialLlamaServerAssetUrl('windows', 'b9568'),
    'https://github.com/ggml-org/llama.cpp/releases/download/b9568/llama-b9568-bin-win-cpu-x64.zip'
  );
});

test('ensureLlamaServerRuntime is a no-op when a Windows runtime already exists', async () => {
  const { root, tauriDir } = createFixture();
  const runtimePath = resolve(tauriDir, 'bin/windows/llama-server.exe');
  mkdirSync(dirname(runtimePath), { recursive: true });
  writeFileSync(runtimePath, 'existing runtime');

  let prepared = false;
  const result = await ensureLlamaServerRuntime({
    platform: 'windows',
    rootDir: root,
    fetchImpl: async () => {
      throw new Error('should not download');
    },
    prepareAssets() {
      prepared = true;
      return [];
    },
  });

  assert.equal(result.status, 'already_present');
  assert.equal(prepared, false);
});

test('ensureLlamaServerRuntime is a no-op when a runtime env var is configured', async () => {
  const { root } = createFixture();
  const result = await ensureLlamaServerRuntime({
    platform: 'windows',
    rootDir: root,
    env: {
      ARIATYPE_LLAMA_SERVER_WINDOWS_PATH: '/tmp/llama-server.exe',
    },
    fetchImpl: async () => {
      throw new Error('should not download');
    },
  });

  assert.deepEqual(result, {
    status: 'env_configured',
    platform: 'windows',
    pathEnv: 'ARIATYPE_LLAMA_SERVER_WINDOWS_PATH',
  });
});

test('ensureLlamaServerRuntime downloads and prepares a missing Windows runtime', async () => {
  const { root } = createFixture();
  const assetsDir = resolve(root, '.tmp/llama-server-assets');
  let prepareCall = null;

  const result = await ensureLlamaServerRuntime({
    platform: 'windows',
    rootDir: root,
    assetsDir,
    releaseTag: 'b9568',
    fetchImpl: async () => ({
      ok: true,
      arrayBuffer: async () => Buffer.from('zip payload'),
    }),
    prepareAssets(options) {
      prepareCall = options;
      return [
        {
          key: 'windows-x64-cpu',
          destinationResource: 'bin/windows/llama-server.exe',
        },
      ];
    },
  });

  assert.equal(result.status, 'prepared');
  assert.equal(result.downloaded, true);
  assert.equal(result.assetName, 'llama-b9568-bin-win-cpu-x64.zip');
  assert.equal(
    readFileSync(resolve(assetsDir, 'llama-b9568-bin-win-cpu-x64.zip'), 'utf8'),
    'zip payload'
  );
  assert.deepEqual(prepareCall, {
    platform: 'windows',
    assetsDir,
    rootDir: root,
  });
});

test('ensureLlamaServerRuntime downloads and prepares both pinned macOS runtimes', async () => {
  const { root } = createFixture();
  const assetsDir = resolve(root, '.tmp/llama-server-assets');
  const requestedUrls = [];
  let prepareCall = null;
  const result = await ensureLlamaServerRuntime({
    platform: 'macos',
    rootDir: root,
    assetsDir,
    releaseTag: 'b9568',
    fetchImpl: async (url) => {
      requestedUrls.push(url);
      if (url.endsWith('/releases/tags/b9568')) {
        return {
          ok: true,
          json: async () => ({
            assets: [
              {
                name: 'llama-b9568-bin-macos-arm64.zip',
                browser_download_url: 'https://example.test/macos-arm64.zip',
              },
              {
                name: 'llama-b9568-bin-macos-x64.zip',
                browser_download_url: 'https://example.test/macos-x64.zip',
              },
            ],
          }),
        };
      }
      return {
        ok: true,
        arrayBuffer: async () => Buffer.from(`payload:${url}`),
      };
    },
    prepareAssets(options) {
      prepareCall = options;
      return [
        { key: 'macos-arm64', destinationResource: 'bin/apple-silicon/llama-server' },
        { key: 'macos-x64', destinationResource: 'bin/intel/llama-server' },
      ];
    },
  });

  assert.equal(result.status, 'prepared');
  assert.equal(result.downloaded, true);
  assert.deepEqual(result.assetNames, [
    'llama-b9568-bin-macos-arm64.zip',
    'llama-b9568-bin-macos-x64.zip',
  ]);
  assert.deepEqual(requestedUrls, [
    'https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/b9568',
    'https://example.test/macos-arm64.zip',
    'https://example.test/macos-x64.zip',
  ]);
  assert.deepEqual(prepareCall, {
    platform: 'macos',
    assetsDir,
    rootDir: root,
  });
});

test('ensureLlamaServerRuntime repairs a partial macOS runtime set', async () => {
  const { root, tauriDir } = createFixture();
  const armRuntime = resolve(tauriDir, 'bin/apple-silicon/llama-server');
  mkdirSync(dirname(armRuntime), { recursive: true });
  writeFileSync(armRuntime, 'arm-only');

  let prepared = false;
  const result = await ensureLlamaServerRuntime({
    platform: 'macos',
    rootDir: root,
    releaseTag: 'b9568',
    fetchImpl: async (url) => {
      if (url.endsWith('/releases/tags/b9568')) {
        return {
          ok: true,
          json: async () => ({
            assets: [
              { name: 'llama-b9568-bin-macos-arm64.zip', browser_download_url: 'https://example.test/arm.zip' },
              { name: 'llama-b9568-bin-macos-x64.zip', browser_download_url: 'https://example.test/x64.zip' },
            ],
          }),
        };
      }
      return { ok: true, arrayBuffer: async () => Buffer.from('archive') };
    },
    prepareAssets() {
      prepared = true;
      return [];
    },
  });

  assert.equal(result.status, 'prepared');
  assert.equal(prepared, true);
});
