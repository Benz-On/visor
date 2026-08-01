import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

const node = process.execPath;

test('release version gate accepts the synchronized prerelease tag', () => {
  const output = execFileSync(node, ['scripts/check-release-version.mjs', 'v0.7.0-beta.1'], {
    encoding: 'utf8',
  });
  assert.match(output, /release metadata is consistent/);
});

test('GitHub Actions use immutable commit pins', () => {
  const output = execFileSync(node, ['scripts/check-workflow-pins.mjs'], { encoding: 'utf8' });
  assert.match(output, /Verified immutable action pins/);
});

test('release version gate rejects a tag that does not match the manifests', () => {
  const result = spawnSync(node, ['scripts/check-release-version.mjs', 'v9.9.9'], {
    encoding: 'utf8',
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Release version mismatch/);
});

test('release manifest records asset bytes and SHA-256 checksums', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'visor-release-'));
  const assetName = 'VISOR-test-portable.bin';
  const asset = Buffer.from('VISOR release fixture\n');
  await writeFile(join(directory, assetName), asset);

  execFileSync(node, ['scripts/create-release-manifest.mjs', directory]);

  const manifest = JSON.parse(await readFile(join(directory, 'release-manifest.json'), 'utf8'));
  const checksums = await readFile(join(directory, 'SHA256SUMS.txt'), 'utf8');
  const digest = createHash('sha256').update(asset).digest('hex');

  assert.equal(manifest.version, '0.7.0-beta.1');
  assert.equal(manifest.channel, 'prerelease');
  assert.deepEqual(manifest.files, [{ name: assetName, bytes: asset.byteLength, sha256: digest }]);
  assert.equal(checksums, `${digest}  ${assetName}\n`);
});
