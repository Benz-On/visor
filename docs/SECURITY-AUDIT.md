# VISOR 0.7.0-beta.1 security audit

Audit date: 2026-08-01

## Results

- `npm audit --audit-level=high`: **0 vulnerabilities** across production and
  development dependencies.
- RustSec `cargo audit`: **0 known vulnerabilities** in the unchanged locked
  Rust dependency graph at the previous 2026-07-27 audit. The helper is not
  installed on the current manual build host.
- Repository history scan: no common private-key, GitHub token, OpenAI-style
  token or AWS access-key pattern found.
- Tracked-file scan: no `.env`, private key, certificate or signing bundle found.
- GitHub Actions are pinned to full immutable commit SHAs and run with minimal
  repository permissions.
- Hardware inventory intentionally omits motherboard/disk serials, MAC addresses,
  IP addresses, and Windows product identifiers from the application snapshot.
- AI provider detection tests environment-variable presence only. Credential
  values are never copied into Rust/JavaScript snapshots, logs, or the UI.
- Windows process, memory, hardware, and sensor PowerShell probes are embedded,
  non-interactive, time-bounded, and executed locally with no remote input.

## Residual dependency warnings

RustSec reports 19 allowed warnings in transitive dependencies. Most are
unmaintained GTK3 bindings inherited by Tauri's Linux WebKit stack. Two are
soundness advisories:

- `RUSTSEC-2024-0429` affects a specific `glib::VariantStrIter` iterator API.
  VISOR does not call that API directly; the crate is present in the Linux GUI
  dependency graph. Linux remains a preview target while the Tauri/Wry stack is
  monitored for a supported migration.
- `RUSTSEC-2026-0097` affects `rand 0.7` with a custom logger. VISOR inherits it
  through the Tauri HTML/CSS parsing build toolchain and does not call it from
  application runtime code.

These warnings are not silently ignored: the pinned CI workflow includes
`cargo audit` when hosted runners are available, and Dependabot tracks npm,
Cargo and GitHub Actions updates. A new RustSec vulnerability causes that audit
command to fail.

## Release boundary

The beta is suitable for public testing, not for a stable trust claim. Windows
and macOS packages remain unsigned, macOS is not notarized, and the repository
owner must select a source license before describing VISOR as open source.
