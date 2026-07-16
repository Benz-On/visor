const GiB = 1024 ** 3;

const finite = (value, fallback = 0) => Number.isFinite(Number(value)) ? Number(value) : fallback;
const round = (value, digits = 1) => {
  const scale = 10 ** digits;
  return Math.round(finite(value) * scale) / scale;
};

export function detectAiApplication(processData = {}) {
  const name = String(processData.name || '').toLowerCase();
  const haystack = `${processData.name || ''} ${processData.command || ''} ${processData.path || ''}`.toLowerCase();

  if (name.includes('msedgewebview2') && /ollama app\.exe/.test(haystack)) {
    return { application: 'Ollama Desktop', runtime: 'WebView UI', role: 'ui-helper' };
  }
  if (/ollama/.test(haystack)) {
    const role = /llama-server|ollama_llama|runner/.test(name) ? 'model-runner' : /\bserve\b/.test(haystack) ? 'runtime' : 'application';
    return { application: 'Ollama', runtime: 'Ollama engine', role };
  }
  if (/comfyui|comfy-desktop|comfy desktop/.test(haystack)) {
    return { application: 'ComfyUI', runtime: 'PyTorch', role: /python/.test(name) ? 'model-runner' : 'application' };
  }
  if (/lm studio|lmstudio/.test(haystack)) {
    return { application: 'LM Studio', runtime: 'llama.cpp', role: /server|llama/.test(name) ? 'model-runner' : 'application' };
  }
  if (/llama-server|llama\.cpp|koboldcpp|kobold/.test(haystack)) {
    return { application: /kobold/.test(haystack) ? 'KoboldCpp' : 'llama.cpp', runtime: 'llama.cpp', role: 'model-runner' };
  }
  if (/stable.?diffusion|invokeai|fooocus/.test(haystack)) {
    return { application: /fooocus/.test(haystack) ? 'Fooocus' : /invokeai/.test(haystack) ? 'InvokeAI' : 'Stable Diffusion', runtime: 'PyTorch', role: 'model-runner' };
  }
  if (/python/.test(name) && /torch|tensorflow|transformers|diffusion/.test(haystack)) {
    return { application: 'Python AI workload', runtime: /tensorflow/.test(haystack) ? 'TensorFlow' : 'PyTorch', role: 'model-runner' };
  }
  return null;
}

