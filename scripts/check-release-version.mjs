import { readFile } from 'node:fs/promises';

const releaseTag = String(process.argv[2] || '').trim();
if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(releaseTag)) {
  throw new Error(`Invalid or missing release tag: ${releaseTag || '(empty)'}`);
}

const packageManifest = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
const tauriManifest = JSON.parse(await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
const cargoManifest = await readFile(new URL('../src-tauri/Cargo.toml', import.meta.url), 'utf8');
const cargoVersion = cargoManifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const versions = new Map([
  ['tag', releaseTag.slice(1)],
  ['package.json', packageManifest.version],
  ['tauri.conf.json', tauriManifest.version],
  ['Cargo.toml', cargoVersion],
]);
const expected = packageManifest.version;
const mismatches = [...versions].filter(([, version]) => version !== expected);
if (mismatches.length > 0) {
  throw new Error(`Release version mismatch. Expected ${expected}; ${mismatches.map(([source, version]) => `${source}=${version}`).join(', ')}`);
}
console.log(`VISOR ${expected} release metadata is consistent.`);
