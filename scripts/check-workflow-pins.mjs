import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';

const workflows = resolve('.github/workflows');
const files = (await readdir(workflows))
  .filter((name) => /\.ya?ml$/i.test(name))
  .sort();
const violations = [];

for (const file of files) {
  const source = await readFile(resolve(workflows, file), 'utf8');
  source.split(/\r?\n/).forEach((line, index) => {
    const reference = line.match(/^\s*uses:\s*([^\s#]+)/)?.[1];
    if (!reference || reference.startsWith('./') || reference.startsWith('docker://')) return;
    if (!/^[^/@\s]+\/[^/@\s]+@[0-9a-f]{40}$/i.test(reference)) {
      violations.push(`${file}:${index + 1} uses ${reference}`);
    }
  });
}

if (violations.length > 0) {
  throw new Error(`GitHub Actions must use immutable full commit SHAs:\n${violations.join('\n')}`);
}

console.log(`Verified immutable action pins in ${files.length} workflows.`);
