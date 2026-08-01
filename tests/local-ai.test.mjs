import test from 'node:test';
import assert from 'node:assert/strict';
import { buildCloudProviderCatalog, buildLocalAiSnapshot, detectAiApplication, parseComfyQueuePayload, parseLlamaCppPayload, parseLmStudioPayload, parseOllamaManifest, parseOllamaPayload, parseOllamaTagsPayload } from '../agent/local-ai.mjs';

test('Ollama adapter keeps the exact model identity and allocation', () => {
  const [model] = parseOllamaPayload({
    models: [{
      name: 'huihui_ai/gemma-4-abliterated:26b-qat',
      digest: 'abc',
      size: 18_740_800_387,
      size_vram: 8_239_280_291,
      context_length: 262144,
      details: { family: 'gemma4', parameter_size: '25.2B', quantization_level: 'Q4_K_M', format: 'gguf' },
    }],
  });
  assert.equal(model.model, 'huihui_ai/gemma-4-abliterated:26b-qat');
  assert.equal(model.quantization, 'Q4_K_M');
  assert.equal(model.contextLength, 262144);
  assert.equal(model.confidence, 100);
});

test('local AI snapshot connects an exact model to its application workload', () => {
  const discovery = {
    models: parseOllamaPayload({ models: [{ name: 'qwen3:14b', size: 10_000_000_000, size_vram: 8_000_000_000 }] }),
    adapters: [],
  };
  const snapshot = buildLocalAiSnapshot([
    { id: 42, name: 'ollama.exe', command: 'ollama.exe serve', cpu: 4, gpu: 20, memory: 1.5, vram: 7.5, energyWatts: 62 },
  ], discovery);
  assert.equal(snapshot.models[0].application, 'Ollama');
  assert.equal(snapshot.models[0].process.pid, 42);
  assert.equal(snapshot.models[0].applicationEnergyWatts, 62);
});

test('Ollama WebView helpers are identified without becoming model runners', () => {
  const identity = detectAiApplication({ name: 'msedgewebview2.exe', command: '--webview-exe-name="ollama app.exe"' });
  assert.deepEqual(identity, { application: 'Ollama Desktop', runtime: 'WebView UI', role: 'ui-helper', category: 'local-runtime', execution: 'local', provider: 'Ollama' });
});

test('installed Ollama models are inventoried even when they are not running', () => {
  const [model] = parseOllamaTagsPayload({
    models: [{ name: 'qwen3-coder:30b', size: 18_556_700_761, details: { parameter_size: '30.5B', quantization_level: 'Q4_K_M', format: 'gguf' } }],
  });
  assert.equal(model.status, 'detected');
  assert.equal(model.installed, true);
  assert.equal(model.sizeBytes, 18_556_700_761);
  assert.equal(model.parameters, '30.5B');
});

test('offline Ollama manifests preserve model identity and total layer size', () => {
  const model = parseOllamaManifest({
    config: { size: 120 },
    layers: [{ size: 400 }, { size: 500 }],
  }, ['registry.ollama.ai', 'library', 'qwen3', '8b']);
  assert.equal(model.model, 'qwen3:8b');
  assert.equal(model.sizeBytes, 1020);
  assert.equal(model.status, 'detected');
  assert.equal(model.source, 'Ollama manifest store');
});

test('LM Studio keeps downloaded models that are not loaded', () => {
  const [model] = parseLmStudioPayload({ data: [{ id: 'qwen-14b', state: 'not-loaded', size_bytes: 9_000_000_000, quantization: 'Q4_K_M', max_context_length: 32768 }] });
  assert.equal(model.status, 'detected');
  assert.equal(model.model, 'qwen-14b');
  assert.equal(model.contextLength, 32768);
});

test('cloud coding agents are separated from local inference runtimes', () => {
  const codex = detectAiApplication({ name: 'ChatGPT.exe', path: 'C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.7\\ChatGPT.exe' });
  const claude = detectAiApplication({ name: 'node.exe', command: 'node C:\\npm\\node_modules\\@anthropic-ai\\claude-code\\cli.js' });
  const kimi = detectAiApplication({ name: 'kimi.exe', path: 'C:\\Users\\me\\.local\\bin\\kimi.exe' });
  assert.equal(codex.application, 'Codex');
  assert.equal(codex.execution, 'cloud');
  assert.equal(claude.application, 'Claude Code');
  assert.equal(kimi.application, 'Kimi Code');
});

