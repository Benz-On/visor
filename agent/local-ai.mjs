import { readFile, readdir, stat } from 'node:fs/promises';
import { homedir } from 'node:os';
import { basename, delimiter, extname, join } from 'node:path';

const GiB = 1024 ** 3;
const INVENTORY_TTL_MS = 60_000;
const MAX_SCANNED_ENTRIES = 4_000;
const MAX_FILE_MODELS = 240;

const finite = (value, fallback = 0) => Number.isFinite(Number(value)) ? Number(value) : fallback;
const round = (value, digits = 1) => {
  const scale = 10 ** digits;
  return Math.round(finite(value) * scale) / scale;
};

let fileInventoryCache = { expiresAt: 0, models: [], roots: [] };

function identity(application, runtime, role, category, execution, provider) {
  return { application, runtime, role, category, execution, provider };
}

export function detectAiApplication(processData = {}) {
  const name = String(processData.name || '').toLowerCase();
  const path = String(processData.path || '').toLowerCase();
  const command = String(processData.command || '').toLowerCase();
  const haystack = `${name} ${command} ${path}`;

  // Cloud clients and coding agents are intentionally classified before their
  // generic host executable (ChatGPT.exe, node.exe, python.exe, WebView2, etc.).
  const nodeHost = /^(node|node\.exe|bun|bun\.exe|deno|deno\.exe)$/.test(name);
  if (/openai\.codex|[\\/]openai[\\/]codex[\\/]/.test(path)
      || /^(codex(?:\.exe)?|codex-code-mode-host(?:\.exe)?|codex-command-runner(?:\.exe)?)$/.test(name)
      || (nodeHost && /@openai[\\/]codex/.test(command))) {
    return identity('Codex', 'OpenAI cloud', /host|runner/.test(name) ? 'agent-helper' : 'coding-agent', 'coding-agent', 'cloud', 'OpenAI');
  }
  if (name === 'chatgpt' || name === 'chatgpt.exe' || /openai\.chatgpt/.test(path)) {
    return identity('ChatGPT', 'OpenAI cloud', /msedgewebview2/.test(name) ? 'ui-helper' : 'application', 'cloud-client', 'cloud', 'OpenAI');
  }
  const claudeBinary = name === 'claude' || name === 'claude.exe';
  const claudeCodeMarker = /@anthropic-ai[\\/]claude-code|claude-code|\.claude[\\/]/.test(`${path} ${command}`);
  if (claudeBinary || (nodeHost && claudeCodeMarker)) {
    const codingAgent = claudeCodeMarker;
    return identity(codingAgent ? 'Claude Code' : 'Claude', 'Anthropic cloud', codingAgent ? 'coding-agent' : 'application', codingAgent ? 'coding-agent' : 'cloud-client', 'cloud', 'Anthropic');
  }
  if (name === 'kimi' || name === 'kimi.exe' || (nodeHost && /@moonshot-ai[\\/]|kimi-code|kimi-cli/.test(`${path} ${command}`))) {
    return identity('Kimi Code', 'Moonshot cloud', 'coding-agent', 'coding-agent', 'cloud', 'Moonshot AI');
  }
  if (/^(copilot-agent|copilot-language-server)(?:\.exe)?$/.test(name)
      || /github[ ._-]?copilot/.test(path)
      || (nodeHost && /github[ ._-]?copilot|copilot-agent|copilot-language-server/.test(command))) {
    return identity('GitHub Copilot', 'GitHub cloud', 'agent-helper', 'coding-agent', 'cloud', 'GitHub');
  }
  if (/@google[\\/]gemini-cli|[\\/]\.gemini[\\/]/.test(path)
      || (nodeHost && /@google[\\/]gemini-cli|[\\/]\.gemini[\\/]/.test(command))
      || ['gemini', 'gemini.exe', 'gemini.cmd'].includes(name)) {
    return identity('Gemini CLI', 'Google cloud', 'coding-agent', 'coding-agent', 'cloud', 'Google');
  }
  if (['opencode', 'opencode.exe'].includes(name) || /[\\/]opencode[\\/]/.test(path)) {
    return identity('OpenCode', 'Configured AI provider', 'coding-agent', 'coding-agent', 'hybrid', 'OpenCode');
  }
  if (['aider', 'aider.exe'].includes(name) || (/python/.test(name) && /(?:^|\s)-m\s+aider(?:\s|$)|[\\/]aider(?:-chat)?[\\/]/.test(command))) {
    return identity('Aider', 'Configured AI provider', 'coding-agent', 'coding-agent', 'hybrid', 'Aider');
  }

  if (name.includes('msedgewebview2') && /ollama app\.exe/.test(haystack)) {
    return identity('Ollama Desktop', 'WebView UI', 'ui-helper', 'local-runtime', 'local', 'Ollama');
  }
  if (/ollama/.test(haystack)) {
    const role = /llama-server|ollama_llama|runner/.test(name) ? 'model-runner' : /\bserve\b/.test(haystack) ? 'runtime' : 'application';
    return identity('Ollama', 'Ollama engine', role, 'local-runtime', 'local', 'Ollama');
  }
  if (/comfyui|comfy-desktop|comfy desktop/.test(haystack)) {
    return identity('ComfyUI', 'PyTorch', /python/.test(name) ? 'model-runner' : 'application', 'creative-ai', 'local', 'Comfy Org');
  }
  if (/lm studio|lmstudio|llmster/.test(haystack)) {
    return identity('LM Studio', 'llama.cpp', /server|llama|llmster/.test(name) ? 'model-runner' : 'application', 'local-runtime', 'local', 'LM Studio');
  }
  if (/gpt4all/.test(haystack)) {
    return identity('GPT4All', 'llama.cpp', /server|llama/.test(name) ? 'model-runner' : 'application', 'local-runtime', 'local', 'Nomic AI');
  }
  if (/(^|[\\/ ])jan(?:\.exe)?\b|jan\.ai/.test(haystack)) {
    return identity('Jan', 'llama.cpp', /llama|server/.test(name) ? 'model-runner' : 'application', 'local-runtime', 'local', 'Jan');
  }
  if (/anythingllm|anything-llm/.test(name) || /[\\/]anything-?llm[\\/]/.test(path)) {
    return identity('AnythingLLM', 'Local orchestrator', 'application', 'local-runtime', 'hybrid', 'Mintplex Labs');
  }
  if (/open-webui|open_webui/.test(name) || /[\\/]open[ _-]?webui[\\/]/.test(path) || (/python/.test(name) && /(?:^|\s)-m\s+open_webui(?:\s|$)/.test(command))) {
    return identity('Open WebUI', 'Local AI interface', 'application', 'local-runtime', 'hybrid', 'Open WebUI');
  }
  if (/\bmsty(?:\.exe)?\b|[\\/]msty[\\/]/.test(haystack)) {
    return identity('Msty', 'Local AI interface', 'application', 'local-runtime', 'hybrid', 'Msty');
  }
  if (/\btabby(?:\.exe)?\b|tabbyml/.test(haystack)) {
    return identity('Tabby', 'Local code model server', 'model-runner', 'local-runtime', 'local', 'TabbyML');
  }
  if (/llamafile/.test(name) || /[\\/]llamafile[\\/]/.test(path)) {
    return identity('llamafile', 'llama.cpp', 'model-runner', 'local-runtime', 'local', 'Mozilla');
  }
  if (/mlx[_-]lm/.test(name) || /[\\/]mlx[_-]lm[\\/]/.test(path) || (/python/.test(name) && /mlx_lm(?:\.server)?/.test(command))) {
    return identity('MLX LM', 'Apple MLX', 'model-runner', 'local-runtime', 'local', 'MLX Community');
  }
  if (['exo', 'exo.exe'].includes(name) || /[\\/]exo[\\/]/.test(path) || (/python/.test(name) && /exo[_-]inference/.test(command))) {
    return identity('exo', 'Distributed local inference', 'model-runner', 'local-runtime', 'local', 'exo');
  }
  if (/tensorrt[_-]?llm/.test(name) || /[\\/]tensorrt[_-]?llm[\\/]/.test(path) || (/python/.test(name) && /tensorrt[_-]?llm/.test(command))) {
    return identity('TensorRT-LLM', 'NVIDIA TensorRT', 'model-runner', 'local-runtime', 'local', 'NVIDIA');
  }
  if (/llama-server|llama\.cpp|koboldcpp|kobold/.test(haystack)) {
    const application = /kobold/.test(haystack) ? 'KoboldCpp' : 'llama.cpp';
    return identity(application, 'llama.cpp', 'model-runner', 'local-runtime', 'local', application);
  }
  const localAiHost = name === 'localai.exe' || /[\\/]localai(?:[\\/]|\.exe$)/.test(path) || /^localai(?:\.exe)?(?:\s|$)/.test(command.trim());
  if (/vllm|text-generation-webui|oobabooga/.test(haystack) || localAiHost) {
    const application = /vllm/.test(haystack) ? 'vLLM' : localAiHost ? 'LocalAI' : 'Text generation web UI';
    return identity(application, /vllm/.test(haystack) ? 'vLLM' : 'Python inference', 'model-runner', 'local-runtime', 'local', 'Open source');
  }
  if (/stable.?diffusion|invokeai|fooocus/.test(haystack)) {
    const application = /fooocus/.test(haystack) ? 'Fooocus' : /invokeai/.test(haystack) ? 'InvokeAI' : 'Stable Diffusion';
    return identity(application, 'PyTorch', 'model-runner', 'creative-ai', 'local', 'Open source');
  }
  if (/python/.test(name) && /torch|tensorflow|transformers|diffusion/.test(haystack)) {
    return identity('Python AI workload', /tensorflow/.test(haystack) ? 'TensorFlow' : 'PyTorch', 'model-runner', 'local-runtime', 'local', 'Python');
  }
  return null;
}

