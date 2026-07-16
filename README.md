# VISOR

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
- Dedicated overview, performance, AI workload, history, alerts, and settings views
- 1.1 second live refresh, pause/resume, command palette, and light/dark themes
- Responsive desktop, compact, and mobile layouts
- Local-only API bound to `127.0.0.1`; no telemetry leaves the device

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

## Validate

```powershell
npm.cmd run lint
npm.cmd run build
npm.cmd run test:agent
```

The production web bundle is written to `dist/`.

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

## Native collector roadmap

The next milestone moves the agent into a signed Tauri 2 / Rust collector while
keeping the same product surfaces. Collection will remain layered so VISOR stays
useful when a vendor-specific API is unavailable:

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
