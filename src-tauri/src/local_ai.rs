use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const GIB: f64 = 1_073_741_824.0;
const MAX_SCANNED_ENTRIES: usize = 4_000;
const MAX_FILE_MODELS: usize = 240;
const INVENTORY_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct Identity {
    application: &'static str,
    runtime: &'static str,
    role: &'static str,
    category: &'static str,
    execution: &'static str,
    provider: &'static str,
}

fn identity(
    application: &'static str,
    runtime: &'static str,
    role: &'static str,
    category: &'static str,
    execution: &'static str,
    provider: &'static str,
) -> Identity {
    Identity {
        application,
        runtime,
        role,
        category,
        execution,
        provider,
    }
}

fn detect(process: &Value) -> Option<Identity> {
    let name = string(process.get("name")).to_ascii_lowercase();
    let path = string(process.get("path")).to_ascii_lowercase();
    let command = string(process.get("command")).to_ascii_lowercase();
    let haystack = format!("{name} {path} {command}");
    let node_host = matches!(
        name.as_str(),
        "node" | "node.exe" | "bun" | "bun.exe" | "deno" | "deno.exe"
    );

    // Cloud clients and coding agents must win before generic host executable rules.
    if path.contains("openai.codex")
        || path.contains("/openai/codex/")
        || path.contains("\\openai\\codex\\")
        || name == "codex.exe"
        || name == "codex"
        || name.starts_with("codex-")
        || (node_host && (command.contains("@openai/codex") || command.contains("@openai\\codex")))
    {
        return Some(identity(
            "Codex",
            "OpenAI cloud",
            if name.contains("host") || name.contains("runner") {
                "agent-helper"
            } else {
                "coding-agent"
            },
            "coding-agent",
            "cloud",
            "OpenAI",
        ));
    }
    if matches!(name.as_str(), "chatgpt" | "chatgpt.exe") || path.contains("openai.chatgpt") {
        return Some(identity(
            "ChatGPT",
            "OpenAI cloud",
            "application",
            "cloud-client",
            "cloud",
            "OpenAI",
        ));
    }
    let claude_binary = matches!(name.as_str(), "claude" | "claude.exe");
    let claude_code_marker = path.contains("claude-code")
        || path.contains(".claude\\")
        || path.contains(".claude/")
        || command.contains("@anthropic-ai/claude-code")
        || command.contains("@anthropic-ai\\claude-code")
        || command.contains("claude-code")
        || command.contains(".claude\\")
        || command.contains(".claude/");
    if claude_binary || (node_host && claude_code_marker) {
        let coding_agent = claude_code_marker;
        return Some(identity(
            if coding_agent {
                "Claude Code"
            } else {
                "Claude"
            },
            "Anthropic cloud",
            if coding_agent {
                "coding-agent"
            } else {
                "application"
            },
            if coding_agent {
                "coding-agent"
            } else {
                "cloud-client"
            },
            "cloud",
            "Anthropic",
        ));
    }
    if matches!(name.as_str(), "kimi" | "kimi.exe")
        || (node_host
            && (path.contains("@moonshot-ai")
                || command.contains("@moonshot-ai")
                || command.contains("kimi-code")
                || command.contains("kimi-cli")))
    {
        return Some(identity(
            "Kimi Code",
            "Moonshot cloud",
            "coding-agent",
            "coding-agent",
            "cloud",
            "Moonshot AI",
        ));
    }
    if matches!(
        name.as_str(),
        "copilot-agent"
            | "copilot-agent.exe"
            | "copilot-language-server"
            | "copilot-language-server.exe"
    ) || path.contains("github copilot")
        || path.contains("github-copilot")
        || (node_host
            && (command.contains("github copilot")
                || command.contains("github-copilot")
                || command.contains("copilot-agent")
                || command.contains("copilot-language-server")))
    {
        return Some(identity(
            "GitHub Copilot",
            "GitHub cloud",
            "agent-helper",
            "coding-agent",
            "cloud",
            "GitHub",
        ));
    }
    if path.contains("@google/gemini-cli")
        || path.contains("@google\\gemini-cli")
        || path.contains("/.gemini/")
        || path.contains("\\.gemini\\")
        || (node_host
            && (command.contains("@google/gemini-cli")
                || command.contains("@google\\gemini-cli")
                || command.contains("/.gemini/")
                || command.contains("\\.gemini\\")))
        || matches!(name.as_str(), "gemini" | "gemini.exe" | "gemini.cmd")
    {
        return Some(identity(
            "Gemini CLI",
            "Google cloud",
            "coding-agent",
            "coding-agent",
            "cloud",
            "Google",
        ));
    }
    if matches!(name.as_str(), "opencode" | "opencode.exe")
        || path.contains("/opencode/")
        || path.contains("\\opencode\\")
    {
        return Some(identity(
            "OpenCode",
            "Configured AI provider",
            "coding-agent",
            "coding-agent",
            "hybrid",
            "OpenCode",
        ));
    }
    if matches!(name.as_str(), "aider" | "aider.exe")
        || (name.contains("python")
            && (command.contains(" -m aider ")
                || command.ends_with(" -m aider")
                || command.contains("/aider-chat/")
                || command.contains("\\aider-chat\\")))
    {
        return Some(identity(
            "Aider",
            "Configured AI provider",
            "coding-agent",
            "coding-agent",
            "hybrid",
            "Aider",
        ));
    }
    if matches!(name.as_str(), "cursor" | "cursor.exe")
        || path.contains("/cursor/")
        || path.contains("\\cursor\\")
    {
        return Some(identity(
            "Cursor",
            "Configured cloud/local provider",
            "coding-client",
            "coding-agent",
            "hybrid",
            "Cursor",
        ));
    }
    if matches!(name.as_str(), "windsurf" | "windsurf.exe")
        || path.contains("/windsurf/")
        || path.contains("\\windsurf\\")
        || path.contains("codeium")
    {
        return Some(identity(
            "Windsurf",
            "Codeium cloud",
            "coding-client",
            "coding-agent",
            "cloud",
            "Codeium",
        ));
    }
    if matches!(name.as_str(), "q" | "q.exe" | "q-desktop" | "q-desktop.exe")
        && (path.contains("amazon")
            || command.contains("amazon q")
            || command.contains("q developer"))
    {
        return Some(identity(
            "Amazon Q Developer",
            "AWS cloud",
            "coding-agent",
            "coding-agent",
            "cloud",
            "AWS",
        ));
    }
    if matches!(name.as_str(), "perplexity" | "perplexity.exe")
        || path.contains("/perplexity/")
        || path.contains("\\perplexity\\")
    {
        return Some(identity(
            "Perplexity",
            "Perplexity cloud",
            "application",
            "cloud-client",
            "cloud",
            "Perplexity",
        ));
    }
    if node_host
        && (command.contains("continue.continue")
            || command.contains("saoudrizwan.claude-dev")
            || command.contains("rooveterinaryinc.roo-cline"))
    {
        let (application, provider) = if command.contains("roo-cline") {
            ("Roo Code", "Configured provider")
        } else if command.contains("claude-dev") {
            ("Cline", "Configured provider")
        } else {
            ("Continue", "Configured provider")
        };
        return Some(identity(
            application,
            "Configured cloud/local provider",
            "agent-helper",
            "coding-agent",
            "hybrid",
            provider,
        ));
    }

    if name.contains("msedgewebview2") && haystack.contains("ollama app.exe") {
        return Some(identity(
            "Ollama Desktop",
            "WebView UI",
            "ui-helper",
            "local-runtime",
            "local",
            "Ollama",
        ));
    }
    if haystack.contains("ollama") {
        return Some(identity(
            "Ollama",
            "Ollama engine",
            if name.contains("runner") || name.contains("llama-server") {
                "model-runner"
            } else if command.contains("serve") {
                "runtime"
            } else {
                "application"
            },
            "local-runtime",
            "local",
            "Ollama",
        ));
    }
    if haystack.contains("comfyui") || haystack.contains("comfy-desktop") {
        return Some(identity(
            "ComfyUI",
            "PyTorch",
            if name.contains("python") {
                "model-runner"
            } else {
                "application"
            },
            "creative-ai",
            "local",
            "Comfy Org",
        ));
    }
    if haystack.contains("lm studio")
        || haystack.contains("lmstudio")
        || haystack.contains("llmster")
    {
        return Some(identity(
            "LM Studio",
            "llama.cpp",
            if haystack.contains("server")
                || haystack.contains("llama")
                || haystack.contains("llmster")
            {
                "model-runner"
            } else {
                "application"
            },
            "local-runtime",
            "local",
            "LM Studio",
        ));
    }
    if haystack.contains("gpt4all") {
        return Some(identity(
            "GPT4All",
            "llama.cpp",
            if haystack.contains("server") || haystack.contains("llama") {
                "model-runner"
            } else {
                "application"
            },
            "local-runtime",
            "local",
            "Nomic AI",
        ));
    }
    if name == "jan.exe" || haystack.contains("jan.ai") {
        return Some(identity(
            "Jan",
            "llama.cpp",
            if haystack.contains("server") || haystack.contains("llama") {
                "model-runner"
            } else {
                "application"
            },
            "local-runtime",
            "local",
            "Jan",
        ));
    }
    if name.contains("anythingllm")
        || name.contains("anything-llm")
        || path.contains("/anythingllm/")
        || path.contains("/anything-llm/")
        || path.contains("\\anythingllm\\")
        || path.contains("\\anything-llm\\")
    {
        return Some(identity(
            "AnythingLLM",
            "Local orchestrator",
            "application",
            "local-runtime",
            "hybrid",
            "Mintplex Labs",
        ));
    }
    if name.contains("open-webui")
        || name.contains("open_webui")
        || path.contains("/open-webui/")
        || path.contains("/open_webui/")
        || path.contains("\\open-webui\\")
        || path.contains("\\open_webui\\")
        || (name.contains("python") && command.contains(" -m open_webui"))
    {
        return Some(identity(
            "Open WebUI",
            "Local AI interface",
            "application",
            "local-runtime",
            "hybrid",
            "Open WebUI",
        ));
    }
    if matches!(name.as_str(), "msty" | "msty.exe")
        || path.contains("/msty/")
        || path.contains("\\msty\\")
    {
        return Some(identity(
            "Msty",
            "Local AI interface",
            "application",
            "local-runtime",
            "hybrid",
            "Msty",
        ));
    }
    if matches!(name.as_str(), "tabby" | "tabby.exe") || haystack.contains("tabbyml") {
        return Some(identity(
            "Tabby",
            "Local code model server",
            "model-runner",
            "local-runtime",
            "local",
            "TabbyML",
        ));
    }
    if name.contains("llamafile") || path.contains("/llamafile/") || path.contains("\\llamafile\\")
    {
        return Some(identity(
            "llamafile",
            "llama.cpp",
            "model-runner",
            "local-runtime",
            "local",
            "Mozilla",
        ));
    }
    if name.contains("mlx_lm")
        || name.contains("mlx-lm")
        || path.contains("/mlx_lm/")
        || path.contains("/mlx-lm/")
        || path.contains("\\mlx_lm\\")
        || path.contains("\\mlx-lm\\")
        || (name.contains("python") && command.contains("mlx_lm"))
    {
        return Some(identity(
            "MLX LM",
            "Apple MLX",
            "model-runner",
            "local-runtime",
            "local",
            "MLX Community",
        ));
    }
    if matches!(name.as_str(), "exo" | "exo.exe")
        || path.contains("/exo/")
        || path.contains("\\exo\\")
        || (name.contains("python")
            && (command.contains("exo-inference") || command.contains("exo_inference")))
    {
        return Some(identity(
            "exo",
            "Distributed local inference",
            "model-runner",
            "local-runtime",
            "local",
            "exo",
        ));
    }
    if name.contains("tensorrt-llm")
        || name.contains("tensorrt_llm")
        || path.contains("/tensorrt-llm/")
        || path.contains("/tensorrt_llm/")
        || path.contains("\\tensorrt-llm\\")
        || path.contains("\\tensorrt_llm\\")
        || (name.contains("python")
            && (command.contains("tensorrt-llm") || command.contains("tensorrt_llm")))
    {
        return Some(identity(
            "TensorRT-LLM",
            "NVIDIA TensorRT",
            "model-runner",
            "local-runtime",
            "local",
            "NVIDIA",
        ));
    }
    if haystack.contains("llama-server")
        || haystack.contains("llama.cpp")
        || haystack.contains("kobold")
    {
        let application = if haystack.contains("kobold") {
            "KoboldCpp"
        } else {
            "llama.cpp"
        };
        return Some(identity(
            application,
            "llama.cpp",
            "model-runner",
            "local-runtime",
            "local",
            application,
        ));
    }
    let local_ai_host = name == "localai.exe"
        || path.contains("\\localai\\")
        || path.contains("/localai/")
        || path.ends_with("\\localai.exe")
        || command.trim_start().starts_with("localai ")
        || command.trim() == "localai";
    if haystack.contains("vllm")
        || haystack.contains("text-generation-webui")
        || haystack.contains("oobabooga")
        || local_ai_host
    {
        return Some(identity(
            if haystack.contains("vllm") {
                "vLLM"
            } else if local_ai_host {
                "LocalAI"
            } else {
                "Text generation web UI"
            },
            if haystack.contains("vllm") {
                "vLLM"
            } else {
                "Python inference"
            },
            "model-runner",
            "local-runtime",
            "local",
            "Open source",
        ));
    }
    if haystack.contains("stable-diffusion")
        || haystack.contains("stable_diffusion")
        || haystack.contains("fooocus")
        || haystack.contains("invokeai")
    {
        return Some(identity(
            if haystack.contains("fooocus") {
                "Fooocus"
            } else if haystack.contains("invokeai") {
                "InvokeAI"
            } else {
                "Stable Diffusion"
            },
            "PyTorch",
            "model-runner",
            "creative-ai",
            "local",
            "Open source",
        ));
    }
    if name.contains("python")
        && (haystack.contains("torch")
            || haystack.contains("tensorflow")
            || haystack.contains("transformers"))
    {
        return Some(identity(
            "Python AI workload",
            if haystack.contains("tensorflow") {
                "TensorFlow"
            } else {
                "PyTorch"
            },
            "model-runner",
            "local-runtime",
            "local",
            "Python",
        ));
    }
    None
}

