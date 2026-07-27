# VISOR

[![CI](https://github.com/Benz-On/visor/actions/workflows/ci.yml/badge.svg)](https://github.com/Benz-On/visor/actions/workflows/ci.yml)
[![Desktop releases](https://github.com/Benz-On/visor/actions/workflows/release.yml/badge.svg)](https://github.com/Benz-On/visor/actions/workflows/release.yml)
[![Latest release](https://img.shields.io/github/v/release/Benz-On/visor?include_prereleases)](https://github.com/Benz-On/visor/releases)

Current version: **0.6.0-beta.2**

VISOR is a premium, local-first system monitor for Windows, macOS, and Linux.
It combines high-density live telemetry, process control, energy modeling, and
local AI workload intelligence in a calm desktop interface.

> VISOR is beta software. Windows x64 currently has the deepest sensor coverage.
> macOS, Linux, and ARM64 packages are public previews with honest capability
> labels when a hardware or operating-system sensor is unavailable.

## Download

Download published builds from the [GitHub Releases page](https://github.com/Benz-On/visor/releases).
Every release includes `SHA256SUMS.txt` and `release-manifest.json`.

| Platform | Portable | Installer/package |
| --- | --- | --- |
| Windows x64 / ARM64 | `VISOR-*-windows-*-portable.exe` | `VISOR-*-windows-*-setup.exe` |
| macOS Intel / Apple Silicon | `VISOR-*-macos-*-portable.app.zip` | `VISOR-*-macos-*.dmg` |
| Linux x64 / ARM64 | `VISOR-*-linux-*-portable.AppImage` | `VISOR-*-linux-*.deb` |

Windows and macOS prerelease artifacts are not yet production-signed. Windows
may show an unknown-publisher warning. On macOS, use Control-click → Open for
the first launch of an unnotarized preview. Linux AppImages may need:

```bash
chmod +x VISOR-*-portable.AppImage
```

See [platform support](docs/PLATFORMS.md) for detailed coverage and limitations.

## Features

- Live CPU, RAM, disk, network, process, and hardware telemetry
- GPU utilization, temperature, power, clocks, and VRAM when vendor telemetry is available
- Process explorer with termination and priority controls protected by PID confirmation
- Energy Lens with component estimates, session energy, cost, carbon, and confidence
- Sustained-load, thermal, and VRAM alerts with game-aware suppression
- Installed and loaded model inventory for Ollama, LM Studio, llama.cpp, Jan,
  ComfyUI, GGUF/GGML, Safetensors, ONNX, and PyTorch weight folders
- Hardware-fit advisor with reserved-memory modeling and conservative tok/s ranges
- Live local footprint for Codex, ChatGPT, Claude Code, Kimi Code, Copilot,
  Ollama, LM Studio, Jan, GPT4All, llama.cpp, and related runtimes
- Self-hosted Geist and Geist Mono typography with four persistent themes
- Local native Tauri IPC; no external telemetry or analytics

For cloud AI clients, VISOR reports only the CPU, GPU, RAM, VRAM, and modeled
power consumed by their local processes. It cannot infer paid status, remote
tokens, provider charges, or datacenter use. See [AI detection coverage](docs/AI-COVERAGE.md).

## Platform support

| Capability | Windows | macOS | Linux |
| --- | --- | --- | --- |
| CPU, RAM, processes, disk, network | Beta | Preview | Preview |
| Process termination | Yes | Yes | Yes |
| Process priority | Yes | Yes, permission-dependent | Yes, permission-dependent |
| NVIDIA system telemetry | Yes | Not applicable | Yes when `nvidia-smi` is available |
| Per-process GPU attribution | Windows counters | Limited | Limited |
| CPU / storage temperatures | WMI/provider-dependent | Limited | Limited |
| Local runtime API probes | Yes | Yes | Yes |
| Known local model folders | Yes | Yes | Yes |

Missing sensors are displayed as unavailable. VISOR never replaces absent native
telemetry with fabricated values.

## Run from source

Requirements:

- Node.js 20.19 or newer
- Rust stable
- Platform prerequisites from the [Tauri documentation](https://v2.tauri.app/start/prerequisites/)

```bash
npm install
npm run desktop
```

The compiled application embeds the Rust collector. The optional Node agent is
only a browser-development fallback and currently targets Windows:

```bash
npm run agent
npm run dev
```

Open <http://localhost:1420>.

## Build locally

Compile only the native executable for the current platform:

```bash
npm run desktop:build
```

Create platform bundles:

```bash
npm run bundle:windows
npm run bundle:macos
npm run bundle:linux
```

Only run the command matching the host operating system. Windows also retains
the audited local release command:

```powershell
npm.cmd run release:windows
npm.cmd run smoke:windows
```

## Validate

```bash
npm run lint
npm run build
npm run test:agent
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

GitHub CI repeats native tests on Windows, macOS, and Linux. Version tags matching
`v*` trigger six platform builds and publish their assets on one GitHub Release.
See the [changelog](CHANGELOG.md) and [release documentation](docs/RELEASING.md).

## Local AI evidence

VISOR probes loopback-only runtime APIs and joins model evidence with the local
process tree. An exact model name is shown only when a runtime reports it.

- Ollama: installed models from offline manifests and `/api/tags`, active models from `/api/ps`
- LM Studio: installed models and loaded instances from its local API
- llama.cpp: loaded model metadata from `/props`
- Jan: models from its local OpenAI-compatible API
- ComfyUI: model files referenced by the active queue
- GPT4All, vLLM, Text generation web UI and LocalAI-compatible loopback APIs
- Known roots: GGUF, GGML, Safetensors, ONNX, and PyTorch weights
- Custom roots: use the platform path separator in `VISOR_MODEL_PATHS`

The hardware advisor reserves operating-system and runtime memory before deciding
whether a model fits in VRAM, split VRAM/RAM, CPU RAM, or is too large. Tok/s is
an engineering range until a real benchmark is run. Embedding models are not
misrepresented as text-generation models.

## Energy methodology

VISOR does not claim false measurement precision. Compatible vendor telemetry is
used when available; CPU, memory, storage, platform, conversion loss, and process
shares remain labeled estimates. The interface exposes the method and confidence.

Default projections are `0.25 EUR/kWh` and `56 gCO2e/kWh`. The Windows development
agent accepts `VISOR_TARIFF_EUR_KWH` and `VISOR_CARBON_G_KWH` overrides.

## Privacy and security

- Hardware, process, and model data remains on the device.
- Native builds use Tauri IPC and do not start a telemetry web server.
- Runtime API probes are loopback-only and time-bounded.
- Process mutation requires explicit confirmation and blocks critical processes.
- Command arguments are passed directly to operating-system tools without shell interpolation.

Read [PRIVACY.md](PRIVACY.md), [SECURITY.md](SECURITY.md), and the
[beta security audit](docs/SECURITY-AUDIT.md). Please do not report security
vulnerabilities in public issues.

## Contributing

Issues and technical contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md)
and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) first.

## License status

An open-source license has not yet been selected. Public visibility alone does
not grant permission to copy, modify, or redistribute the source. A license must
be chosen before the repository is presented as open source; see the
[public-release checklist](docs/PUBLIC_RELEASE_CHECKLIST.md).

Third-party components retain their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