function modelBase({ id, application, runtime, model, status = 'detected', source, confidence, sizeBytes = 0 }) {
  return {
    id,
    application,
    runtime,
    model,
    status,
    source,
    confidence,
    family: '',
    parameters: '',
    quantization: '',
    format: '',
    contextLength: 0,
    sizeBytes: finite(sizeBytes),
    allocatedBytes: 0,
    allocatedVramBytes: 0,
    installed: true,
    location: application,
    expiresAt: null,
  };
}

export function parseOllamaPayload(payload = {}) {
  return (Array.isArray(payload.models) ? payload.models : []).map((model) => ({
    ...modelBase({
      id: `ollama:${model.digest || model.model || model.name}`,
      application: 'Ollama',
      runtime: 'Ollama engine',
      model: model.name || model.model || 'Unknown Ollama model',
      status: 'loaded',
      source: 'Ollama /api/ps',
      confidence: 100,
      sizeBytes: model.size,
    }),
    family: model.details?.family || model.details?.families?.join(', ') || '',
    parameters: model.details?.parameter_size || '',
    quantization: model.details?.quantization_level || '',
    format: model.details?.format || '',
    contextLength: finite(model.context_length),
    allocatedBytes: finite(model.size),
    allocatedVramBytes: finite(model.size_vram),
    expiresAt: model.expires_at || null,
  }));
}