fn probe_json(url: &str) -> Option<Value> {
    ureq::get(url)
        .timeout(Duration::from_millis(500))
        .call()
        .ok()?
        .into_json::<Value>()
        .ok()
}

fn string(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn number(value: Option<&Value>) -> f64 {
    value.and_then(Value::as_f64).unwrap_or_default()
}

trait EmptyFallback {
    fn or_else(self, fallback: impl FnOnce() -> String) -> String;
}

impl EmptyFallback for String {
    fn or_else(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() {
            fallback()
        } else {
            self
        }
    }
}

struct ModelBaseInput<'a> {
    id: String,
    application: &'a str,
    runtime: &'a str,
    model: String,
    status: &'a str,
    source: &'a str,
    confidence: u64,
    size_bytes: f64,
    location: String,
}

fn model_base(input: ModelBaseInput<'_>) -> Value {
    let ModelBaseInput {
        id,
        application,
        runtime,
        model,
        status,
        source,
        confidence,
        size_bytes,
        location,
    } = input;
    json!({
        "id": id,
        "application": application,
        "runtime": runtime,
        "model": model,
        "status": status,
        "source": source,
        "confidence": confidence,
        "family": "",
        "parameters": "",
        "quantization": "",
        "format": "",
        "contextLength": 0,
        "sizeBytes": size_bytes,
        "allocatedBytes": 0,
        "allocatedVramBytes": 0,
        "installed": true,
        "location": location,
        "expiresAt": null
    })
}

