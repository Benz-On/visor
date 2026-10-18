# AI detection coverage

VISOR separates four different kinds of evidence. A process match is never
presented as proof of a loaded model, a paid plan, token usage, or provider cost.

## Evidence levels

| Level | VISOR can show | Limits |
| --- | --- | --- |
| Runtime API | Exact model identity and reported allocation/context | Only while a supported loopback API is enabled and accessible without credentials |
| Offline inventory | Installed model identity, file size, format and inferred metadata | File names and manifests may omit architecture, quantization or context |
| Process attribution | Local CPU, GPU, RAM, VRAM and modeled watts for a recognized service | Shared browser/editor hosts cannot always be attributed to one extension |
| Provider billing | Not connected in this beta | Requires an explicit authenticated provider connector |

## Loopback adapters

VISOR probes only `127.0.0.1`, with short timeouts:

- Ollama (`11434`): running and installed models
- LM Studio (`1234`): library and loaded instances
- llama.cpp / compatible (`8080`): server properties and OpenAI-compatible models
- Jan (`1337`): OpenAI-compatible model list
- ComfyUI (`8188`): model files referenced by the active queue
- GPT4All API (`4891`): OpenAI-compatible model list
- vLLM / compatible (`8000`): OpenAI-compatible model list
- Text generation web UI / compatible (`5000`): OpenAI-compatible model list
- LocalAI / compatible (`8080`): model list when the endpoint is not identified as llama.cpp

Ports can be changed by those applications. A non-default, authenticated, IPv6,
container-only, WSL-only, or remote endpoint may therefore remain unavailable.

## Offline model inventory

VISOR reads bounded, known local model roots and custom roots supplied through
`VISOR_MODEL_PATHS`. It recognizes:

- offline Ollama manifests, including namespace, tag and aggregate layer size;
- LM Studio, GPT4All, Jan, Hugging Face and llama.cpp library roots;
- GGUF and GGML files;
- Safetensors, ONNX and sharded PyTorch weight folders.

The scan is capped by entry count, model count, depth and a 60-second cache. It
does not upload filenames or recursively follow directory symlinks.

## Recognized local and hybrid processes

The curated process catalog includes Ollama, LM Studio, llama.cpp, llamafile,
GPT4All, Jan, LocalAI, vLLM, Text generation web UI, KoboldCpp, ComfyUI,
AnythingLLM, Open WebUI, Msty, Tabby, MLX LM, exo, TensorRT-LLM, InvokeAI,
Fooocus, Stable Diffusion and identifiable Python AI runners.

This is broad coverage, not a claim to detect every possible runtime. Containers,
renamed executables, generic Python/Node hosts and custom launchers may require a
future signed adapter or user-supplied detection rule.

## Cloud clients and paid APIs

VISOR recognizes identifiable local processes for Codex, ChatGPT, Claude,
Claude Code, Kimi Code, GitHub Copilot, Gemini CLI, Cursor, Windsurf, Amazon Q
Developer, Perplexity, Continue, Cline, Roo Code, OpenCode and Aider. For these
services it reports only resource consumption on the current device.

The provider checker reports local client processes and the **presence only**
of known environment variables for 23 providers, including OpenAI, Azure OpenAI,
Anthropic, Gemini, Vertex AI, Mistral, Groq, Cohere, Together, Fireworks,
OpenRouter, DeepSeek, xAI, Perplexity, Moonshot, GitHub Copilot, Amazon Bedrock,
Hugging Face, Replicate, Cerebras, SambaNova, Cloudflare Workers AI, and NVIDIA
NIM. VISOR never returns, logs, or persists the variable values.

VISOR does **not** currently determine whether the account is free or paid, read
subscription limits, intercept HTTPS traffic, inspect prompts, count remote
tokens, or calculate the provider invoice. A browser tab alone is deliberately
not classified because doing so would attribute the entire browser to one site.

Future billing connectors should be opt-in, read-only, disabled by default and
store credentials in the operating-system keychain. Provider capabilities are
not uniform: OpenAI, Anthropic and Mistral expose organization/admin usage APIs;
OpenRouter exposes authenticated credit and usage data; Gemini usage is surfaced
through AI Studio and Google Cloud billing. Personal subscriptions do not always
offer a programmatic usage API.

No connector should reuse an inference key when a narrower read-only management
credential is available.
