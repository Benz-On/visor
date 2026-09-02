# Changelog

All notable VISOR changes are documented here. Versions follow Semantic
Versioning while the product remains in beta.

## [0.9.2-beta.1] - Unreleased

### Added

- Premium design-system surfaces for the measured-AI features: a
  mint-accented measured-rate strip with per-metric tone gradients and
  tabular numerals; a layer offload plan rendered as interactive
  placement cards with FITS/OVER verdict chips, hover lift, and tone
  theming; a session throughput history card with average/peak/tokens
  stat row and legend markers.
- "GGUF exact" provenance tag in the model spec grid, styled as a
  system chip rather than ad-hoc text.
- Command palette keyboard-selection highlighting (inset accent bar)
  matching the hover state.
- Theme parity for Cyberdeck and Retro terminal across all new
  surfaces; custom thin scrollbars and cyan text selection globally.

### Changed

- All ad-hoc inline styles in the AI view were replaced with design
  system classes; every color now flows through theme tokens so all
  four themes render the new panels correctly.

## [0.9.1-beta.1] - Unreleased

### Added

- Session throughput history: every measured completion (runtime log or
  metrics endpoint) is recorded with decode/prefill rates and token count,
  deduplicated so re-read log lines never double-count, and exposed via a
  `get_throughput_history` command. The AI view renders the session curve
  with average, peak, and total tokens — explicitly labeled as
  in-memory for this collector session.
- Layer offload plan in the inference planner: with an exact GGUF layer
  count and weight bytes, four placements (100/75/50/25% of layers on
  GPU) each report predicted tok/s and VRAM need, answering "how many
  layers fit" with copy-ready `num_gpu` guidance.

## [0.9.0-beta.1] - Unreleased

### Added

- Measured inference throughput: decode and prefill tok/s read from the
  Ollama server log (`slot print_timing` lines) and from llama.cpp/vLLM
  prometheus `/metrics` counters, with explicit evidence labels
  (`runtime-log`, `metrics-endpoint`, `unavailable`). Rates are never
  modeled; when no generation is observed the panel reports absence.
- Exact GGUF header reader: parameter count, block count, attention and
  KV head counts (per-layer arrays included), embedding and context
  lengths, expert counts for MoE, and exact weight bytes summed from the
  tensor table. Every allocation is capped so a corrupt file aborts the
  probe instead of the collector.
- Installed Ollama models are probed through their manifests to the
  primary GGUF blob, and the model advisor now prefers exact header
  metadata (true parameter count, GQA-aware KV math, MoE active ratio,
  weight bytes) over filename heuristics, raising advisor confidence.
- Multi-GPU visibility: nvidia-smi enumerates every adapter; the primary
  GPU is the one with active memory, siblings appear in the sensor
  console and in a new `gpus` snapshot block.
- Live process re-validation: kill and priority commands refresh the
  process table at action time and re-run the protected-process check,
  closing the PID-recycling window between the UI snapshot and the OS
  call.
- Collector diagnostics: `agent.lastError` now reports real probe
  failures (for example an nvidia-smi timeout) instead of staying null.

### Changed

- Command palette is functional: query filtering, keyboard navigation
  (up/down/Enter), and eight navigation targets.
- History view honestly labels its rolling buffer (~37 s at 1.1 s
  samples) instead of showing inert 1 hour/24 hours/7 days switches.
- The AI view shows a measured-rate panel with decode/prefill split when
  a runtime reports timings, and keeps the modeled estimate clearly
  separate from it.

## [0.8.0-beta.1] - Unreleased

### Added

- Interactive 2K–32K context planner with a transparent weights + KV cache +
  runtime memory equation, combined RAM/VRAM budget, reserves, deficit, and GPU
  offload visualization.
- Bandwidth-oriented generation estimator with device profiles, detected DDR
  topology, MoE active-parameter handling, context penalties, and an explicit
  confidence score and bottleneck.
- Conditional tok/s forecasts for models beyond usable RAM + VRAM instead of
  hiding speed behind an unavailable verdict.

### Changed

- Unified-memory systems are counted once; dedicated systems now use RAM + VRAM
  directly for capacity planning while keeping OS and display reserves visible.
