# VISOR 0.6 beta

VISOR 0.6.0-beta.1 introduces the public multi-platform release pipeline and
native preview packages for Windows, macOS, and Linux on x64 and ARM64.

## Delivery matrix

| Target | Status | Download |
| --- | --- | --- |
| Windows 10/11 x64 | Beta | Portable `.exe`, NSIS setup |
| Windows 11 ARM64 | Preview | Portable `.exe`, NSIS setup |
| macOS Intel | Preview | Portable `.app.zip`, DMG |
| macOS Apple Silicon | Preview | Portable `.app.zip`, DMG |
| Linux x64 | Preview | AppImage, Debian package |
| Linux ARM64 | Preview | AppImage, Debian package |

All assets are built by GitHub Actions on native hosted runners and published
to one GitHub Release with SHA-256 checksums and a machine-readable manifest.

## Beta guarantees

- Hardware and process telemetry remains local to the device.
- Native builds use Tauri IPC and do not start an HTTP telemetry server.
- Missing sensors are shown as unavailable; VISOR does not substitute demo values.
- Process termination requires exact PID confirmation and blocks critical processes.
- Runtime and hardware probes are time-bounded.
- CI runs frontend tests plus native Rust tests on Windows, macOS, and Linux.
- Release versions are rejected when package, Cargo, Tauri, and Git tag versions differ.

## Known limits before stable

- Windows and macOS binaries are not yet production-signed or notarized.
- GPU and thermal telemetry is deepest on Windows with NVIDIA hardware.
- macOS GPU, VRAM, energy, and thermal sensors currently report partial coverage.
- Linux per-process GPU attribution and non-NVIDIA adapters remain limited.
- Priority increases on macOS/Linux may require elevated permission.
- Whole-device and per-process energy remain labeled engineering estimates unless
  a compatible vendor power sensor reports measured values.
- Model tok/s values remain conservative predictions until a local benchmark runs.

## Release acceptance

1. Frontend lint, production build, and Node tests pass.
2. Rust formatting and tests pass on Windows, macOS, and Linux CI.
3. Each matrix runner produces both expected platform artifacts.
4. Checksums match the generated release manifest.
5. A portable build is smoke-tested on each supported operating-system family.
6. The interface is checked for unavailable-sensor honesty and console errors.
