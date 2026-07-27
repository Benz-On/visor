# Releasing VISOR

GitHub Releases are currently published from verified local builds. The hosted
workflow is retained for future use but has no automatic trigger.

## Release procedure

1. Update the same version in `package.json`, `package-lock.json`,
   `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`.
2. Run the validation commands documented in the README.
3. On Windows, run `npm.cmd run release:windows` followed by
   `npm.cmd run smoke:windows` to validate the portable executable.
4. Run `node scripts/check-release-version.mjs vX.Y.Z`.
5. Commit the release changes and create an annotated `vX.Y.Z` tag.
6. Push the commit and tag. Pushing a tag does not trigger GitHub Actions.
7. Create the matching GitHub Release manually and upload only verified assets,
   `SHA256SUMS.txt`, and `release-manifest.json`.

Versions containing a hyphen, such as `0.6.0-beta.2`, must be published as
prereleases.

## Manual publication mode

Keep binaries out of Git history: publish packages as assets of the matching
GitHub Release.

For a Windows x64 beta:

1. Run `npm.cmd run release:windows` on Windows.
2. Run `npm.cmd run smoke:windows` against the generated portable executable.
3. Verify `artifacts/SHA256SUMS.txt` and `artifacts/release-manifest.json`.
4. Upload only the two current-version executables, the checksum file, and the
   manifest to the existing tag's prerelease.
5. Download the published assets again and compare their SHA-256 hashes before
   announcing the release.

Do not publish a stable release manually until the license, cross-platform
tests, Windows signing, and Apple notarization gates are complete.

## Signing

Unsigned beta packages are intentional while the repository is being prepared.
Before a stable release:

- Configure a Windows code-signing certificate in repository or environment secrets.
- Configure Apple Developer ID signing and notarization secrets.
- Optionally configure GPG signing for Linux AppImages.
- Protect the release environment so signing secrets require approval.

Never place signing certificates, passwords, API tokens, or private keys in Git.
