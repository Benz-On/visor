use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

const GIB: f64 = 1_073_741_824.0;

#[derive(Clone)]
struct Identity {
    application: &'static str,
    runtime: &'static str,
    role: &'static str,
}

fn detect(process: &Value) -> Option<Identity> {
    let name = process
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path = process
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let haystack = format!("{name} {path}");
    if haystack.contains("ollama") {
        let role = if haystack.contains("runner") || haystack.contains("llama-server") {
            "model-runner"
        } else {
            "application"
        };
        return Some(Identity {
            application: "Ollama",
            runtime: "Ollama engine",
            role,
        });
    }
    if haystack.contains("comfyui") || haystack.contains("comfy-desktop") {
        return Some(Identity {
            application: "ComfyUI",
            runtime: "PyTorch",
            role: if name.contains("python") {
                "model-runner"
            } else {
                "application"
            },
        });
    }
    if haystack.contains("lm studio") || haystack.contains("lmstudio") {
        return Some(Identity {
            application: "LM Studio",
            runtime: "llama.cpp",
            role: if haystack.contains("server") || haystack.contains("llama") {
                "model-runner"
            } else {
                "application"
            },
        });
    }
    if haystack.contains("llama-server")
        || haystack.contains("llama.cpp")
        || haystack.contains("kobold")
    {
        return Some(Identity {
            application: if haystack.contains("kobold") {
                "KoboldCpp"
            } else {
                "llama.cpp"
            },
            runtime: "llama.cpp",
            role: "model-runner",
        });
    }
    if haystack.contains("stable-diffusion")
        || haystack.contains("stable_diffusion")
        || haystack.contains("fooocus")
        || haystack.contains("invokeai")
    {
        return Some(Identity {
            application: if haystack.contains("fooocus") {
                "Fooocus"
            } else if haystack.contains("invokeai") {
                "InvokeAI"
            } else {
                "Stable Diffusion"
            },
            runtime: "PyTorch",
            role: "model-runner",
        });
    }
    None
}

fn probe_json(url: &str) -> Option<Value> {
    ureq::get(url)
        .timeout(Duration::from_millis(350))
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

fn model_base(
    id: String,
    application: &str,
    runtime: &str,
    model: String,
    source: &str,
    confidence: u64,
) -> Value {
    json!({
        "id": id,
        "application": application,
        "runtime": runtime,
        "model": model,
        "status": "loaded",
        "source": source,
        "confidence": confidence,
        "family": "",
        "parameters": "",
        "quantization": "",
        "format": "",
        "contextLength": 0,
        "allocatedBytes": 0,
        "allocatedVramBytes": 0,
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
                "Ollama /api/ps",
                100,
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
        for (instance_index, instance) in instances.into_iter().flatten().enumerate() {
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
                "LM Studio local API",
                96,
            );
            item["family"] = Value::String(string(row.get("architecture")));
            item["parameters"] = Value::String(string(row.get("params_string")));
            item["quantization"] = Value::String(string(row.get("quantization")));
            item["format"] = Value::String(string(row.get("format")));
            item["contextLength"] = json!(number(instance.get("context_length")));
            item["allocatedBytes"] = json!(number(row.get("size_bytes")));
            item["allocatedVramBytes"] = json!(number(instance.get("vram_bytes")));
            models.push(item);
        }
    }
    models
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
        "llama.cpp /props",
        98,
    );
    item["format"] = Value::String(model.rsplit('.').next().unwrap_or_default().to_string());
    item["contextLength"] = json!(number(payload.get("n_ctx")));
    vec![item]
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
                "ComfyUI active queue",
                98,
            );
            item["status"] = Value::String("active".to_string());
            item["format"] =
                Value::String(model.rsplit('.').next().unwrap_or_default().to_string());
            item
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
                "id": key.to_ascii_lowercase().replace([' ', '.', ':'], "-"),
                "application": identity.application,
                "runtime": identity.runtime,
                "processes": [],
                "cpu": 0.0,
                "gpu": 0.0,
                "memoryGb": 0.0,
                "vramGb": 0.0,
                "energyWatts": 0.0,
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
    let applications: Vec<Value> = groups.into_values().collect();

    let ollama = probe_json("http://127.0.0.1:11434/api/ps");
    let lm_studio = probe_json("http://127.0.0.1:1234/api/v0/models");
    let llama_cpp = probe_json("http://127.0.0.1:8080/props");
    let comfy = probe_json("http://127.0.0.1:8188/queue");
    let mut models = Vec::new();
    if let Some(value) = &ollama {
        models.extend(parse_ollama(value));
    }
    if let Some(value) = &lm_studio {
        models.extend(parse_lm_studio(value));
    }
    if let Some(value) = &llama_cpp {
        models.extend(parse_llama_cpp(value));
    }
    if let Some(value) = &comfy {
        models.extend(parse_comfy(value));
    }

    for model in &mut models {
        let application_name = string(model.get("application"));
        let application = applications
            .iter()
            .find(|item| string(item.get("application")) == application_name);
        let active = application
            .and_then(|item| item.get("active"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if active {
            model["status"] = Value::String("active".to_string());
        }
        let runner = application
            .and_then(|item| item.get("processes"))
            .and_then(Value::as_array)
            .and_then(|items| {
                items.iter().max_by(|left, right| {
                    number(left.get("energyWatts")).total_cmp(&number(right.get("energyWatts")))
                })
            })
            .cloned();
        let allocated = number(model.get("allocatedBytes"));
        let vram = number(model.get("allocatedVramBytes"));
        model["process"] = runner.unwrap_or(Value::Null);
        model["applicationEnergyWatts"] = application
            .map(|item| json!(number(item.get("energyWatts"))))
            .unwrap_or(Value::Null);
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
    let loaded_count = models.len();
    json!({
        "scannedAt": chrono::Utc::now().to_rfc3339(),
        "adapters": [
            { "id": "ollama", "name": "Ollama", "status": if ollama.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:11434" },
            { "id": "lmstudio", "name": "LM Studio", "status": if lm_studio.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:1234" },
            { "id": "llamacpp", "name": "llama.cpp", "status": if llama_cpp.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:8080" },
            { "id": "comfyui", "name": "ComfyUI", "status": if comfy.is_some() { "online" } else { "offline" }, "endpoint": "127.0.0.1:8188" }
        ],
        "models": models,
        "applications": applications,
        "activeModelCount": active_count,
        "loadedModelCount": loaded_count
    })
}