fn parse_ollama(payload: &Value) -> Vec<Value> {
    payload
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            let model = string(row.get("name")).or_else(|| string(row.get("model")));
            let digest = string(row.get("digest"));
            let display_model = if model.is_empty() {
                "Unknown Ollama model".to_string()
            } else {
                model.clone()
            };
            let mut item = model_base(ModelBaseInput {
                id: format!(
                    "ollama:{}",
                    if digest.is_empty() { &model } else { &digest }
                ),
                application: "Ollama",
                runtime: "Ollama engine",
                model: display_model.clone(),
                status: "loaded",
                source: "Ollama /api/ps",
                confidence: 100,
                // `/api/ps.size` is the live allocation, which can include a
                // large KV context. Weight bytes come from `/api/tags`/manifests.
                size_bytes: 0.0,
                location: "Ollama library".to_string(),
            });
            let details = row.get("details").unwrap_or(&Value::Null);
            let parameters = string(details.get("parameter_size"));
            let quantization = string(details.get("quantization_level"));
            item["family"] = Value::String(string(details.get("family")));
            item["parameters"] = Value::String(
                if parameters.is_empty() || parameters.eq_ignore_ascii_case("unknown") {
                    infer_parameters(&display_model)
                } else {
                    parameters
                },
            );
            item["quantization"] = Value::String(
                if quantization.is_empty() || quantization.eq_ignore_ascii_case("unknown") {
                    infer_quantization(&display_model)
                } else {
                    quantization
                },
            );
            item["format"] = Value::String(string(details.get("format")));
            item["contextLength"] = json!(number(row.get("context_length")));
            item["allocatedBytes"] = json!(number(row.get("size")));
            item["allocatedVramBytes"] = json!(number(row.get("size_vram")));
            item["expiresAt"] = row.get("expires_at").cloned().unwrap_or(Value::Null);
            item
        })
        .collect()
}

fn parse_ollama_tags(payload: &Value) -> Vec<Value> {
    payload
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            let model = string(row.get("name")).or_else(|| string(row.get("model")));
            let digest = string(row.get("digest"));
            let display_model = if model.is_empty() {
                "Unknown Ollama model".to_string()
            } else {
                model.clone()
            };
            let mut item = model_base(ModelBaseInput {
                id: format!(
                    "ollama:{}",
                    if digest.is_empty() { &model } else { &digest }
                ),
                application: "Ollama",
                runtime: "Ollama engine",
                model: display_model.clone(),
                status: "detected",
                source: "Ollama /api/tags",
                confidence: 100,
                size_bytes: number(row.get("size")),
                location: "Ollama library".to_string(),
            });
            let details = row.get("details").unwrap_or(&Value::Null);
            let parameters = string(details.get("parameter_size"));
            let quantization = string(details.get("quantization_level"));
            item["family"] = Value::String(string(details.get("family")));
            item["parameters"] = Value::String(
                if parameters.is_empty() || parameters.eq_ignore_ascii_case("unknown") {
                    infer_parameters(&display_model)
                } else {
                    parameters
                },
            );
            item["quantization"] = Value::String(
                if quantization.is_empty() || quantization.eq_ignore_ascii_case("unknown") {
                    infer_quantization(&display_model)
                } else {
                    quantization
                },
            );
            item["format"] = Value::String(string(details.get("format")));
            item
        })
        .collect()
}

fn parse_lm_studio(payload: &Value) -> Vec<Value> {
    let rows = payload
        .get("models")
        .and_then(Value::as_array)
        .or_else(|| payload.get("data").and_then(Value::as_array));
    let mut models = Vec::new();
    for (index, row) in rows.into_iter().flatten().enumerate() {
        let instances = row
            .get("loaded_instances")
            .and_then(Value::as_array)
            .or_else(|| row.get("loadedInstances").and_then(Value::as_array));
        let empty = Vec::new();
        let instance_rows = instances.unwrap_or(&empty);
        if instance_rows.is_empty() {
            let key = string(row.get("key"))
                .or_else(|| string(row.get("id")))
                .or_else(|| index.to_string());
            let name = string(row.get("display_name"))
                .or_else(|| string(row.get("displayName")))
                .or_else(|| string(row.get("id")))
                .or_else(|| "LM Studio model".to_string());
            let mut item = model_base(ModelBaseInput {
                id: format!("lmstudio:{key}:0"),
                application: "LM Studio",
                runtime: "llama.cpp",
                model: name,
                status: "detected",
                source: "LM Studio /api/v0/models",
                confidence: 98,
                size_bytes: number(row.get("size_bytes")).max(number(row.get("size"))),
                location: "LM Studio library".to_string(),
            });
            enrich_lm_studio(&mut item, row, None);
            models.push(item);
        } else {
            for (instance_index, instance) in instance_rows.iter().enumerate() {
                let key = string(row.get("key"))
                    .or_else(|| string(row.get("id")))
                    .or_else(|| index.to_string());
                let name = string(row.get("display_name"))
                    .or_else(|| string(row.get("displayName")))
                    .or_else(|| string(row.get("id")))
                    .or_else(|| "Loaded LM Studio model".to_string());
                let mut item = model_base(ModelBaseInput {
                    id: format!("lmstudio:{key}:{instance_index}"),
                    application: "LM Studio",
                    runtime: "llama.cpp",
                    model: name,
                    status: "loaded",
                    source: "LM Studio /api/v0/models",
                    confidence: 98,
                    size_bytes: number(row.get("size_bytes")).max(number(row.get("size"))),
                    location: "LM Studio library".to_string(),
                });
                enrich_lm_studio(&mut item, row, Some(instance));
                models.push(item);
            }
        }
    }
    models
}

