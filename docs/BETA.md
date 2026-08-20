# VISOR 0.8 beta

VISOR 0.8.0-beta.1 develops the hardware-intelligence release into a transparent
local-inference planner. It combines RAM and VRAM, models KV context and memory
bandwidth, handles MoE active parameters, and keeps a conditional speed estimate
visible even when a model exceeds fast memory.

## Delivery matrix

| Target | Status | Download |
| --- | --- | --- |
| Windows 10/11 x64 | Beta | Portable `.exe`, NSIS setup |
| Windows 11 ARM64 | Preview | Portable `.exe`, NSIS setup |
| macOS Intel | Preview | Portable `.app.zip`, DMG |
| macOS Apple Silicon | Preview | Portable `.app.zip`, DMG |
| Linux x64 | Preview | AppImage, Debian package |
| Linux ARM64 | Preview | AppImage, Debian package |

Windows assets are built, smoke-tested, checksummed, and published manually
while hosted-runner quota is unavailable. Cross-platform source remains public;
macOS, Linux, and ARM64 binary previews resume when native builders are available.

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
- Model tok/s ranges now expose context, memory equation, offload, bottleneck,
  confidence, and paging assumptions, but remain predictions until a local
  benchmark runs.
- Exact CPU package, fan, voltage, and motherboard readings depend on firmware
  exposure or a running LibreHardwareMonitor/OpenHardwareMonitor WMI provider.

## Release acceptance

1. Frontend lint, production build, and Node tests pass.
2. Rust formatting and tests pass on the release host; cross-platform tests run
   again when hosted-runner quota is restored.
3. Each published platform produces both expected portable and package artifacts.
4. Checksums match the generated release manifest.
5. A portable build is smoke-tested on each supported operating-system family.
6. The interface is checked for unavailable-sensor honesty and console errors.
