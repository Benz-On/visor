import { createHash } from 'node:crypto';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import { basename, resolve } from 'node:path';

const artifactDirectory = resolve(process.argv[2] || 'release-assets');
const packageManifest = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
const ignored = new Set(['release-manifest.json', 'SHA256SUMS.txt']);
const names = (await readdir(artifactDirectory, { withFileTypes: true }))
  .filter((entry) => entry.isFile() && !ignored.has(entry.name))
  .map((entry) => entry.name)
  .sort();
if (names.length === 0) throw new Error(`No release assets found in ${artifactDirectory}`);

const files = [];
for (const name of names) {
  const bytes = await readFile(resolve(artifactDirectory, name));
  files.push({ name, bytes: bytes.byteLength, sha256: createHash('sha256').update(bytes).digest('hex') });
}
const manifest = {
  product: 'VISOR',
  version: packageManifest.version,
  channel: packageManifest.version.includes('-') ? 'prerelease' : 'stable',
  generatedAt: new Date().toISOString(),
  signed: false,
  files,
};
await writeFile(resolve(artifactDirectory, 'release-manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
await writeFile(resolve(artifactDirectory, 'SHA256SUMS.txt'), `${files.map((file) => `${file.sha256}  ${basename(file.name)}`).join('\n')}\n`, 'utf8');
console.log(`Created release manifest for ${files.length} assets.`);