- Hybrid speed is calculated from serial GPU/CPU layer time, so slow system RAM
  can no longer be masked by a fast GPU coefficient.

## [0.7.0-beta.1] - 2026-08-01

### Added

- Dedicated Hardware Intelligence view with motherboard, BIOS, CPU socket and
  cache, GPU adapters, DIMM topology, storage health, network, and display inventory.
- HWiNFO-style sensor console covering temperature, load, clock, fan, voltage,
  power, and throughput values with source and evidence labels.
- Windows memory-manager snapshot with cache, commit, kernel pools, paging
  activity, and an explicit RAM/process refresh control.
- Per-process working set, private and virtual memory, handles, threads, disk
  throughput, uptime, responsiveness, executable path, priority, and task control.
- Detection for Cursor, Windsurf, Amazon Q Developer, Perplexity, Continue,
  Cline, and Roo Code plus a 23-provider cloud API signal catalog.

### Changed

- Native collector failures no longer fall back to fabricated demo values.
- Temperature values retain one-decimal precision and explicit hardware,
  graphics-driver, firmware-zone, OS-counter, or modeled provenance.
- Manual refresh now invalidates the native process, RAM, hardware, thermal,
  GPU, and AI caches for a complete verified resample.
- Process and critical hardware values use a denser, more legible premium hierarchy.

## [0.6.0-beta.2] - 2026-07-27

### Added

- Offline Ollama manifest discovery plus loopback adapters for GPT4All, vLLM,
  Text generation web UI, and LocalAI-compatible runtimes.
- Process attribution for Gemini CLI, OpenCode, Aider, Open WebUI, Msty, Tabby,
  llamafile, MLX LM, exo, and TensorRT-LLM.
- Documented evidence levels and explicit privacy boundaries for local runtimes,
  cloud clients, and paid provider APIs.
- Critical-process protection for macOS and Linux process controls.

### Changed

- Pinned every GitHub Action to an immutable full commit SHA and gated CI and
  release fan-out behind a single workflow-integrity preflight.
- Clarified that cloud-client telemetry is the local process footprint only;
  account status, tokens, API costs, prompts, and encrypted traffic are not read.
- Improved important-metric typography consistency and cross-platform wording.
- Updated the lint toolchain and removed all reported npm dependency advisories.

## [0.6.0-beta.1] - 2026-07-27

### Added

- Native Windows x64 and ARM64, macOS Intel and Apple Silicon, and Linux x64
  and ARM64 release matrix.
- GitHub Release publication with portable downloads, native packages,
  SHA-256 checksums, and a machine-readable manifest.
- Native Unix process termination, process-tree handling, and priority control.
- Cross-platform local-model folder discovery and NVIDIA Linux telemetry.
- Public contribution, conduct, privacy, security, support, platform, and
  release documentation.
- Dependabot configuration, issue forms, pull-request template, release gates,
  and Windows portable smoke testing.

### Changed

- Replaced Windows-only interface language with capability-based platform copy.
- Split continuous integration from release packaging and reduced workflow
  permissions to the minimum required per job.
- Marked macOS, Linux, and ARM64 packages as previews until native sensor
  coverage and production signing are complete.

## [0.5.0-beta.1] - 2026-07-16

### Added

- Local AI model inventory, hardware-fit advisor, and live service attribution.
- Premium Geist typography, four themes, thermal emphasis, and sustained-load
  alerts with gaming-aware suppression.

[0.8.0-beta.1]: https://github.com/Benz-On/visor/compare/v0.7.0-beta.1...main
[0.7.0-beta.1]: https://github.com/Benz-On/visor/compare/v0.6.0-beta.2...v0.7.0-beta.1
[0.6.0-beta.2]: https://github.com/Benz-On/visor/compare/v0.6.0-beta.1...v0.6.0-beta.2
[0.6.0-beta.1]: https://github.com/Benz-On/visor/compare/v0.5.0-beta.1...v0.6.0-beta.1
[0.5.0-beta.1]: https://github.com/Benz-On/visor/releases/tag/v0.5.0-beta.1
