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

    // Cloud clients and coding agents must win before generic host executable rules.
    if haystack.contains("openai.codex")
        || haystack.contains("@openai/codex")
        || haystack.contains("@openai\\codex")
        || name == "codex.exe"
        || name.starts_with("codex-")
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
    if name == "chatgpt.exe" || path.contains("openai.chatgpt") {
        return Some(identity(
            "ChatGPT",
            "OpenAI cloud",
            "application",
            "cloud-client",
            "cloud",
            "OpenAI",
        ));
    }
    if haystack.contains("claude-code")
        || haystack.contains("claude code")
        || haystack.contains(".claude\\")
        || haystack.contains(".claude/")
        || name == "claude.exe"
    {
        let coding_agent = haystack.contains("claude-code")
            || haystack.contains(".claude\\")
            || haystack.contains(".claude/")
            || name == "node.exe";
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
    if haystack.contains("@moonshot-ai")
        || haystack.contains("kimi-code")
        || haystack.contains("kimi-cli")
        || name == "kimi.exe"
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
    if haystack.contains("github copilot")
        || haystack.contains("github-copilot")
        || haystack.contains("copilot-agent")
        || haystack.contains("copilot-language-server")
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
    if haystack.contains("anythingllm") || haystack.contains("anything-llm") {
        return Some(identity(
            "AnythingLLM",
            "Local orchestrator",
            "application",
            "local-runtime",
            "hybrid",
            "Mintplex Labs",
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

fn model_base(
    id: String,
    application: &str,
    runtime: &str,
    model: String,
    status: &str,
    source: &str,
    confidence: u64,
    size_bytes: f64,
    location: &str,
) -> Value {
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
            let mut item = model_base(
                format!(
                    "ollama:{}",
                    if digest.is_empty() { &model } else { &digest }
                ),
                "Ollama",
                "Ollama engine",
                if model.is_empty() {
                    "Unknown Ollama model".to_string()
                } else {
                    model
                },
                "loaded",
                "Ollama /api/ps",
                100,
                number(row.get("size")),
                "Ollama library",
            );
            let details = row.get("details").unwrap_or(&Value::Null);
            item["family"] = Value::String(string(details.get("family")));
            item["parameters"] = Value::String(string(details.get("parameter_size")));
            item["quantization"] = Value::String(string(details.get("quantization_level")));
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
            let mut item = model_base(
                format!(
                    "ollama:{}",
                    if digest.is_empty() { &model } else { &digest }
                ),
                "Ollama",
                "Ollama engine",
                if model.is_empty() {
                    "Unknown Ollama model".to_string()
                } else {
                    model
                },
                "detected",
                "Ollama /api/tags",
                100,
                number(row.get("size")),
                "Ollama library",
            );
            let details = row.get("details").unwrap_or(&Value::Null);
            item["family"] = Value::String(string(details.get("family")));
            item["parameters"] = Value::String(string(details.get("parameter_size")));
            item["quantization"] = Value::String(string(details.get("quantization_level")));
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
            let mut item = model_base(
                format!("lmstudio:{key}:0"),
                "LM Studio",
                "llama.cpp",
                name,
                "detected",
                "LM Studio /api/v0/models",
                98,
                number(row.get("size_bytes")).max(number(row.get("size"))),
                "LM Studio library",
            );
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
                let mut item = model_base(
                    format!("lmstudio:{key}:{instance_index}"),
                    "LM Studio",
                    "llama.cpp",
                    name,
                    "loaded",
                    "LM Studio /api/v0/models",
                    98,
                    number(row.get("size_bytes")).max(number(row.get("size"))),
                    "LM Studio library",
                );
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
    let mut item = model_base(
        format!("llamacpp:{model}"),
        "llama.cpp",
        "llama.cpp server",
        model.clone(),
        "loaded",
        "llama.cpp /props",
        98,
        0.0,
        "Active llama.cpp server",
    );
    item["installed"] = Value::Bool(false);
    item["format"] = Value::String(model.rsplit('.').next().unwrap_or_default().to_string());
    item["contextLength"] = json!(number(payload.get("n_ctx")));
    vec![item]
}

fn parse_openai_models(payload: &Value, application: &str, source: &str) -> Vec<Value> {
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
            let mut item = model_base(
                format!(
                    "{slug}:{}",
                    if name.is_empty() {
                        index.to_string()
                    } else {
                        name.clone()
                    }
                ),
                application,
                "llama.cpp",
                name,
                if string(row.get("state")) == "not-loaded" {
                    "detected"
                } else {
                    "loaded"
                },
                source,
                92,
                number(row.get("size_bytes")).max(number(row.get("size"))),
                &format!("{application} library"),
            );
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
            let mut item = model_base(
                format!("comfyui:{model}:{index}"),
                "ComfyUI",
                "PyTorch",
                model.clone(),
                "active",
                "ComfyUI active queue",
                98,
                0.0,
                "Active ComfyUI workflow",
            );
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
        .map(PathBuf::from)
        .unwrap_or_default();
    let app_data = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData").join("Roaming"));
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData").join("Local"));
    let mut roots = vec![
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
    name.split(['-', '_', '.'])
        .map(str::to_ascii_uppercase)
        .find(|part| {
            part.starts_with('Q')
                && part[1..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_digit())
                || matches!(part.as_str(), "F16" | "F32" | "BF16")
        })
        .unwrap_or_default()
}

fn infer_parameters(name: &str) -> String {
    name.split(['-', '_', '.'])
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
                let path = entry.path();
                if !path.is_file() {
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
            let mut item = model_base(
                format!("weights:{}:{}", slug(label), slug(&model)),
                label,
                "Transformers runtime",
                model.clone(),
                "detected",
                &format!("{label} weight folder"),
                90,
                size,
                label,
            );
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
            if path.is_dir() && depth < 6 {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if !matches!(name.as_str(), "node_modules" | ".git" | "blobs") {
                    pending.push_back((path, depth + 1));
                }
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
            let mut item = model_base(
                format!("file:{}:{}", slug(label), slug(&model)),
                label,
                "GGUF runtime",
                model.clone(),
                "detected",
                &format!("{label} model folder"),
                95,
                size,
                label,
            );
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
        let found = scan_model_root(&path, &label);
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

    let (ollama_running, ollama_installed, lm_studio, llama_cpp, comfy, jan) =
        thread::scope(|scope| {
            let ollama_running = scope.spawn(|| probe_json("http://127.0.0.1:11434/api/ps"));
            let ollama_installed = scope.spawn(|| probe_json("http://127.0.0.1:11434/api/tags"));
            let lm_studio = scope.spawn(|| probe_json("http://127.0.0.1:1234/api/v0/models"));
            let llama_cpp = scope.spawn(|| probe_json("http://127.0.0.1:8080/props"));
            let comfy = scope.spawn(|| probe_json("http://127.0.0.1:8188/queue"));
            let jan = scope.spawn(|| probe_json("http://127.0.0.1:1337/v1/models"));
            (
                ollama_running.join().unwrap_or(None),
                ollama_installed.join().unwrap_or(None),
                lm_studio.join().unwrap_or(None),
                llama_cpp.join().unwrap_or(None),
                comfy.join().unwrap_or(None),
                jan.join().unwrap_or(None),
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
            .map(|value| parse_openai_models(value, "Jan", "Jan /v1/models"))
            .unwrap_or_default(),
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
    json!({
        "scannedAt": chrono::Utc::now().to_rfc3339(),
        "adapters": [
            { "id": "ollama", "name": "Ollama", "status": if ollama_running.is_some() || ollama_installed.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:11434" },
            { "id": "lmstudio", "name": "LM Studio", "status": if lm_studio.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:1234" },
            { "id": "llamacpp", "name": "llama.cpp", "status": if llama_cpp.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:8080" },
            { "id": "jan", "name": "Jan", "status": if jan.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:1337" },
            { "id": "comfyui", "name": "ComfyUI", "status": if comfy.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:8188" }
        ],
        "modelRoots": model_roots,
        "models": models,
        "applications": applications,
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
    fn product_names_in_shell_text_are_not_services() {
        let process = json!({
            "name": "powershell.exe",
            "path": "C:\\Windows\\System32\\WindowsPowerShell\\powershell.exe",
            "command": "Write-Output 'LocalAI ChatGPT Codex'"
        });
        assert!(detect(&process).is_none());
    }
}