export function parseOllamaPayload(payload = {}) {
  return (Array.isArray(payload.models) ? payload.models : []).map((model) => ({
    id: `ollama:${model.digest || model.model || model.name}`,
    application: 'Ollama',
    runtime: 'Ollama engine',
    model: model.name || model.model || 'Unknown Ollama model',
    status: 'loaded',
    source: 'Ollama /api/ps',
    confidence: 100,
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

export function parseLmStudioPayload(payload = {}) {
  const rows = Array.isArray(payload.models) ? payload.models : Array.isArray(payload.data) ? payload.data : [];
  return rows.flatMap((model) => {
    const loaded = model.loaded_instances || model.loadedInstances || [];
    if (!Array.isArray(loaded) || loaded.length === 0) return [];
    return loaded.map((instance, index) => ({
      id: `lmstudio:${model.key || model.id || model.path || index}:${instance.id || index}`,
      application: 'LM Studio',
      runtime: 'llama.cpp',
      model: model.display_name || model.displayName || model.id || model.key || 'Loaded LM Studio model',
      status: 'loaded',
      source: 'LM Studio local API',
      confidence: 96,
      family: model.architecture || model.arch || '',
      parameters: model.params_string || model.parameter_size || '',
      quantization: model.quantization || '',
      format: model.format || '',
      contextLength: finite(instance.context_length || instance.contextLength),
      allocatedBytes: finite(model.size_bytes || model.size),
      allocatedVramBytes: finite(instance.vram_bytes || instance.vramBytes),
      expiresAt: null,
    }));
  });
}

export function parseLlamaCppPayload(payload = {}) {
  const modelPath = payload.model_path || payload.modelPath || payload.default_generation_settings?.model || payload.model;
  if (!modelPath) return [];
  const model = String(modelPath).split(/[\\/]/).pop();
  return [{
    id: `llamacpp:${model}`,
    application: 'llama.cpp',
    runtime: 'llama.cpp server',
    model,
    status: 'loaded',
    source: 'llama.cpp /props',
    confidence: 98,
    family: payload.model_meta?.general?.architecture || '',
    parameters: '',
    quantization: model.match(/Q\d(?:_[A-Z0-9]+)+/i)?.[0] || '',
    format: model.split('.').pop() || '',
    contextLength: finite(payload.default_generation_settings?.n_ctx || payload.n_ctx),
    allocatedBytes: 0,
    allocatedVramBytes: 0,
    expiresAt: null,
  }];
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
    id: `comfyui:${model}:${index}`,
    application: 'ComfyUI',
    runtime: 'PyTorch',
    model,
    status: 'active',
    source: 'ComfyUI active queue',
    confidence: 98,
    family: '',
    parameters: '',
    quantization: '',
    format: model.split('.').pop() || '',
    contextLength: 0,
    allocatedBytes: 0,
    allocatedVramBytes: 0,
    expiresAt: null,
  }));
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

export async function discoverLocalModels() {
  const [ollama, lmStudio, llamaCpp, comfy] = await Promise.all([
    probeJson('http://127.0.0.1:11434/api/ps'),
    probeJson('http://127.0.0.1:1234/api/v0/models'),
    probeJson('http://127.0.0.1:8080/props'),
    probeJson('http://127.0.0.1:8188/queue'),
  ]);

  const models = [
    ...(ollama ? parseOllamaPayload(ollama) : []),
    ...(lmStudio ? parseLmStudioPayload(lmStudio) : []),
    ...(llamaCpp ? parseLlamaCppPayload(llamaCpp) : []),
    ...(comfy ? parseComfyQueuePayload(comfy) : []),
  ];

  return {
    scannedAt: new Date().toISOString(),
    adapters: [
      { id: 'ollama', name: 'Ollama', status: ollama ? 'online' : 'offline', endpoint: '127.0.0.1:11434' },
      { id: 'lmstudio', name: 'LM Studio', status: lmStudio ? 'online' : 'offline', endpoint: '127.0.0.1:1234' },
      { id: 'llamacpp', name: 'llama.cpp', status: llamaCpp ? 'online' : 'offline', endpoint: '127.0.0.1:8080' },
      { id: 'comfyui', name: 'ComfyUI', status: comfy ? 'online' : 'offline', endpoint: '127.0.0.1:8188' },
    ],
    models,
  };
}

export function buildLocalAiSnapshot(processes = [], discovery = { adapters: [], models: [] }) {
  const grouped = new Map();
  for (const processData of processes) {
    const identity = detectAiApplication(processData);
    if (!identity) continue;
    const key = `${identity.application}:${identity.runtime}`;
    const group = grouped.get(key) || {
      id: key.toLowerCase().replace(/[^a-z0-9]+/g, '-'),
      application: identity.application,
      runtime: identity.runtime,
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
      role: identity.role,
      cpu: processData.cpu,
      gpu: processData.gpu,
      vramGb: processData.vram,
      energyWatts: processData.energyWatts || 0,
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
  })).sort((left, right) => right.energyWatts - left.energyWatts || right.vramGb - left.vramGb);

  const models = (discovery.models || []).map((model) => {
    const application = applications.find((item) => item.application === model.application);
    const singleModelForApp = (discovery.models || []).filter((item) => item.application === model.application).length === 1;
    const runner = application?.processes
      .filter((item) => item.role === 'model-runner' || item.role === 'runtime')
      .sort((left, right) => right.energyWatts - left.energyWatts || right.vramGb - left.vramGb)[0] || null;
    return {
      ...model,
      status: application?.active ? 'active' : model.status,
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
    models,
    applications,
    activeModelCount: models.filter((model) => model.status === 'active').length,
    loadedModelCount: models.length,
  };
}