export function parseOllamaTagsPayload(payload = {}) {
  return (Array.isArray(payload.models) ? payload.models : []).map((model) => ({
    ...modelBase({
      id: `ollama:${model.digest || model.model || model.name}`,
      application: 'Ollama',
      runtime: 'Ollama engine',
      model: model.name || model.model || 'Unknown Ollama model',
      source: 'Ollama /api/tags',
      confidence: 100,
      sizeBytes: model.size,
    }),
    family: model.details?.family || model.details?.families?.join(', ') || '',
    parameters: model.details?.parameter_size || '',
    quantization: model.details?.quantization_level || '',
    format: model.details?.format || '',
    location: 'Ollama library',
  }));
}

export function parseOllamaManifest(payload = {}, relativeParts = []) {
  const parts = relativeParts.map(String).filter(Boolean);
  if (parts.length < 3) return null;
  const tag = parts.pop();
  parts.shift(); // Registry host, normally registry.ollama.ai.
  if (parts[0] === 'library') parts.shift();
  if (parts.length === 0 || !tag) return null;
  const model = `${parts.join('/')}:${tag}`;
  const layers = Array.isArray(payload.layers) ? payload.layers : [];
  const sizeBytes = finite(payload.config?.size) + layers.reduce((sum, layer) => sum + finite(layer?.size), 0);
  return {
    ...modelBase({
      id: `ollama-manifest:${model}`,
      application: 'Ollama',
      runtime: 'Ollama engine',
      model,
      source: 'Ollama manifest store',
      confidence: 99,
      sizeBytes,
    }),
    format: 'ollama',
    location: 'Ollama library',
  };
}