fn enrich_lm_studio(item: &mut Value, row: &Value, instance: Option<&Value>) {
    item["family"] = Value::String(string(row.get("architecture")));
    item["parameters"] = Value::String(string(row.get("params_string")));
    item["quantization"] = Value::String(string(row.get("quantization")));
    item["format"] = Value::String(string(row.get("format")));
    if let Some(instance) = instance {
        item["contextLength"] = json!(number(instance.get("context_length")));
        item["allocatedBytes"] = json!(number(row.get("size_bytes")).max(number(row.get("size"))));
        item["allocatedVramBytes"] = json!(number(instance.get("vram_bytes")));
    }
}

fn parse_llama_cpp(payload: &Value) -> Vec<Value> {
    let path = string(payload.get("model_path")).or_else(|| string(payload.get("model")));
    if path.is_empty() {
        return Vec::new();
    }
    let model = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_string();
    let mut item = model_base(ModelBaseInput {
        id: format!("llamacpp:{model}"),
        application: "llama.cpp",
        runtime: "llama.cpp server",
        model: model.clone(),
        status: "loaded",
        source: "llama.cpp /props",
        confidence: 98,
        size_bytes: 0.0,
        location: "Active llama.cpp server".to_string(),
    });
    item["installed"] = Value::Bool(false);
    item["format"] = Value::String(model.rsplit('.').next().unwrap_or_default().to_string());
    item["contextLength"] = json!(number(payload.get("n_ctx")));
    vec![item]
}

fn parse_openai_models(
    payload: &Value,
    application: &str,
    runtime: &str,
    source: &str,
) -> Vec<Value> {
    payload
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, row)| {
            let name = string(row.get("id"))
                .or_else(|| string(row.get("name")))
                .or_else(|| "Local API model".to_string());
            let slug = application.to_ascii_lowercase().replace(' ', "-");
            let mut item = model_base(ModelBaseInput {
                id: format!(
                    "{slug}:{}",
                    if name.is_empty() {
                        index.to_string()
                    } else {
                        name.clone()
                    }
                ),
                application,
                runtime,
                model: name,
                status: if string(row.get("state")) == "not-loaded" {
                    "detected"
                } else {
                    "loaded"
                },
                source,
                confidence: 92,
                size_bytes: number(row.get("size_bytes")).max(number(row.get("size"))),
                location: format!("{application} library"),
            });
            item["family"] = Value::String(string(row.get("architecture")));
            item["parameters"] = Value::String(string(row.get("parameter_size")));
            item["quantization"] = Value::String(string(row.get("quantization")));
            item["format"] = Value::String(string(row.get("format")));
            item["contextLength"] =
                json!(number(row.get("max_context_length")).max(number(row.get("context_length"))));
            item
        })
        .collect()
}

fn collect_model_names(value: &Value, key: &str, names: &mut BTreeSet<String>) {
    match value {
        Value::String(text)
            if (key.to_ascii_lowercase().contains("model")
                || key.to_ascii_lowercase().contains("ckpt")
                || key.to_ascii_lowercase().contains("unet")
                || key.to_ascii_lowercase().contains("vae")
                || key.to_ascii_lowercase().contains("lora"))
                && [".safetensors", ".ckpt", ".gguf", ".pt", ".pth"]
                    .iter()
                    .any(|extension| text.to_ascii_lowercase().ends_with(extension)) =>
        {
            names.insert(text.rsplit(['/', '\\']).next().unwrap_or(text).to_string());
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| collect_model_names(item, key, names)),
        Value::Object(map) => map
            .iter()
            .for_each(|(child_key, child)| collect_model_names(child, child_key, names)),
        _ => {}
    }
}

fn parse_comfy(payload: &Value) -> Vec<Value> {
    let mut names = BTreeSet::new();
    if let Some(running) = payload.get("queue_running").and_then(Value::as_array) {
        running
            .iter()
            .for_each(|item| collect_model_names(item, "", &mut names));
    }
    names
        .into_iter()
        .enumerate()
        .map(|(index, model)| {
            let mut item = model_base(ModelBaseInput {
                id: format!("comfyui:{model}:{index}"),
                application: "ComfyUI",
                runtime: "PyTorch",
                model: model.clone(),
                status: "active",
                source: "ComfyUI active queue",
                confidence: 98,
                size_bytes: 0.0,
                location: "Active ComfyUI workflow".to_string(),
            });
            item["installed"] = Value::Bool(false);
            item["format"] =
                Value::String(model.rsplit('.').next().unwrap_or_default().to_string());
            item
        })
        .collect()
}

#[derive(Clone, Default)]
struct FileInventory {
    scanned_at: Option<Instant>,
    models: Vec<Value>,
    roots: Vec<Value>,
}

static FILE_INVENTORY: OnceLock<Mutex<FileInventory>> = OnceLock::new();

fn candidate_model_roots() -> Vec<(PathBuf, String)> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    let app_data = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
    } else if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
    };
    let local_app_data = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"))
    } else if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
    };
    let mut roots = vec![
        (home.join(".ollama").join("models"), "Ollama".to_string()),
        (
            home.join(".lmstudio").join("models"),
            "LM Studio".to_string(),
        ),
        (
            home.join(".cache").join("lm-studio").join("models"),
            "LM Studio".to_string(),
        ),
        (
            home.join(".cache").join("huggingface").join("hub"),
            "Hugging Face".to_string(),
        ),
        (
            home.join(".cache").join("llama.cpp"),
            "llama.cpp".to_string(),
        ),
        (
            app_data.join("LM Studio").join("models"),
            "LM Studio".to_string(),
        ),
        (
            home.join("Documents").join("GPT4All"),
            "GPT4All".to_string(),
        ),
        (
            local_app_data.join("nomic.ai").join("GPT4All"),
            "GPT4All".to_string(),
        ),
        (
            app_data.join("nomic.ai").join("GPT4All"),
            "GPT4All".to_string(),
        ),
        (
            app_data.join("Jan").join("data").join("models"),
            "Jan".to_string(),
        ),
        (
            local_app_data.join("Jan").join("data").join("models"),
            "Jan".to_string(),
        ),
    ];
    if let Some(custom) = std::env::var_os("VISOR_MODEL_PATHS") {
        roots
            .extend(std::env::split_paths(&custom).map(|path| (path, "Custom folder".to_string())));
    }
    let mut seen = BTreeSet::new();
    roots.retain(|(path, _)| seen.insert(path.to_string_lossy().to_ascii_lowercase()));
    roots
}

