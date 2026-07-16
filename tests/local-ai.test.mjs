import test from 'node:test';
import assert from 'node:assert/strict';
import { buildLocalAiSnapshot, detectAiApplication, parseComfyQueuePayload, parseLlamaCppPayload, parseOllamaPayload } from '../agent/local-ai.mjs';

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
  assert.deepEqual(identity, { application: 'Ollama Desktop', runtime: 'WebView UI', role: 'ui-helper' });
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