test('expanded service catalog separates cloud, hybrid and local execution', () => {
  const cases = [
    [{ name: 'gemini.cmd', path: 'C:\\npm\\gemini.cmd' }, 'Gemini CLI', 'cloud'],
    [{ name: 'opencode.exe', path: 'C:\\Tools\\OpenCode\\opencode.exe' }, 'OpenCode', 'hybrid'],
    [{ name: 'open-webui.exe', path: 'C:\\Open WebUI\\open-webui.exe' }, 'Open WebUI', 'hybrid'],
    [{ name: 'llamafile.exe', path: 'C:\\Models\\llamafile.exe' }, 'llamafile', 'local'],
    [{ name: 'python.exe', command: 'python -m mlx_lm.server --model qwen' }, 'MLX LM', 'local'],
    [{ name: 'tabby.exe', path: 'C:\\TabbyML\\tabby.exe' }, 'Tabby', 'local'],
    [{ name: 'cursor.exe', path: 'C:\\Users\\me\\AppData\\Local\\Programs\\Cursor\\Cursor.exe' }, 'Cursor', 'hybrid'],
    [{ name: 'windsurf.exe', path: 'C:\\Program Files\\Windsurf\\Windsurf.exe' }, 'Windsurf', 'cloud'],
    [{ name: 'perplexity.exe', path: 'C:\\Program Files\\Perplexity\\Perplexity.exe' }, 'Perplexity', 'cloud'],
    [{ name: 'node.exe', command: 'node extension.js --extensionDevelopmentPath=continue.continue' }, 'Continue', 'hybrid'],
  ];
  for (const [processData, application, execution] of cases) {
    const detected = detectAiApplication(processData);
    assert.equal(detected.application, application);
    assert.equal(detected.execution, execution);
  }
});

test('commands that merely mention an AI product are not counted as its service', () => {
  assert.equal(detectAiApplication({ name: 'powershell.exe', command: "Write-Output 'LocalAI ChatGPT Codex'" }), null);
  assert.equal(detectAiApplication({ name: 'powershell.exe', command: "Write-Output 'Gemini OpenCode Aider'" }), null);
  assert.equal(detectAiApplication({ name: 'powershell.exe', command: "Write-Output 'Claude Code Kimi Copilot'" }), null);
  assert.equal(detectAiApplication({ name: 'chrome.exe', command: '--app=https://chatgpt.com/' }), null);
});

test('service totals expose live resource attribution per AI provider', () => {
  const snapshot = buildLocalAiSnapshot([
    { id: 10, name: 'codex.exe', path: 'OpenAI.Codex', cpu: 3, gpu: 2, memory: 0.4, vram: 0.1, energyWatts: 9 },
    { id: 11, name: 'ollama.exe', command: 'ollama serve', cpu: 5, gpu: 30, memory: 1.2, vram: 6, energyWatts: 74 },
  ], { adapters: [], models: [], modelRoots: [] });
  assert.equal(snapshot.serviceCount, 2);
  assert.equal(snapshot.applications.find((item) => item.application === 'Codex').execution, 'cloud');
  assert.equal(snapshot.applications.find((item) => item.application === 'Ollama').energyWatts, 74);
  assert.equal(snapshot.cloudProviders.length, 23);
  assert.equal(snapshot.cloudProviders.find((item) => item.id === 'openai').localProcessCount, 1);
});

test('cloud provider catalog reports credential presence without reading secret values', () => {
  const catalog = buildCloudProviderCatalog([], { OPENAI_API_KEY: 'must-never-be-returned', GEMINI_API_KEY: '' });
  const openai = catalog.find((item) => item.id === 'openai');
  const google = catalog.find((item) => item.id === 'google');
  assert.equal(openai.detected, true);
  assert.deepEqual(openai.credentialSignals, ['OPENAI_API_KEY']);
  assert.equal(JSON.stringify(catalog).includes('must-never-be-returned'), false);
  assert.equal(google.credentialConfigured, true);
  assert.equal(openai.billingVisible, false);
});

test('ComfyUI adapter extracts model files from the active queue', () => {
  const models = parseComfyQueuePayload({
    queue_running: [[1, 'prompt', { 4: { inputs: { ckpt_name: 'flux1-dev.safetensors' } } }]],
  });
  assert.equal(models[0].model, 'flux1-dev.safetensors');
  assert.equal(models[0].status, 'active');
});

test('llama.cpp adapter extracts the loaded GGUF file and context', () => {
  const [model] = parseLlamaCppPayload({
    model_path: 'D:\\Models\\Qwen3-14B-Q4_K_M.gguf',
    default_generation_settings: { n_ctx: 32768 },
  });
  assert.equal(model.model, 'Qwen3-14B-Q4_K_M.gguf');
  assert.equal(model.quantization, 'Q4_K_M');
  assert.equal(model.contextLength, 32768);
});
