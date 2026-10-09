import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';

const releaseWorkflow = readFileSync(new URL('../.github/workflows/release.yml', import.meta.url), 'utf8');
const manifestGenerator = fileURLToPath(new URL('./generate-release-manifests.mjs', import.meta.url));

function uploadPatternsFor(artifactName) {
  const uploadStep = releaseWorkflow
    .split(/\n\s+- uses: actions\/upload-artifact@/)
    .find((step) => step.split('\n').some((line) => line.trim() === `name: ${artifactName}`));
  assert.ok(uploadStep, `Missing release upload step for ${artifactName}`);

  const lines = uploadStep.split('\n');
  const pathIndex = lines.findIndex((line) => line.trim() === 'path: |');
  assert.notEqual(pathIndex, -1, `Missing upload paths for ${artifactName}`);
  const pathIndent = lines[pathIndex].match(/^\s*/)[0].length;
  const patterns = [];
  for (const line of lines.slice(pathIndex + 1)) {
    if (line.trim().length === 0) continue;
    if (line.match(/^\s*/)[0].length <= pathIndent) break;
    patterns.push(line.trim());
  }
  assert.ok(patterns.length > 0, `Empty upload paths for ${artifactName}`);
  return patterns;
}

function matchesUploadPattern(path, pattern) {
  const escape = (part) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const expression = pattern.split('*').map(escape).join('[^/]*');
  return new RegExp(`^${expression}$`).test(path);
}

for (const installer of [
  { bundle: 'nsis', filename: 'AriaType_1.1.2_x64-setup.exe' },
  { bundle: 'msi', filename: 'AriaType_1.1.2_x64_en-US.msi' },
]) {
  test(`release upload preserves the signed ${installer.bundle} installer for required updater platforms`, () => {
    const root = mkdtempSync(join(tmpdir(), 'ariatype-release-upload-'));
    try {
      const releaseDir = join(root, 'downloaded-artifacts');
      mkdirSync(releaseDir);
      const macBundle = 'apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos';
      const windowsBundle = `apps/desktop/src-tauri/target/release/bundle/${installer.bundle}`;
      // Fixture signatures prove artifact discovery, not cryptographic validation.
      const artifacts = [
        { artifactName: 'macos-release-assets', path: `${macBundle}/AriaType.app.tar.gz`, content: 'mac updater' },
        { artifactName: 'macos-release-assets', path: `${macBundle}/AriaType.app.tar.gz.sig`, content: 'mac-signature' },
        { artifactName: 'windows-release-assets', path: `${windowsBundle}/${installer.filename}`, content: 'windows installer' },
        { artifactName: 'windows-release-assets', path: `${windowsBundle}/${installer.filename}.sig`, content: 'windows-signature' },
      ];

      for (const artifact of artifacts) {
        const source = join(root, artifact.path);
        mkdirSync(dirname(source), { recursive: true });
        writeFileSync(source, artifact.content);
        const patterns = uploadPatternsFor(artifact.artifactName);
        if (patterns.some((pattern) => matchesUploadPattern(artifact.path, pattern))) {
          copyFileSync(source, join(releaseDir, basename(artifact.path)));
        }
      }

      const result = spawnSync(process.execPath, [
        manifestGenerator,
        '--release-dir', releaseDir,
        '--version', '1.1.2',
        '--base-url', 'https://github.com/joe223/AriaType/releases/download/v1.1.2',
        '--require-updater',
        '--require-updater-platform', 'darwin-aarch64',
        '--require-updater-platform', 'darwin-x86_64',
        '--require-updater-platform', 'windows-x86_64',
      ], { encoding: 'utf8' });

      assert.equal(result.status, 0, result.stderr);
      const manifest = JSON.parse(readFileSync(join(releaseDir, 'latest.updater.json'), 'utf8'));
      assert.deepEqual(Object.keys(manifest.platforms), [
        'darwin-aarch64', 'darwin-x86_64', 'windows-x86_64',
      ]);
      assert.equal(manifest.platforms['windows-x86_64'].signature, 'windows-signature');
      assert.equal(
        manifest.platforms['windows-x86_64'].url,
        `https://github.com/joe223/AriaType/releases/download/v1.1.2/${installer.filename}`,
      );
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}

test('release workflow pins a llama.cpp runtime release', () => {
  assert.match(releaseWorkflow, /LLAMA_CPP_RELEASE_TAG:\s*b\d+/);
});

test('release workflow prepares macOS and Windows local polish runtimes', () => {
  assert.match(releaseWorkflow, /prepare-llama-server-release-assets\.mjs[\s\S]*--platform macos/);
  assert.match(releaseWorkflow, /prepare-llama-server-release-assets\.mjs[\s\S]*--platform windows/);
  assert.match(releaseWorkflow, /\*macos\*arm64\*/);
  assert.match(releaseWorkflow, /\*macos\*x64\*/);
  assert.match(releaseWorkflow, /\*win\*cpu\*x64\*/);
});

test('release workflow requires bundled local polish runtime during packaging', () => {
  const requiredGateCount = releaseWorkflow.match(/ARIATYPE_REQUIRE_LOCAL_POLISH_RUNTIME:\s*"1"/g)
    ?.length ?? 0;

  assert.equal(requiredGateCount, 2);
});

test('release workflow verifies bundled runtime resources before upload', () => {
  assert.match(
    releaseWorkflow,
    /verify-tauri-runtime-resources\.mjs --platform macos --smoke --smoke-timeout-ms 30000/
  );
  assert.match(
    releaseWorkflow,
    /verify-tauri-runtime-resources\.mjs --platform windows --smoke --smoke-timeout-ms 30000/
  );
});