export function parseLmStudioPayload(payload = {}) {
  const rows = Array.isArray(payload.models) ? payload.models : Array.isArray(payload.data) ? payload.data : [];
  return rows.flatMap((model, modelIndex) => {
    const loaded = Array.isArray(model.loaded_instances) ? model.loaded_instances : Array.isArray(model.loadedInstances) ? model.loadedInstances : [];
    const instances = loaded.length > 0 ? loaded : [null];
    return instances.map((instance, instanceIndex) => ({
      ...modelBase({
        id: `lmstudio:${model.key || model.id || model.path || modelIndex}:${instance?.id || instanceIndex}`,
        application: 'LM Studio',
        runtime: model.compatibility_type || 'llama.cpp',
        model: model.display_name || model.displayName || model.id || model.key || 'LM Studio model',
        status: instance || model.state === 'loaded' ? 'loaded' : 'detected',
        source: 'LM Studio /api/v0/models',
        confidence: 98,
        sizeBytes: model.size_bytes || model.size,
      }),
      family: model.architecture || model.arch || '',
      parameters: model.params_string || model.parameter_size || '',
      quantization: model.quantization || '',
      format: model.format || model.compatibility_type || '',
      contextLength: finite(instance?.context_length || instance?.contextLength || model.max_context_length),
      allocatedBytes: instance ? finite(model.size_bytes || model.size) : 0,
      allocatedVramBytes: finite(instance?.vram_bytes || instance?.vramBytes),
      location: 'LM Studio library',
    }));
  });
}

export function parseLlamaCppPayload(payload = {}) {
  const modelPath = payload.model_path || payload.modelPath || payload.default_generation_settings?.model || payload.model;
  if (!modelPath) return [];
  const model = String(modelPath).split(/[\\/]/).pop();
  return [{
    ...modelBase({ id: `llamacpp:${model}`, application: 'llama.cpp', runtime: 'llama.cpp server', model, status: 'loaded', source: 'llama.cpp /props', confidence: 98 }),
    family: payload.model_meta?.general?.architecture || '',
    quantization: model.match(/Q\d(?:_[A-Z0-9]+)+/i)?.[0] || '',
    format: model.split('.').pop() || '',
    contextLength: finite(payload.default_generation_settings?.n_ctx || payload.n_ctx),
    installed: false,
    location: 'Active llama.cpp server',
  }];
}

export function parseOpenAiModelsPayload(payload = {}, application = 'Local API', runtime = 'OpenAI-compatible', source = 'Local /v1/models') {
  return (Array.isArray(payload.data) ? payload.data : []).map((row, index) => ({
    ...modelBase({
      id: `${application.toLowerCase().replace(/[^a-z0-9]+/g, '-')}:${row.id || index}`,
      application,
      runtime,
      model: row.id || row.name || 'Local API model',
      status: row.state === 'not-loaded' ? 'detected' : 'loaded',
      source,
      confidence: 92,
      sizeBytes: row.size_bytes || row.size,
    }),
    family: row.architecture || row.arch || '',
    parameters: row.parameter_size || row.params_string || '',
    quantization: row.quantization || '',
    format: row.format || row.compatibility_type || '',
    contextLength: finite(row.max_context_length || row.context_length),
    location: `${application} library`,
  }));
}

