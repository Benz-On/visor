# Public repository checklist

The source and automation are prepared for public visibility. Complete these
repository-owner decisions before changing visibility.

## Blocking decision

- [ ] Select and add a source license. Apache-2.0 is the recommended permissive
      option for a systems project because it includes an explicit patent grant;
      MIT is shorter; a proprietary/source-available license is also possible.

Do not describe VISOR as open source until this decision is complete.

## GitHub repository settings

- [ ] Confirm that `benz.on.contact@gmail.com` may remain visible in Git history.
- [x] Change repository visibility to **Public**.
- [x] Keep Actions workflow permissions read-only by default; `release.yml` grants
      `contents: write` only to its publish job.
- [ ] Protect `main`: require pull requests, CI success, resolved conversations,
      and block force pushes/deletion.
- [ ] Enable private vulnerability reporting, Dependabot alerts, secret scanning,
      and push protection.
- [ ] Enable Discussions if community support is desired.
- [ ] Set repository description, topics, and website/release link.

Suggested topics: `system-monitor`, `tauri`, `rust`, `react`, `windows`, `macos`,
`linux`, `ollama`, `local-ai`, `hardware-monitoring`.

## Release readiness

- [ ] Run CI successfully on Windows, macOS, and Linux.
- [ ] Run `Publish desktop release` for the current tag and verify all twelve
      platform files plus checksums and manifest.
- [ ] Smoke-test one package per operating system and architecture family.
- [ ] Configure Windows signing and Apple notarization before declaring stable.
- [ ] Publish release notes that clearly label preview sensor limitations.

## Audit completed locally

- [x] No committed `.env`, private key, certificate, or common token signature found.
- [x] Generated artifacts, build output, and dependency folders are ignored.
- [x] Loopback probes and native IPC remain local-only.
- [x] Direct runtime dependency licenses were reviewed and documented.
- [x] Windows x64 portable and NSIS artifacts were built; the portable binary
      passed an eight-second startup smoke test.
- [x] Windows x64 beta artifacts, checksums, and manifest were manually uploaded
      to the matching GitHub prerelease without committing binaries to Git.
- [x] npm and RustSec audits report no known dependency vulnerabilities; residual
      transitive warnings are documented in `docs/SECURITY-AUDIT.md`.
