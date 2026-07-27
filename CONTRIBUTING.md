# Contributing to VISOR

Thank you for helping improve VISOR.

## Before starting

- Search existing issues and pull requests.
- Use a feature request for product proposals and a bug report for reproducible defects.
- Discuss large architecture or platform changes before implementation.
- Never include real credentials, prompts, private model paths, or personal process dumps.

## Development

Install Node.js 20.19+, Rust stable, and the current platform prerequisites for Tauri.

```bash
npm install
npm run desktop
```

Run before opening a pull request:

```bash
npm run lint
npm run build
npm run test:agent
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

## Pull requests

- Keep changes focused and explain user-visible behavior.
- Add tests for collectors, parsers, security rules, and regressions.
- Preserve explicit `Unavailable` states when a sensor is absent.
- Keep runtime probes local-only and time-bounded.
- Use direct command arguments; do not introduce shell interpolation.
- Document platform-specific behavior in `docs/PLATFORMS.md`.
- Include screenshots for material interface changes.

Contributors must have the right to submit their work. The project source license
is still awaiting an owner decision; pull-request licensing terms will be finalized
before the repository is presented as open source.