function collectModelNames(value, names, key = '') {
  if (typeof value === 'string' && /(model|ckpt|unet|vae|lora)(_name)?$/i.test(key) && /\.(safetensors|ckpt|gguf|pt|pth)$/i.test(value)) {
    names.add(value.split(/[\\/]/).pop());
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((item) => collectModelNames(item, names, key));
    return;
  }
  if (value && typeof value === 'object') {
    Object.entries(value).forEach(([childKey, childValue]) => collectModelNames(childValue, names, childKey));
  }
}

export function parseComfyQueuePayload(payload = {}) {
  const running = Array.isArray(payload.queue_running) ? payload.queue_running : [];
  const names = new Set();
  running.forEach((item) => collectModelNames(item, names));
  return [...names].map((model, index) => ({
    ...modelBase({ id: `comfyui:${model}:${index}`, application: 'ComfyUI', runtime: 'PyTorch', model, status: 'active', source: 'ComfyUI active queue', confidence: 98 }),
    format: model.split('.').pop() || '',
    installed: false,
    location: 'Active ComfyUI workflow',
  }));
}

function quantizationFromName(name) {
  return String(name).match(/(?:^|[-_.])(Q\d(?:_[A-Z0-9]+)+|Q\d_K|F16|F32|BF16)(?:[-_.]|$)/i)?.[1]?.toUpperCase() || '';
}

function parametersFromName(name) {
  const match = String(name).match(/(?:^|[-_.])(\d+(?:\.\d+)?)\s*[bB](?:[-_.]|$)/);
  return match ? `${match[1]}B` : '';
}

function directoryModelName(directory, rootPath) {
  const relativeParts = directory.slice(rootPath.length).split(/[\\/]/).filter(Boolean);
  const huggingFaceRepository = relativeParts.find((part) => part.startsWith('models--'));
  if (huggingFaceRepository) return huggingFaceRepository.slice('models--'.length).replaceAll('--', '/');
  return basename(directory);
}

function candidateModelRoots() {
  const home = homedir();
  const appData = process.env.APPDATA || join(home, 'AppData', 'Roaming');
  const localAppData = process.env.LOCALAPPDATA || join(home, 'AppData', 'Local');
  const roots = [
    { path: join(home, '.ollama', 'models'), label: 'Ollama' },
    { path: join(home, '.lmstudio', 'models'), label: 'LM Studio' },
    { path: join(home, '.cache', 'lm-studio', 'models'), label: 'LM Studio' },
    { path: join(home, '.cache', 'huggingface', 'hub'), label: 'Hugging Face' },
    { path: join(home, '.cache', 'llama.cpp'), label: 'llama.cpp' },
    { path: join(home, 'Documents', 'GPT4All'), label: 'GPT4All' },
    { path: join(localAppData, 'nomic.ai', 'GPT4All'), label: 'GPT4All' },
    { path: join(appData, 'nomic.ai', 'GPT4All'), label: 'GPT4All' },
    { path: join(appData, 'Jan', 'data', 'models'), label: 'Jan' },
    { path: join(localAppData, 'Jan', 'data', 'models'), label: 'Jan' },
  ];
  for (const customPath of String(process.env.VISOR_MODEL_PATHS || '').split(delimiter).filter(Boolean)) {
    roots.push({ path: customPath, label: 'Custom folder' });
  }
  return [...new Map(roots.map((root) => [root.path.toLowerCase(), root])).values()];
}