fn infer_quantization(name: &str) -> String {
    name.split(['-', '.', ':', '/', '\\'])
        .map(|part| part.trim_matches('_').to_ascii_uppercase())
        .find(|part| {
            (part.starts_with('Q')
                && part[1..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_digit()))
                || matches!(part.as_str(), "F16" | "F32" | "BF16")
        })
        .unwrap_or_default()
}

fn infer_parameters(name: &str) -> String {
    name.split(['-', '_', '.', ':', '/', '\\'])
        .find_map(|part| {
            let upper = part.to_ascii_uppercase();
            let value = upper.strip_suffix('B')?;
            value.parse::<f64>().ok().map(|_| upper)
        })
        .unwrap_or_default()
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn directory_model_name(directory: &Path, root: &Path) -> String {
    if let Ok(relative) = directory.strip_prefix(root) {
        for component in relative.components() {
            let part = component.as_os_str().to_string_lossy();
            if let Some(repository) = part.strip_prefix("models--") {
                return repository.replace("--", "/");
            }
        }
    }
    directory
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "Local model".to_string())
}

fn parse_ollama_manifest(payload: &Value, relative_parts: &[String]) -> Option<Value> {
    if relative_parts.len() < 3 {
        return None;
    }
    let tag = relative_parts.last()?.trim();
    let mut model_parts = relative_parts[1..relative_parts.len() - 1].to_vec();
    if model_parts.first().is_some_and(|part| part == "library") {
        model_parts.remove(0);
    }
    if tag.is_empty() || model_parts.is_empty() {
        return None;
    }
    let model = format!("{}:{tag}", model_parts.join("/"));
    let size = number(payload.get("config").and_then(|config| config.get("size")))
        + payload
            .get("layers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|layer| number(layer.get("size")))
            .sum::<f64>();
    let mut item = model_base(ModelBaseInput {
        id: format!("ollama-manifest:{model}"),
        application: "Ollama",
        runtime: "Ollama engine",
        model,
        status: "detected",
        source: "Ollama manifest store",
        confidence: 99,
        size_bytes: size,
        location: "Ollama library".to_string(),
    });
    item["format"] = Value::String("ollama".to_string());
    Some(item)
}

fn scan_ollama_manifests(root: &Path) -> Vec<Value> {
    let manifest_root = root.join("manifests");
    let mut models = Vec::new();
    let mut pending = VecDeque::from([(manifest_root.clone(), 0_u8)]);
    let mut scanned_entries = 0;
    while let Some((directory, depth)) = pending.pop_front() {
        if scanned_entries >= MAX_SCANNED_ENTRIES || models.len() >= MAX_FILE_MODELS {
            break;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            scanned_entries += 1;
            if scanned_entries >= MAX_SCANNED_ENTRIES || models.len() >= MAX_FILE_MODELS {
                break;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() && depth < 6 {
                pending.push_back((path, depth + 1));
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
                continue;
            }
            let Ok(relative) = path.strip_prefix(&manifest_root) else {
                continue;
            };
            let parts = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>();
            let Some(payload) = fs::read_to_string(&path)
                .ok()
                .and_then(|source| serde_json::from_str::<Value>(&source).ok())
            else {
                continue;
            };
            if let Some(model) = parse_ollama_manifest(&payload, &parts) {
                models.push(model);
            }
        }
    }
    models
}

fn scan_model_root(root: &Path, label: &str) -> Vec<Value> {
    let mut models = Vec::new();
    let mut pending = VecDeque::from([(root.to_path_buf(), 0_u8)]);
    let mut scanned_entries = 0;
    while let Some((directory, depth)) = pending.pop_front() {
        if scanned_entries >= MAX_SCANNED_ENTRIES || models.len() >= MAX_FILE_MODELS {
            break;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        let entries = entries.flatten().collect::<Vec<_>>();
        let transformer_weights = entries
            .iter()
            .filter(|entry| {
                if !entry
                    .file_type()
                    .ok()
                    .is_some_and(|file_type| file_type.is_file())
                {
                    return false;
                }
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                name.ends_with(".safetensors")
                    || name.ends_with(".onnx")
                    || (name.starts_with("pytorch_model") && name.ends_with(".bin"))
            })
            .collect::<Vec<_>>();
        if !transformer_weights.is_empty() && models.len() < MAX_FILE_MODELS {
            let size = transformer_weights
                .iter()
                .filter_map(|entry| fs::metadata(entry.path()).ok())
                .map(|metadata| metadata.len() as f64)
                .sum::<f64>();
            let model = directory_model_name(&directory, root);
            let formats = transformer_weights
                .iter()
                .filter_map(|entry| {
                    entry
                        .path()
                        .extension()
                        .map(|value| value.to_string_lossy().to_ascii_lowercase())
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join("+");
            let source = format!("{label} weight folder");
            let mut item = model_base(ModelBaseInput {
                id: format!("weights:{}:{}", slug(label), slug(&model)),
                application: label,
                runtime: "Transformers runtime",
                model: model.clone(),
                status: "detected",
                source: &source,
                confidence: 90,
                size_bytes: size,
                location: label.to_string(),
            });
            item["parameters"] = Value::String(infer_parameters(&model));
            item["format"] = Value::String(formats);
            models.push(item);
        }
        for entry in entries {
            scanned_entries += 1;
            if scanned_entries >= MAX_SCANNED_ENTRIES || models.len() >= MAX_FILE_MODELS {
                break;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() && depth < 6 {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if !matches!(name.as_str(), "node_modules" | ".git" | "blobs") {
                    pending.push_back((path, depth + 1));
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !matches!(extension.as_str(), "gguf" | "ggml") {
                continue;
            }
            let model = entry.file_name().to_string_lossy().to_string();
            let size = entry
                .metadata()
                .map(|metadata| metadata.len() as f64)
                .unwrap_or_default();
            let source = format!("{label} model folder");
            let mut item = model_base(ModelBaseInput {
                id: format!("file:{}:{}", slug(label), slug(&model)),
                application: label,
                runtime: "GGUF runtime",
                model: model.clone(),
                status: "detected",
                source: &source,
                confidence: 95,
                size_bytes: size,
                location: label.to_string(),
            });
            item["parameters"] = Value::String(infer_parameters(&model));
            item["quantization"] = Value::String(infer_quantization(&model));
            item["format"] = Value::String(extension);
            models.push(item);
        }
    }
    models
}

fn discover_file_models() -> (Vec<Value>, Vec<Value>) {
    let cache = FILE_INVENTORY.get_or_init(|| Mutex::new(FileInventory::default()));
    if let Ok(inventory) = cache.lock() {
        if inventory
            .scanned_at
            .is_some_and(|instant| instant.elapsed() < INVENTORY_TTL)
        {
            return (inventory.models.clone(), inventory.roots.clone());
        }
    }
    let mut models = Vec::new();
    let mut roots = Vec::new();
    for (path, label) in candidate_model_roots() {
        let found = if label == "Ollama" {
            scan_ollama_manifests(&path)
        } else {
            scan_model_root(&path, &label)
        };
        if !found.is_empty() {
            roots.push(json!({ "label": label, "modelCount": found.len() }));
            models.extend(found);
        }
        if models.len() >= MAX_FILE_MODELS {
            break;
        }
    }
    if let Ok(mut inventory) = cache.lock() {
        *inventory = FileInventory {
            scanned_at: Some(Instant::now()),
            models: models.clone(),
            roots: roots.clone(),
        };
    }
    (models, roots)
}

fn status_rank(value: &Value) -> u8 {
    match value.get("status").and_then(Value::as_str) {
        Some("active") => 2,
        Some("loaded") => 1,
        _ => 0,
    }
}

fn merge_models(collections: Vec<Vec<Value>>) -> Vec<Value> {
    let mut merged: BTreeMap<String, Value> = BTreeMap::new();
    for model in collections.into_iter().flatten() {
        let key = format!(
            "{}:{}",
            string(model.get("application")),
            string(model.get("model"))
        )
        .to_ascii_lowercase();
        let Some(existing) = merged.remove(&key) else {
            merged.insert(key, model);
            continue;
        };
        let (mut primary, secondary) = if status_rank(&model) > status_rank(&existing) {
            (model, existing)
        } else {
            (existing, model)
        };
        primary["sizeBytes"] =
            json!(number(primary.get("sizeBytes")).max(number(secondary.get("sizeBytes"))));
        primary["installed"] = json!(
            primary
                .get("installed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || secondary
                    .get("installed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        );
        let sources = [
            string(primary.get("source")),
            string(secondary.get("source")),
        ]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(" + ");
        primary["source"] = Value::String(sources);
        merged.insert(key, primary);
    }
    let mut models = merged.into_values().collect::<Vec<_>>();
    models.sort_by(|left, right| {
        status_rank(right)
            .cmp(&status_rank(left))
            .then_with(|| number(right.get("sizeBytes")).total_cmp(&number(left.get("sizeBytes"))))
    });
    models
}

fn cloud_provider_catalog(applications: &[Value]) -> Vec<Value> {
    let providers: [(&str, &str, &[&str]); 23] = [
        ("openai", "OpenAI", &["OPENAI_API_KEY"]),
        ("azure-openai", "Azure OpenAI", &["AZURE_OPENAI_API_KEY"]),
        ("anthropic", "Anthropic", &["ANTHROPIC_API_KEY"]),
        (
            "google",
            "Google Gemini",
            &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        ),
        ("mistral", "Mistral AI", &["MISTRAL_API_KEY"]),
        ("groq", "Groq", &["GROQ_API_KEY"]),
        ("cohere", "Cohere", &["COHERE_API_KEY"]),
        ("together", "Together AI", &["TOGETHER_API_KEY"]),
        ("fireworks", "Fireworks AI", &["FIREWORKS_API_KEY"]),
        ("openrouter", "OpenRouter", &["OPENROUTER_API_KEY"]),
        ("deepseek", "DeepSeek", &["DEEPSEEK_API_KEY"]),
        ("xai", "xAI", &["XAI_API_KEY"]),
        ("perplexity", "Perplexity", &["PERPLEXITY_API_KEY"]),
        ("moonshot", "Moonshot AI", &["MOONSHOT_API_KEY"]),
        ("github-copilot", "GitHub Copilot", &[]),
        (
            "aws-bedrock",
            "Amazon Bedrock",
            &["AWS_ACCESS_KEY_ID", "AWS_PROFILE"],
        ),
        (
            "vertex-ai",
            "Google Vertex AI",
            &["GOOGLE_APPLICATION_CREDENTIALS"],
        ),
        (
            "huggingface",
            "Hugging Face",
            &["HF_TOKEN", "HUGGING_FACE_HUB_TOKEN"],
        ),
        ("replicate", "Replicate", &["REPLICATE_API_TOKEN"]),
        ("cerebras", "Cerebras", &["CEREBRAS_API_KEY"]),
        ("sambanova", "SambaNova", &["SAMBANOVA_API_KEY"]),
        (
            "cloudflare-ai",
            "Cloudflare Workers AI",
            &["CLOUDFLARE_API_TOKEN"],
        ),
        (
            "nvidia-nim",
            "NVIDIA NIM",
            &["NVIDIA_API_KEY", "NGC_API_KEY"],
        ),
    ];
    providers
        .into_iter()
        .map(|(id, name, environment_keys)| {
            let matching = applications
                .iter()
                .filter(|application| {
                    let provider = string(application.get("provider")).to_ascii_lowercase();
                    let application_name =
                        string(application.get("application")).to_ascii_lowercase();
                    match id {
                        "openai" => provider == "openai",
                        "anthropic" => provider == "anthropic",
                        "google" => provider == "google",
                        "perplexity" => provider == "perplexity",
                        "github-copilot" => provider == "github",
                        "aws-bedrock" => provider == "aws",
                        "vertex-ai" | "nvidia-nim" => false,
                        _ => provider.contains(id) || application_name.contains(id),
                    }
                })
                .collect::<Vec<_>>();
            let credential_signals = environment_keys
                .iter()
                .filter(|key| std::env::var_os(key).is_some())
                .copied()
                .collect::<Vec<_>>();
            let process_count = matching
                .iter()
                .map(|application| {
                    application
                        .get("processes")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or_default()
                })
                .sum::<usize>();
            let sum = |field: &str| {
                matching
                    .iter()
                    .map(|item| number(item.get(field)))
                    .sum::<f64>()
            };
            json!({
                "id": id,
                "name": name,
                "provider": name,
                "detected": process_count > 0 || !credential_signals.is_empty(),
                "credentialConfigured": !credential_signals.is_empty(),
                "credentialSignals": credential_signals,
                "localProcessCount": process_count,
                "cpu": sum("cpu"),
                "gpu": sum("gpu"),
                "memoryGb": sum("memoryGb"),
                "energyWatts": sum("energyWatts"),
                "billingVisible": false
            })
        })
        .collect()
}

pub fn build(processes: &[Value]) -> Value {
    let mut groups: BTreeMap<String, Value> = BTreeMap::new();
    for process in processes {
        let Some(identity) = detect(process) else {
            continue;
        };
        let key = format!("{}:{}", identity.application, identity.runtime);
        let group = groups.entry(key.clone()).or_insert_with(|| {
            json!({
                "id": slug(&key),
                "application": identity.application,
                "runtime": identity.runtime,
                "provider": identity.provider,
                "category": identity.category,
                "execution": identity.execution,
                "processes": [],
                "cpu": 0.0,
                "gpu": 0.0,
                "memoryGb": 0.0,
                "vramGb": 0.0,
                "energyWatts": 0.0,
                "running": true,
                "active": false
            })
        });
        let cpu = number(process.get("cpu"));
        let gpu = number(process.get("gpu"));
        let memory = number(process.get("memory"));
        let vram = number(process.get("vram"));
        let energy = number(process.get("energyWatts"));
        group["processes"].as_array_mut().unwrap().push(json!({
            "pid": process.get("id").and_then(Value::as_u64).unwrap_or_default(),
            "name": string(process.get("name")),
            "role": identity.role,
            "cpu": cpu,
            "gpu": gpu,
            "memoryGb": memory,
            "vramGb": vram,
            "energyWatts": energy
        }));
        for (field, increment) in [
            ("cpu", cpu),
            ("gpu", gpu),
            ("memoryGb", memory),
            ("vramGb", vram),
            ("energyWatts", energy),
        ] {
            group[field] = json!(number(group.get(field)) + increment);
        }
        group["active"] = json!(
            number(group.get("cpu")) >= 1.0
                || number(group.get("gpu")) >= 1.0
                || number(group.get("vramGb")) >= 0.25
        );
    }
    let mut applications: Vec<Value> = groups.into_values().collect();
    applications.sort_by(|left, right| {
        number(right.get("energyWatts"))
            .total_cmp(&number(left.get("energyWatts")))
            .then_with(|| number(right.get("memoryGb")).total_cmp(&number(left.get("memoryGb"))))
    });

    let (
        ollama_running,
        ollama_installed,
        lm_studio,
        llama_cpp,
        comfy,
        jan,
        gpt4all,
        vllm,
        text_generation_web_ui,
        open_ai_8080,
    ) = thread::scope(|scope| {
        let ollama_running = scope.spawn(|| probe_json("http://127.0.0.1:11434/api/ps"));
        let ollama_installed = scope.spawn(|| probe_json("http://127.0.0.1:11434/api/tags"));
        let lm_studio = scope.spawn(|| probe_json("http://127.0.0.1:1234/api/v0/models"));
        let llama_cpp = scope.spawn(|| probe_json("http://127.0.0.1:8080/props"));
        let comfy = scope.spawn(|| probe_json("http://127.0.0.1:8188/queue"));
        let jan = scope.spawn(|| probe_json("http://127.0.0.1:1337/v1/models"));
        let gpt4all = scope.spawn(|| probe_json("http://127.0.0.1:4891/v1/models"));
        let vllm = scope.spawn(|| probe_json("http://127.0.0.1:8000/v1/models"));
        let text_generation_web_ui = scope.spawn(|| probe_json("http://127.0.0.1:5000/v1/models"));
        let open_ai_8080 = scope.spawn(|| probe_json("http://127.0.0.1:8080/v1/models"));
        (
            ollama_running.join().unwrap_or(None),
            ollama_installed.join().unwrap_or(None),
            lm_studio.join().unwrap_or(None),
            llama_cpp.join().unwrap_or(None),
            comfy.join().unwrap_or(None),
            jan.join().unwrap_or(None),
            gpt4all.join().unwrap_or(None),
            vllm.join().unwrap_or(None),
            text_generation_web_ui.join().unwrap_or(None),
            open_ai_8080.join().unwrap_or(None),
        )
    });
    let (file_models, model_roots) = discover_file_models();
    let mut models = merge_models(vec![
        ollama_installed
            .as_ref()
            .map(parse_ollama_tags)
            .unwrap_or_default(),
        ollama_running
            .as_ref()
            .map(parse_ollama)
            .unwrap_or_default(),
        lm_studio.as_ref().map(parse_lm_studio).unwrap_or_default(),
        llama_cpp.as_ref().map(parse_llama_cpp).unwrap_or_default(),
        comfy.as_ref().map(parse_comfy).unwrap_or_default(),
        jan.as_ref()
            .map(|value| parse_openai_models(value, "Jan", "llama.cpp", "Jan /v1/models"))
            .unwrap_or_default(),
        gpt4all
            .as_ref()
            .map(|value| parse_openai_models(value, "GPT4All", "Local API", "GPT4All /v1/models"))
            .unwrap_or_default(),
        vllm.as_ref()
            .map(|value| {
                parse_openai_models(
                    value,
                    "vLLM / compatible",
                    "OpenAI-compatible",
                    "Loopback :8000 /v1/models",
                )
            })
            .unwrap_or_default(),
        text_generation_web_ui
            .as_ref()
            .map(|value| {
                parse_openai_models(
                    value,
                    "Text generation web UI / compatible",
                    "OpenAI-compatible",
                    "Loopback :5000 /v1/models",
                )
            })
            .unwrap_or_default(),
        if llama_cpp.is_none() {
            open_ai_8080
                .as_ref()
                .map(|value| {
                    parse_openai_models(
                        value,
                        "LocalAI / compatible",
                        "OpenAI-compatible",
                        "Loopback :8080 /v1/models",
                    )
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        },
        file_models,
    ]);

    let loaded_by_application =
        models
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut counts, model| {
                if model.get("status").and_then(Value::as_str) != Some("detected") {
                    *counts.entry(string(model.get("application"))).or_default() += 1;
                }
                counts
            });
    for model in &mut models {
        let application_name = string(model.get("application"));
        let application = applications
            .iter()
            .find(|item| string(item.get("application")) == application_name);
        let active = application
            .and_then(|item| item.get("active"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if active && model.get("status").and_then(Value::as_str) != Some("detected") {
            model["status"] = Value::String("active".to_string());
        }
        let runner = application
            .and_then(|item| item.get("processes"))
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .filter(|item| {
                        matches!(
                            item.get("role").and_then(Value::as_str),
                            Some("model-runner") | Some("runtime")
                        )
                    })
                    .max_by(|left, right| {
                        number(left.get("energyWatts")).total_cmp(&number(right.get("energyWatts")))
                    })
            })
            .cloned();
        let allocated = number(model.get("allocatedBytes"));
        let vram = number(model.get("allocatedVramBytes"));
        let single_loaded_model = loaded_by_application
            .get(&application_name)
            .copied()
            .unwrap_or_default()
            == 1;
        model["process"] = runner.unwrap_or(Value::Null);
        model["applicationEnergyWatts"] = if single_loaded_model {
            application
                .map(|item| json!(number(item.get("energyWatts"))))
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        model["applicationCpu"] = json!(application
            .map(|item| number(item.get("cpu")))
            .unwrap_or_default());
        model["applicationGpu"] = json!(application
            .map(|item| number(item.get("gpu")))
            .unwrap_or_default());
        model["allocatedRamBytes"] = json!((allocated - vram).max(0.0));
        model["allocatedVramGb"] = json!(vram / GIB);
    }

    let active_count = models
        .iter()
        .filter(|model| model.get("status").and_then(Value::as_str) == Some("active"))
        .count();
    let loaded_count = models
        .iter()
        .filter(|model| {
            matches!(
                model.get("status").and_then(Value::as_str),
                Some("active") | Some("loaded")
            )
        })
        .count();
    let installed_count = models
        .iter()
        .filter(|model| {
            model
                .get("installed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let cloud_providers = cloud_provider_catalog(&applications);
    json!({
        "scannedAt": chrono::Utc::now().to_rfc3339(),
        "adapters": [
            { "id": "ollama", "name": "Ollama", "status": if ollama_running.is_some() || ollama_installed.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:11434" },
            { "id": "lmstudio", "name": "LM Studio", "status": if lm_studio.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:1234" },
            { "id": "llamacpp", "name": "llama.cpp", "status": if llama_cpp.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:8080" },
            { "id": "jan", "name": "Jan", "status": if jan.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:1337" },
            { "id": "comfyui", "name": "ComfyUI", "status": if comfy.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:8188" },
            { "id": "gpt4all", "name": "GPT4All API", "status": if gpt4all.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:4891" },
            { "id": "openai-8000", "name": "vLLM / compatible", "status": if vllm.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:8000" },
            { "id": "openai-5000", "name": "Text generation web UI / compatible", "status": if text_generation_web_ui.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:5000" },
            { "id": "openai-8080", "name": "LocalAI / llama.cpp compatible", "status": if open_ai_8080.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:8080" }
        ],
        "modelRoots": model_roots,
        "models": models,
        "applications": applications,
        "cloudProviders": cloud_providers,
        "activeModelCount": active_count,
        "loadedModelCount": loaded_count,
        "installedModelCount": installed_count,
        "serviceCount": applications.len()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_host_is_not_mistaken_for_chatgpt() {
        let process = json!({
            "name": "ChatGPT.exe",
            "path": "C:\\Program Files\\WindowsApps\\OpenAI.Codex_1.0\\ChatGPT.exe",
            "command": ""
        });
        let result = detect(&process).unwrap();
        assert_eq!(result.application, "Codex");
        assert_eq!(result.execution, "cloud");
    }

    #[test]
    fn installed_ollama_model_is_not_marked_loaded() {
        let models = parse_ollama_tags(&json!({ "models": [{
            "name": "qwen3:8b", "size": 5_000_000_000_f64,
            "details": { "parameter_size": "8B", "quantization_level": "Q4_K_M", "format": "gguf" }
        }] }));
        assert_eq!(models[0]["status"], "detected");
        assert_eq!(models[0]["parameters"], "8B");
        assert_eq!(models[0]["sizeBytes"], 5_000_000_000_f64);
    }

    #[test]
    fn ollama_tag_metadata_falls_back_to_name() {
        let models = parse_ollama_tags(&json!({ "models": [{
            "name": "org/Qwen3.6-35B-A3B:Q6_K_P", "size": 29_000_000_000_f64,
            "details": { "parameter_size": "unknown", "quantization_level": "unknown", "format": "gguf" }
        }] }));
        assert_eq!(models[0]["parameters"], "35B");
        assert_eq!(models[0]["quantization"], "Q6_K_P");
    }

    #[test]
    fn offline_ollama_manifest_keeps_identity_and_size() {
        let model = parse_ollama_manifest(
            &json!({ "config": { "size": 120 }, "layers": [{ "size": 400 }, { "size": 500 }] }),
            &[
                "registry.ollama.ai".to_string(),
                "library".to_string(),
                "qwen3".to_string(),
                "8b".to_string(),
            ],
        )
        .expect("valid Ollama manifest");
        assert_eq!(model["model"], "qwen3:8b");
        assert_eq!(model["sizeBytes"], 1020.0);
        assert_eq!(model["status"], "detected");
    }

    #[test]
    fn expanded_catalog_separates_cloud_hybrid_and_local_services() {
        let cases = [
            (
                json!({ "name": "gemini.cmd", "path": "C:\\npm\\gemini.cmd" }),
                "Gemini CLI",
                "cloud",
            ),
            (
                json!({ "name": "opencode.exe", "path": "C:\\Tools\\OpenCode\\opencode.exe" }),
                "OpenCode",
                "hybrid",
            ),
            (
                json!({ "name": "open-webui.exe", "path": "C:\\Open-WebUI\\open-webui.exe" }),
                "Open WebUI",
                "hybrid",
            ),
            (
                json!({ "name": "llamafile.exe", "path": "C:\\Models\\llamafile.exe" }),
                "llamafile",
                "local",
            ),
            (
                json!({ "name": "python.exe", "command": "python -m mlx_lm.server --model qwen" }),
                "MLX LM",
                "local",
            ),
            (
                json!({ "name": "tabby.exe", "path": "C:\\TabbyML\\tabby.exe" }),
                "Tabby",
                "local",
            ),
            (
                json!({ "name": "cursor.exe", "path": "C:\\Users\\me\\AppData\\Local\\Programs\\Cursor\\Cursor.exe" }),
                "Cursor",
                "hybrid",
            ),
            (
                json!({ "name": "windsurf.exe", "path": "C:\\Program Files\\Windsurf\\Windsurf.exe" }),
                "Windsurf",
                "cloud",
            ),
            (
                json!({ "name": "perplexity.exe", "path": "C:\\Program Files\\Perplexity\\Perplexity.exe" }),
                "Perplexity",
                "cloud",
            ),
        ];
        for (process, application, execution) in cases {
            let detected = detect(&process).expect("service should be detected");
            assert_eq!(detected.application, application);
            assert_eq!(detected.execution, execution);
        }
    }

    #[test]
    fn product_names_in_shell_text_are_not_services() {
        let process = json!({
            "name": "powershell.exe",
            "path": "C:\\Windows\\System32\\WindowsPowerShell\\powershell.exe",
            "command": "Write-Output 'LocalAI ChatGPT Codex'"
        });
        assert!(detect(&process).is_none());
        let expanded = json!({
            "name": "powershell.exe",
            "path": "C:\\Windows\\System32\\WindowsPowerShell\\powershell.exe",
            "command": "Write-Output 'Gemini OpenCode Aider Open WebUI llamafile Claude Code Kimi Copilot'"
        });
        assert!(detect(&expanded).is_none());
    }

    #[test]
    fn cloud_provider_catalog_exposes_footprint_without_claiming_billing() {
        let snapshot = build(&[json!({
            "id": 42,
            "name": "codex.exe",
            "path": "C:\\Program Files\\OpenAI.Codex\\codex.exe",
            "cpu": 3.0,
            "gpu": 1.0,
            "memory": 0.4,
            "vram": 0.0,
            "energyWatts": 8.0
        })]);
        let providers = snapshot["cloudProviders"].as_array().unwrap();
        assert_eq!(providers.len(), 23);
        let openai = providers
            .iter()
            .find(|item| item["id"] == "openai")
            .unwrap();
        assert_eq!(openai["localProcessCount"], 1);
        assert_eq!(openai["billingVisible"], false);
    }
}
