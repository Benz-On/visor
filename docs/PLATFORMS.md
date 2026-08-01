# Platform support

VISOR uses one Tauri interface and one Rust collector across desktop platforms.
The collector exposes missing sensors explicitly instead of simulating them.

## Windows 10/11

- x64: primary beta platform
- ARM64: preview build on native GitHub-hosted runners
- Portable executable and per-user NSIS installer
- NVIDIA telemetry through `nvidia-smi`
- Per-process GPU engine and dedicated-memory counters through Windows APIs
- SMBIOS/WMI inventory for system, motherboard, BIOS, CPU, DIMMs, GPU adapters,
  physical storage, network adapters, and connected displays
- Windows memory-manager counters for cache, commit, paging, and kernel pools
- Optional temperature, fan, voltage, clock, power, and load sensors through
  LibreHardwareMonitor/OpenHardwareMonitor WMI providers
- Process tree termination and Windows priority classes

The portable executable requires the system WebView2 runtime. The NSIS installer
contains the WebView2 bootstrapper.

## macOS

- Intel x64 and Apple Silicon ARM64 preview builds
- Portable zipped `.app` and DMG image
- CPU, RAM, process, disk, network, and local AI discovery through the native collector
- POSIX process termination and `renice` priority control
- GPU, VRAM, and thermal coverage is limited until native Metal / IOKit probes are added

Prerelease builds use ad-hoc signing and are not notarized. Production distribution
requires an Apple Developer ID certificate and notarization secrets.

## Linux

- x64 and ARM64 preview builds
- Portable AppImage and Debian package
- CPU, RAM, process, disk, network, and local AI discovery through the native collector
- NVIDIA telemetry when `nvidia-smi` is installed
- POSIX process termination and `renice` priority control
- Desktop integration depends on WebKitGTK 4.1 and the distribution environment

The AppImage is the recommended portable download. The `.deb` is intended for
Debian, Ubuntu, and compatible distributions.

## ARM64 policy

ARM64 packages are built on native GitHub-hosted runners rather than cross-compiled
for release. This ensures the Rust target, frontend runtime, and Tauri bundler all
execute on the architecture they package.

## Capability roadmap

1. Native Apple GPU, power, and thermal telemetry.
2. Linux AMD/Intel GPU adapters and per-process GPU attribution.
3. Production code signing and macOS notarization.
4. Measured local-model benchmark profiles by backend and quantization.
