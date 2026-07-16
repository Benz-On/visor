# VISOR 0.4 beta

VISOR 0.4.0-beta.1 is the first end-to-end desktop beta. It is intended for
real Windows workstation testing, not only interface preview.

## Supported delivery

| Target | Status | Notes |
| --- | --- | --- |
| Windows 10/11 x64 portable | Beta | One executable; requires the system WebView2 runtime |
| Windows 10/11 x64 installer | Beta | Per-user NSIS install; WebView2 bootstrapper included |
| Windows ARM64 | Roadmap | Native collector and packaging validation required |
| Linux x64/ARM64 | Roadmap | Platform collectors and process controls required |
| macOS Intel/Apple silicon | Roadmap | Native sensor entitlements and signing required |

Windows x64 is deliberately the first supported beta target. A web shell can
run elsewhere, but VISOR does not claim cross-platform monitoring until each
native collector has equivalent coverage and has passed platform QA.

## Beta guarantees

- Hardware and process telemetry remains local to the PC.
- The desktop build uses native Tauri IPC and does not start an HTTP telemetry server.
- Missing sensors are shown as unavailable; VISOR does not substitute demo values.
- Process termination requires an exact PID confirmation and blocks critical Windows processes.
- External hardware probes are time-bounded so a stalled vendor tool cannot freeze telemetry forever.
- The release pipeline runs lint, agent tests, Rust tests, and the production web build.
- Portable and installer SHA-256 hashes are emitted in `artifacts/SHA256SUMS.txt`.

## Known limits before stable

- Beta executables are unsigned, so Windows may display an unknown publisher warning.
- The self-updater is disabled for pre-release artifacts.
- NVIDIA exposes the richest GPU telemetry. Other vendors may report partial GPU data.
- CPU and SSD temperature depend on Windows sensor availability or a compatible
  LibreHardwareMonitor WMI provider; unavailable readings stay clearly labeled.
- Whole-PC and per-process energy are engineering estimates unless a vendor power
  sensor reports a measured component value. Confidence and source are displayed.
- Some protected or elevated processes require VISOR itself to run elevated before
  Windows permits a control action.

## Release acceptance

A beta artifact is accepted only after:

1. Frontend lint and production build pass.
2. Windows agent and native Rust tests pass.
3. Portable executable starts, remains responsive, and reports the expected version.
4. Installer performs a silent per-user install, installed application smoke test,
   and silent uninstall on a clean validation environment.
5. Generated file hashes are recomputed and match the release manifest.
6. A visual pass covers overview, processes, performance, local AI, themes, alerts,
   settings, responsive layout, and browser console errors.
