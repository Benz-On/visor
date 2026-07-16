# VISOR

Current version: **0.4.0-beta.1**

VISOR is a premium, local-first Windows system monitor for demanding
workstations, AI workloads, games, and creative tools. It combines a calm,
high-density interface with a local Windows agent that reads the machine and
performs explicitly confirmed process actions.

## What is working now

- Real-time CPU, GPU, RAM, VRAM, disk, network, thermal, and hardware telemetry
- NVIDIA power, temperature, clocks, utilization, and VRAM through vendor telemetry
- Full process explorer with live CPU, GPU, memory, VRAM, handles, and threads
- Confirmed process termination plus efficiency and high-priority controls
- Protected Windows critical-process denylist and PID confirmation on destructive actions
- Energy Lens with live whole-PC watts, component model, session energy, cost,
  carbon projection, confidence score, and per-process energy fingerprints
- Exact local AI attribution from application to runtime to loaded model, with
  adapters for Ollama, LM Studio, llama.cpp, and ComfyUI
- Model evidence including source, confidence, quantization, parameter count,
  context capacity, RAM/VRAM allocation, consumer PID, and application power
- Stateful sustained-load and thermal alert policies running inside the agent
- Game-aware CPU/GPU load suppression while thermal and VRAM protection stay active
- Four persistent, legible themes: Studio, Porcelain, Cyberdeck, and Retro Terminal
- Dedicated overview, performance, AI workload, history, alerts, and settings views
- 1.1 second live refresh, pause/resume, and command palette
- Responsive desktop, compact, and mobile layouts
- Local-only API bound to `127.0.0.1`; no telemetry leaves the device
- Native Tauri desktop runtime with an embedded Rust collector and no Node.js
  requirement for the compiled application

When the local agent is unavailable, VISOR makes the fallback demo state
explicit instead of presenting simulated values as real telemetry.

## Run locally

Requirements: Windows and Node.js 20 or newer.

Install once:

```powershell
npm.cmd install
```

Start the local Windows agent in an **Administrator PowerShell** when you want
to control processes owned by other users or by elevated applications:

```powershell
npm.cmd run agent
```

In a second PowerShell window, start the interface:

```powershell
npm.cmd run dev
```

Open [http://localhost:1420](http://localhost:1420).

## Run the native desktop beta

Requirements: Windows 10/11, Rust stable with the MSVC target, Visual Studio
2022 Build Tools with the C++ workload, Node.js 20 or newer, and WebView2.

```powershell
npm.cmd install
npm.cmd run desktop
```

The desktop runtime invokes the embedded Rust collector directly. The legacy
loopback agent remains available for browser development, but it is not needed
by the compiled desktop executable.

Build a native executable without an installer:

```powershell
npm.cmd run desktop:build:debug
```

The executable is written to `src-tauri/target/debug/visor.exe`.

## Build Windows release artifacts

The audited Windows pipeline produces both supported delivery modes:

```powershell
npm.cmd run release:windows
```

Outputs are written to the ignored `artifacts/` directory:

- `VISOR-<version>-windows-x64-portable.exe`
- `VISOR-<version>-windows-x64-setup.exe`
- `release-manifest.json`
- `SHA256SUMS.txt`

The installer carries the Microsoft WebView2 bootstrapper. The portable build
uses the WebView2 runtime already distributed with supported Windows 10/11
systems. Alpha and beta artifacts are currently unsigned and Windows will show
the corresponding publisher warning until production code signing is enabled.

The current beta targets Windows 10/11 x64. See [docs/BETA.md](docs/BETA.md)
for the tested delivery matrix, known limits, and release acceptance criteria.

## Validate

```powershell
npm.cmd run lint
npm.cmd run build
npm.cmd run test:agent
cargo test --manifest-path src-tauri/Cargo.toml
```

The production web bundle is written to `dist/`.

## Local AI evidence

VISOR probes loopback-only runtime APIs and joins their model evidence with the
Windows process tree and GPU counters. An exact model name is only shown when a
runtime reports it. A process-only detection is labeled as such.

- Ollama: active models from `/api/ps`
- LM Studio: loaded instances from its local API
- llama.cpp: loaded model metadata from `/props`
- ComfyUI: model files referenced by the active queue

Runtime probes have short timeouts and never leave `127.0.0.1`.

## Thermal sensor coverage

NVIDIA temperature is read through vendor telemetry. CPU and storage
temperatures are read when Windows exposes Storage Reliability counters or when
LibreHardwareMonitor/OpenHardwareMonitor publishes temperature sensors through
its WMI namespace. If this PC exposes neither source, VISOR displays
`Unavailable`; it never fabricates a temperature.

## Smart alert policy

The Windows agent tracks condition duration rather than notifying on short
spikes. Defaults include sustained CPU/GPU load, CPU/GPU/SSD temperature, VRAM
pressure, resolution history, and progress toward each alert threshold. When an
active game process is detected, only sustained CPU/GPU load alerts are muted.
Temperature and VRAM policies remain armed.

## Energy methodology

VISOR does not claim false measurement precision. With a compatible NVIDIA GPU,
GPU watts are measured by vendor telemetry while CPU, memory, storage, platform,
and conversion losses are estimated from live utilization and detected hardware
limits. The interface labels this as a **hybrid estimate** and exposes its
confidence and methodology. Without a measured GPU power sensor, the entire
figure is labeled **estimated**.

Cost and carbon projections use configurable assumptions. Defaults are
`0.25 EUR/kWh` and `56 gCO2e/kWh`; override them before starting the agent with
`VISOR_TARIFF_EUR_KWH` and `VISOR_CARBON_G_KWH`.

## Security model

- The agent listens only on the IPv4 loopback interface.
- Mutations accept only known VISOR origins and require an action header.
- Process termination requires an exact PID confirmation in the request body.
- Critical Windows processes, PID 0-4, and the agent itself cannot be terminated.
- Command arguments are passed directly to Windows tools without shell interpolation.

## Native collector architecture

The Tauri 2 desktop build now embeds a Rust collector while keeping the browser
agent as a development fallback. Collection remains layered so VISOR stays
useful when a vendor-specific API is unavailable:

1. PDH and Windows performance counters for CPU, memory, disk, and network.
2. DXGI and GPU engine counters for per-process GPU attribution.
3. NVIDIA NVML first, followed by AMD and Intel adapters, for clocks, VRAM,
   temperature, fan, power, and engine details.
4. ETW sessions for high-fidelity process, disk, and network attribution.
5. Signed Windows notifications and durable alert/history persistence.

The collector must poll asynchronously, batch updates into one snapshot, retain
history in a bounded ring buffer, and expose its own CPU/memory overhead in the
VISOR diagnostics view. No telemetry leaves the device.

## Product principles

- A user should understand system health in five seconds.
- Every aggregate metric should lead to the processes responsible for it.
- Missing sensors are shown honestly, never estimated without a label.
- Alerts remain quiet until they are actionable.
- VISOR's own monitoring overhead is a first-class metric.