async function scanOllamaManifests(root) {
  const manifestRoot = join(root.path, 'manifests');
  const models = [];
  const pending = [{ path: manifestRoot, depth: 0 }];
  let scannedEntries = 0;
  while (pending.length && scannedEntries < MAX_SCANNED_ENTRIES && models.length < MAX_FILE_MODELS) {
    const current = pending.shift();
    let entries;
    try {
      entries = await readdir(current.path, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      scannedEntries += 1;
      if (scannedEntries >= MAX_SCANNED_ENTRIES || models.length >= MAX_FILE_MODELS) break;
      const path = join(current.path, entry.name);
      if (entry.isDirectory() && current.depth < 6) {
        pending.push({ path, depth: current.depth + 1 });
        continue;
      }
      if (!entry.isFile()) continue;
      try {
        const file = await stat(path);
        if (file.size > 2 * 1024 * 1024) continue;
        const relativeParts = path.slice(manifestRoot.length).split(/[\\/]/).filter(Boolean);
        const model = parseOllamaManifest(JSON.parse(await readFile(path, 'utf8')), relativeParts);
        if (model) models.push(model);
      } catch {
        // A corrupt or concurrently updated manifest must not break telemetry.
      }
    }
  }
  return models;
}

async function scanModelRoot(root) {
  const models = [];
  const pending = [{ path: root.path, depth: 0 }];
  let scannedEntries = 0;
  while (pending.length && scannedEntries < MAX_SCANNED_ENTRIES && models.length < MAX_FILE_MODELS) {
    const current = pending.shift();
    let entries;
    try {
      entries = await readdir(current.path, { withFileTypes: true });
    } catch {
      continue;
    }
    const transformerWeights = entries.filter((entry) => entry.isFile() && (
      /\.safetensors$/i.test(entry.name)
      || /\.onnx$/i.test(entry.name)
      || /^pytorch_model(?:-\d+-of-\d+)?\.bin$/i.test(entry.name)
    ));
    if (transformerWeights.length > 0 && models.length < MAX_FILE_MODELS) {
      const weightStats = await Promise.all(transformerWeights.map(async (entry) => {
        try { return await stat(join(current.path, entry.name)); } catch { return null; }
      }));
      const sizeBytes = weightStats.reduce((sum, file) => sum + finite(file?.size), 0);
      const model = directoryModelName(current.path, root.path) || `${root.label} model`;
      const formats = [...new Set(transformerWeights.map((entry) => extname(entry.name).slice(1).toLowerCase()))];
      models.push({
        ...modelBase({
          id: `weights:${root.label.toLowerCase().replace(/[^a-z0-9]+/g, '-')}:${model.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
          application: root.label,
          runtime: 'Transformers runtime',
          model,
          source: `${root.label} weight folder`,
          confidence: 90,
          sizeBytes,
        }),
        parameters: parametersFromName(model),
        format: formats.join('+'),
        location: root.label,
      });
    }
    for (const entry of entries) {
      scannedEntries += 1;
      if (scannedEntries >= MAX_SCANNED_ENTRIES || models.length >= MAX_FILE_MODELS) break;
      const path = join(current.path, entry.name);
      if (entry.isSymbolicLink()) continue;
      if (entry.isDirectory() && current.depth < 6 && !/^(node_modules|\.git|blobs)$/i.test(entry.name)) {
        pending.push({ path, depth: current.depth + 1 });
        continue;
      }
      const extension = extname(entry.name).toLowerCase();
      if (!entry.isFile() || !['.gguf', '.ggml'].includes(extension)) continue;
      let file;
      try {
        file = await stat(path);
      } catch {
        continue;
      }
      const model = basename(entry.name);
      models.push({
        ...modelBase({
          id: `file:${root.label.toLowerCase().replace(/[^a-z0-9]+/g, '-')}:${model.toLowerCase()}`,
          application: root.label,
          runtime: 'GGUF runtime',
          model,
          source: `${root.label} model folder`,
          confidence: 95,
          sizeBytes: file.size,
        }),
        parameters: parametersFromName(model),
        quantization: quantizationFromName(model),
        format: extension.slice(1),
        location: root.label,
      });
    }
  }
  return models;
}

async function discoverFileModels() {
  if (Date.now() < fileInventoryCache.expiresAt) return fileInventoryCache;
  const roots = candidateModelRoots();
  const scanned = await Promise.all(roots.map(async (root) => ({
    root,
    models: root.label === 'Ollama' ? await scanOllamaManifests(root) : await scanModelRoot(root),
  })));
  fileInventoryCache = {
    expiresAt: Date.now() + INVENTORY_TTL_MS,
    models: scanned.flatMap((item) => item.models),
    roots: scanned.filter((item) => item.models.length > 0).map((item) => ({ label: item.root.label, modelCount: item.models.length })),
  };
  return fileInventoryCache;
}

async function probeJson(url, timeoutMs = 650) {
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(timeoutMs), cache: 'no-store' });
    if (!response.ok) return null;
    return await response.json();
  } catch {
    return null;
  }
}

function mergeModels(...collections) {
  const merged = new Map();
  const statusRank = { detected: 0, loaded: 1, active: 2 };
  for (const model of collections.flat()) {
    const key = `${model.application}:${model.model}`.toLowerCase();
    const existing = merged.get(key);
    if (!existing) {
      merged.set(key, model);
      continue;
    }
    const incomingWins = statusRank[model.status] > statusRank[existing.status];
    const primary = incomingWins ? model : existing;
    const secondary = incomingWins ? existing : model;
    merged.set(key, {
      ...secondary,
      ...primary,
      sizeBytes: Math.max(finite(existing.sizeBytes), finite(model.sizeBytes)),
      installed: Boolean(existing.installed || model.installed),
      source: [...new Set([existing.source, model.source].filter(Boolean))].join(' + '),
    });
  }
  return [...merged.values()].sort((left, right) => statusRank[right.status] - statusRank[left.status] || finite(right.sizeBytes) - finite(left.sizeBytes));
}

export async function discoverLocalModels() {
  const [ollamaRunning, ollamaInstalled, lmStudio, llamaCpp, comfy, jan, gpt4all, vllm, textGenerationWebUi, openAi8080, files] = await Promise.all([
    probeJson('http://127.0.0.1:11434/api/ps'),
    probeJson('http://127.0.0.1:11434/api/tags'),
    probeJson('http://127.0.0.1:1234/api/v0/models'),
    probeJson('http://127.0.0.1:8080/props'),
    probeJson('http://127.0.0.1:8188/queue'),
    probeJson('http://127.0.0.1:1337/v1/models'),
    probeJson('http://127.0.0.1:4891/v1/models'),
    probeJson('http://127.0.0.1:8000/v1/models'),
    probeJson('http://127.0.0.1:5000/v1/models'),
    probeJson('http://127.0.0.1:8080/v1/models'),
    discoverFileModels(),
  ]);

  const models = mergeModels(
    ollamaInstalled ? parseOllamaTagsPayload(ollamaInstalled) : [],
    ollamaRunning ? parseOllamaPayload(ollamaRunning) : [],
    lmStudio ? parseLmStudioPayload(lmStudio) : [],
    llamaCpp ? parseLlamaCppPayload(llamaCpp) : [],
    comfy ? parseComfyQueuePayload(comfy) : [],
    jan ? parseOpenAiModelsPayload(jan, 'Jan', 'llama.cpp', 'Jan /v1/models') : [],
    gpt4all ? parseOpenAiModelsPayload(gpt4all, 'GPT4All', 'Local API', 'GPT4All /v1/models') : [],
    vllm ? parseOpenAiModelsPayload(vllm, 'vLLM / compatible', 'OpenAI-compatible', 'Loopback :8000 /v1/models') : [],
    textGenerationWebUi ? parseOpenAiModelsPayload(textGenerationWebUi, 'Text generation web UI / compatible', 'OpenAI-compatible', 'Loopback :5000 /v1/models') : [],
    openAi8080 && !llamaCpp ? parseOpenAiModelsPayload(openAi8080, 'LocalAI / compatible', 'OpenAI-compatible', 'Loopback :8080 /v1/models') : [],
    files.models,
  );

  return {
    scannedAt: new Date().toISOString(),
    adapters: [
      { id: 'ollama', name: 'Ollama', status: ollamaRunning || ollamaInstalled ? 'online' : 'offline', endpoint: '127.0.0.1:11434' },
      { id: 'lmstudio', name: 'LM Studio', status: lmStudio ? 'online' : 'offline', endpoint: '127.0.0.1:1234' },
      { id: 'llamacpp', name: 'llama.cpp', status: llamaCpp ? 'online' : 'offline', endpoint: '127.0.0.1:8080' },
      { id: 'jan', name: 'Jan', status: jan ? 'online' : 'offline', endpoint: '127.0.0.1:1337' },
      { id: 'comfyui', name: 'ComfyUI', status: comfy ? 'online' : 'offline', endpoint: '127.0.0.1:8188' },
      { id: 'gpt4all', name: 'GPT4All API', status: gpt4all ? 'online' : 'offline', endpoint: '127.0.0.1:4891' },
      { id: 'openai-8000', name: 'vLLM / compatible', status: vllm ? 'online' : 'offline', endpoint: '127.0.0.1:8000' },
      { id: 'openai-5000', name: 'Text generation web UI / compatible', status: textGenerationWebUi ? 'online' : 'offline', endpoint: '127.0.0.1:5000' },
      { id: 'openai-8080', name: 'LocalAI / llama.cpp compatible', status: openAi8080 ? 'online' : 'offline', endpoint: '127.0.0.1:8080' },
    ],
    models,
    modelRoots: files.roots,
  };
}

export function buildLocalAiSnapshot(processes = [], discovery = { adapters: [], models: [], modelRoots: [] }) {
  const grouped = new Map();
  for (const processData of processes) {
    const detected = detectAiApplication(processData);
    if (!detected) continue;
    const key = `${detected.application}:${detected.runtime}`;
    const group = grouped.get(key) || {
      id: key.toLowerCase().replace(/[^a-z0-9]+/g, '-'),
      application: detected.application,
      runtime: detected.runtime,
      provider: detected.provider,
      category: detected.category,
      execution: detected.execution,
      processes: [],
      cpu: 0,
      gpu: 0,
      memoryGb: 0,
      vramGb: 0,
      energyWatts: 0,
    };
    group.processes.push({
      pid: processData.id,
      name: processData.name,
      role: detected.role,
      cpu: finite(processData.cpu),
      gpu: finite(processData.gpu),
      memoryGb: finite(processData.memory),
      vramGb: finite(processData.vram),
      energyWatts: finite(processData.energyWatts),
    });
    group.cpu += finite(processData.cpu);
    group.gpu += finite(processData.gpu);
    group.memoryGb += finite(processData.memory);
    group.vramGb += finite(processData.vram);
    group.energyWatts += finite(processData.energyWatts);
    grouped.set(key, group);
  }

  const applications = [...grouped.values()].map((group) => ({
    ...group,
    cpu: round(group.cpu),
    gpu: round(group.gpu),
    memoryGb: round(group.memoryGb, 2),
    vramGb: round(group.vramGb, 2),
    energyWatts: round(group.energyWatts),
    active: group.cpu >= 1 || group.gpu >= 1 || group.vramGb >= 0.25,
    running: group.processes.length > 0,
  })).sort((left, right) => right.energyWatts - left.energyWatts || right.memoryGb - left.memoryGb);

  const models = (discovery.models || []).map((model) => {
    const application = applications.find((item) => item.application === model.application);
    const singleModelForApp = (discovery.models || []).filter((item) => item.application === model.application && item.status !== 'detected').length === 1;
    const runner = application?.processes
      .filter((item) => item.role === 'model-runner' || item.role === 'runtime')
      .sort((left, right) => right.energyWatts - left.energyWatts || right.vramGb - left.vramGb)[0] || null;
    return {
      ...model,
      status: application?.active && model.status !== 'detected' ? 'active' : model.status,
      process: runner,
      applicationEnergyWatts: singleModelForApp ? round(application?.energyWatts) : null,
      applicationCpu: round(application?.cpu),
      applicationGpu: round(application?.gpu),
      allocatedRamBytes: Math.max(0, finite(model.allocatedBytes) - finite(model.allocatedVramBytes)),
      allocatedVramGb: round(finite(model.allocatedVramBytes) / GiB, 2),
    };
  });

  return {
    scannedAt: discovery.scannedAt || null,
    adapters: discovery.adapters || [],
    modelRoots: discovery.modelRoots || [],
    models,
    applications,
    activeModelCount: models.filter((model) => model.status === 'active').length,
    loadedModelCount: models.filter((model) => model.status === 'active' || model.status === 'loaded').length,
    installedModelCount: models.filter((model) => model.installed).length,
    serviceCount: applications.length,
  };
}
