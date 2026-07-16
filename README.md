# VISOR

VISOR is a premium, local-first Windows system monitor designed for demanding
workstations, AI workloads, games, and creative tools. This repository contains
the first product foundation: the complete responsive interface and a live
telemetry simulator isolated behind a React hook.

## What is working now

- Live overview for CPU, GPU, RAM, VRAM, disk, network, thermals, and power
- Resource attribution table with AI, game, and creative-workload badges
- Full process explorer with search and resource sorting
- Dedicated performance, AI workload, history, alerts, and settings views
- 900 ms live refresh, pause/resume, command palette, and light/dark themes
- Responsive desktop, compact, and mobile layouts
- Offline-safe interface with no remote fonts, images, or runtime assets

The values shown in this first milestone are realistic simulated telemetry. The
UI is intentionally decoupled from collection so the simulator can be replaced
by the native Windows collector without changing the product surfaces.

## Run locally

Requirements: Node.js 20 or newer.

```powershell
npm.cmd install
npm.cmd run dev
```

Open [http://localhost:1420](http://localhost:1420).

## Validate

```powershell
npm.cmd run lint
npm.cmd run build
```

The production web bundle is written to `dist/`.

## Native Windows roadmap

The next milestone adds a Tauri 2 / Rust collector and replaces the telemetry
hook with native commands. Collection should be layered so VISOR remains useful
when a vendor-specific API is unavailable:

1. PDH and Windows performance counters for CPU, memory, disk, and network.
2. DXGI and GPU engine counters for per-process GPU attribution.
3. NVIDIA NVML first, followed by AMD and Intel adapters, for clocks, VRAM,
   temperature, fan, power, and engine details.
4. ETW sessions for high-fidelity process, disk, and network attribution.
5. AI adapters for Ollama, LM Studio, llama.cpp, and common Stable Diffusion
   runtimes, with explicit confidence and source labels.

The collector must poll asynchronously, batch updates into one snapshot, retain
history in a bounded ring buffer, and expose its own CPU/memory overhead in the
VISOR diagnostics view. No telemetry leaves the device.

## Product principles

- A user should understand system health in five seconds.
- Every aggregate metric should lead to the processes responsible for it.
- Missing sensors are shown honestly, never estimated without a label.
- Alerts remain quiet until they are actionable.
- VISOR's own monitoring overhead is a first-class metric.
