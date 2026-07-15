#!/usr/bin/env node
import {
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { prepareLlamaServerReleaseAssets } from './prepare-llama-server-release-assets.mjs';
import {
  existingRuntimeResources,
  normalizePlatform,
  runtimeSidecarSpecs,
} from './prepare-tauri-runtime-resources.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const defaultRoot = resolve(__dirname, '..');

export const DEFAULT_LLAMA_CPP_RELEASE_TAG = 'b9568';
const LLAMA_CPP_RELEASE_API = 'https://api.github.com/repos/ggml-org/llama.cpp/releases/tags';

export function configuredRuntimePathEnv(platformValue, env = process.env) {
  const platform = normalizePlatform(platformValue);

  return (
    runtimeSidecarSpecs(platform).find((spec) => {
      const value = env[spec.pathEnv];
      return typeof value === 'string' && value.trim().length > 0;
    })?.pathEnv ?? null
  );
}

function runtimeSetComplete(platform, resources) {
  if (platform === 'macos') {
    return resources.includes('bin/universal/llama-server') || (
      resources.includes('bin/apple-silicon/llama-server')
      && resources.includes('bin/intel/llama-server')
    );
  }
  if (platform === 'windows') {
    return resources.includes('bin/windows/llama-server.exe');
  }
  return resources.includes('bin/linux/llama-server') || resources.includes('bin/llama-server');
}

function configuredRuntimeSetComplete(platform, env) {
  if (platform === 'macos') {
    return Boolean(env.ARIATYPE_LLAMA_SERVER_MACOS_PATH?.trim()) || (
      Boolean(env.ARIATYPE_LLAMA_SERVER_MACOS_ARM64_PATH?.trim())
      && Boolean(env.ARIATYPE_LLAMA_SERVER_MACOS_X64_PATH?.trim())
    );
  }
  return configuredRuntimePathEnv(platform, env) !== null;
}

export function detectPinnedLlamaCppReleaseTag(rootDir = defaultRoot) {
  const workflowPath = resolve(rootDir, '.github/workflows/release.yml');
  if (!existsSync(workflowPath)) {
    return DEFAULT_LLAMA_CPP_RELEASE_TAG;
  }

  const workflow = readFileSync(workflowPath, 'utf8');
  const match = workflow.match(/LLAMA_CPP_RELEASE_TAG:\s*["']?(b\d+)["']?/);
  return match?.[1] ?? DEFAULT_LLAMA_CPP_RELEASE_TAG;
}

export function officialLlamaServerAssetName(platformValue, releaseTag) {
  const platform = normalizePlatform(platformValue);
  if (platform === 'windows') {
    return `llama-${releaseTag}-bin-win-cpu-x64.zip`;
  }

  throw new Error(`Automatic local polish runtime download is unsupported for ${platform}`);
}

export function officialLlamaServerAssetUrl(platformValue, releaseTag) {
  return `https://github.com/ggml-org/llama.cpp/releases/download/${releaseTag}/${officialLlamaServerAssetName(platformValue, releaseTag)}`;
}

function macosAssetArchitecture(assetName) {
  if (!/macos/i.test(assetName) || !/\.(zip|tar\.gz|tgz)$/i.test(assetName)) {
    return null;
  }
  if (/(arm64|aarch64)/i.test(assetName)) {
    return 'arm64';
  }
  if (/(x64|x86_64)/i.test(assetName)) {
    return 'x64';
  }
  return null;
}

export async function discoverOfficialMacosAssets(releaseTag, fetchImpl = globalThis.fetch) {
  if (typeof fetchImpl !== 'function') {
    throw new Error('fetch is unavailable; cannot discover local polish runtime assets');
  }

  const releaseUrl = `${LLAMA_CPP_RELEASE_API}/${releaseTag}`;
  const response = await fetchImpl(releaseUrl, {
    headers: {
      accept: 'application/vnd.github+json',
      'user-agent': 'AriaType build/runtime preparation',
    },
    redirect: 'follow',
  });
  if (!response.ok) {
    throw new Error(
      `Failed to discover local polish runtime assets: ${releaseUrl} (${response.status} ${response.statusText})`
    );
  }

  const release = await response.json();
  const selected = new Map();
  for (const asset of release.assets ?? []) {
    const architecture = macosAssetArchitecture(asset.name ?? '');
    if (architecture && !selected.has(architecture) && asset.browser_download_url) {
      selected.set(architecture, {
        architecture,
        assetName: asset.name,
        assetUrl: asset.browser_download_url,
      });
    }
  }

  const missing = ['arm64', 'x64'].filter((architecture) => !selected.has(architecture));
  if (missing.length > 0) {
    throw new Error(
      `Missing official macOS llama.cpp release asset(s) for ${releaseTag}: ${missing.join(', ')}`
    );
  }
  return ['arm64', 'x64'].map((architecture) => selected.get(architecture));
}

async function downloadAsset({ assetName, assetUrl, assetsDir, fetchImpl }) {
  mkdirSync(assetsDir, { recursive: true });
  const assetPath = resolve(assetsDir, assetName);
  if (existsSync(assetPath)) {
    return { downloaded: false, assetName, assetPath, assetUrl };
  }

  const response = await fetchImpl(assetUrl, {
    headers: {
      'user-agent': 'AriaType build/runtime preparation',
    },
    redirect: 'follow',
  });
  if (!response.ok) {
    throw new Error(
      `Failed to download local polish runtime asset: ${assetUrl} (${response.status} ${response.statusText})`
    );
  }
  writeFileSync(assetPath, Buffer.from(await response.arrayBuffer()));
  return { downloaded: true, assetName, assetPath, assetUrl };
}

export async function downloadOfficialLlamaServerAsset({
  platform,
  releaseTag,
  assetsDir,
  fetchImpl = globalThis.fetch,
} = {}) {
  if (typeof fetchImpl !== 'function') {
    throw new Error('fetch is unavailable; cannot download local polish runtime asset');
  }

  const assetName = officialLlamaServerAssetName(platform, releaseTag);
  const assetUrl = officialLlamaServerAssetUrl(platform, releaseTag);
  return {
    ...(await downloadAsset({ assetName, assetUrl, assetsDir, fetchImpl })),
    releaseTag,
  };
}

export async function ensureLlamaServerRuntime({
  platform = process.platform,
  rootDir = defaultRoot,
  env = process.env,
  assetsDir = resolve(rootDir, '.tmp/llama-server-assets'),
  releaseTag = detectPinnedLlamaCppReleaseTag(rootDir),
  fetchImpl = globalThis.fetch,
  prepareAssets = prepareLlamaServerReleaseAssets,
} = {}) {
  const normalizedPlatform = normalizePlatform(platform);
  const runtimeResources = existingRuntimeResources(normalizedPlatform, rootDir);
  if (runtimeSetComplete(normalizedPlatform, runtimeResources)) {
    return {
      status: 'already_present',
      platform: normalizedPlatform,
      resources: runtimeResources,
    };
  }

  const configuredPathEnv = configuredRuntimePathEnv(normalizedPlatform, env);
  if (configuredRuntimeSetComplete(normalizedPlatform, env)) {
    return {
      status: 'env_configured',
      platform: normalizedPlatform,
      pathEnv: configuredPathEnv,
    };
  }

  if (normalizedPlatform !== 'windows' && normalizedPlatform !== 'macos') {
    return {
      status: 'skipped',
      platform: normalizedPlatform,
      reason: 'auto_download_not_supported',
    };
  }

  const downloads = normalizedPlatform === 'macos'
    ? await Promise.all(
      (await discoverOfficialMacosAssets(releaseTag, fetchImpl)).map((asset) =>
        downloadAsset({ ...asset, assetsDir, fetchImpl })
      )
    )
    : [await downloadOfficialLlamaServerAsset({
      platform: normalizedPlatform,
      releaseTag,
      assetsDir,
      fetchImpl,
    })];
  const prepared = prepareAssets({
    platform: normalizedPlatform,
    assetsDir,
    rootDir,
  });

  return {
    status: 'prepared',
    platform: normalizedPlatform,
    releaseTag,
    assetName: downloads[0].assetName,
    assetNames: downloads.map((download) => download.assetName),
    assetPath: downloads[0].assetPath,
    assetUrl: downloads[0].assetUrl,
    downloaded: downloads.some((download) => download.downloaded),
    prepared,
  };
}

function parseArgs(args) {
  const getValue = (name) => {
    const index = args.indexOf(name);
    return index === -1 ? undefined : args[index + 1];
  };

  return {
    platform: getValue('--platform') ?? process.platform,
    assetsDir: getValue('--assets-dir'),
    releaseTag: getValue('--release-tag'),
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const result = await ensureLlamaServerRuntime({
    platform: options.platform,
    assetsDir: options.assetsDir,
    releaseTag: options.releaseTag,
  });

  if (result.status === 'already_present') {
    console.log(`Local polish runtime already present for ${result.platform}.`);
    return;
  }

  if (result.status === 'env_configured') {
    console.log(
      `Local polish runtime for ${result.platform} will be prepared from ${result.pathEnv}.`
    );
    return;
  }

  if (result.status === 'skipped') {
    console.log(`Automatic local polish runtime download is disabled for ${result.platform}.`);
    return;
  }

  const downloadState = result.downloaded ? 'downloaded' : 'reused cached';
  console.log(
    `Prepared ${result.platform} local polish runtime from ${result.assetNames.join(', ')} (${downloadState}).`
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
