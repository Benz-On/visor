# Public repository checklist

The source and automation are prepared for public visibility. Complete these
repository-owner decisions before changing visibility.

## Blocking decision

- [x] Select and add a source license. Apache-2.0 was chosen for its explicit
      patent grant and its match with Tauri's own dual licensing. The license
      text is in `LICENSE`; README, CONTRIBUTING, and the inbound=outbound
      contributor grant are aligned with it.

VISOR may now be described as open source under Apache-2.0.

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

- [x] Restore an automatic CI gate on a quota-cheap runner: pushes to `main` and
      pull requests run the preflight, web build, agent tests, and native
      formatting/Clippy/tests on Linux only.
- [ ] Restore the automatic Windows and macOS native legs when Actions quota
      allows; they currently run only on manual dispatch.
- [ ] Build each published target manually on its native host and verify every
      platform file plus checksums and manifest.
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
