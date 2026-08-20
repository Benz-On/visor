# Local model advisor methodology

VISOR's advisor is a capacity planner and decode-throughput estimator. It never
labels a prediction as a measured benchmark.

## Capacity model

For dedicated GPUs, the physical pool is:

`system RAM + dedicated VRAM`

VISOR subtracts an explicit operating-system reserve from RAM and a display /
driver reserve from VRAM. On Apple Silicon and recognized integrated GPUs,
memory is unified and is counted once rather than being duplicated as RAM and
VRAM.

The per-model requirement is:

`model weights + KV cache at selected context + runtime overhead`

Actual file size is preferred over an estimate from parameter count and
quantization. A live runtime allocation can raise the requirement when it is
larger than the static estimate. The context planner offers 2K through 32K
scenarios because KV memory and decode speed change with context length.

## Generation speed model

Autoregressive decode commonly streams a substantial part of the active model
weights for every generated token. VISOR therefore starts from effective memory
bandwidth divided by active bytes read per token, then applies conservative
runtime, quantization, context, split-offload, and uncertainty factors.

- Full GPU offload uses a device bandwidth profile when the adapter is known.
- Hybrid inference adds the GPU and CPU layer times rather than averaging their
  speeds; this prevents fast VRAM from hiding slow system RAM.
- CPU inference derives bandwidth from detected DDR data rate and populated
  channels where available.
- MoE names such as `30B-A3B` use all 30B parameters for capacity but the active
  parameter path for per-token weight traffic.
- A model beyond usable RAM + VRAM still receives a conditional mmap/pagefile
  estimate. The UI marks storage paging as the bottleneck and lowers confidence.

The result is a range plus a modeled center, confidence score, GPU-offload share,
effective bandwidth, memory deficit, and primary bottleneck. It is deliberately
not a single false-precision number.

## Evidence and limits

The method follows llama.cpp's own performance guidance and benchmark tooling,
where text generation is strongly affected by memory bandwidth, physical CPU
core selection, context, and offload configuration:

- [llama.cpp token-generation performance tips](https://github.com/ggml-org/llama.cpp/blob/master/docs/development/token_generation_performance_tips.md)
- [llama.cpp benchmark tool](https://github.com/ggml-org/llama.cpp/tree/master/tools/llama-bench)
- [NVIDIA Blackwell architecture specifications](https://images.nvidia.com/aem-dam/Solutions/geforce/blackwell/nvidia-rtx-blackwell-gpu-architecture.pdf)
- [NVIDIA Ada architecture specifications](https://images.nvidia.com/aem-dam/Solutions/Data-Center/l4/nvidia-ada-gpu-architecture-whitepaper-V2.02.pdf)

Driver version, backend, thermal state, memory timings, prompt length, batch
size, flash attention, speculative decoding, model architecture, and runtime
flags can move real results outside the predicted range. A future opt-in local
benchmark can replace the prediction with a measured profile without sending a
prompt off-device.
