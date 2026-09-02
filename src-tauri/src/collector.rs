use crate::alerts::AlertEngine;
use crate::energy::{self, EnergyEstimate};
use crate::gguf;
use crate::local_ai;
use crate::throughput::{self, InferenceThroughput};
use crate::timed_command;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};
use sysinfo::{Disks, Networks, ProcessesToUpdate, System};

const GIB: f64 = 1_073_741_824.0;
const MIB: f64 = 1_048_576.0;
const SENSOR_SCRIPT: &str = include_str!("../../agent/sensors.ps1");
const GPU_COUNTER_SCRIPT: &str = include_str!("../../agent/gpu-counters.ps1");
const PROCESS_SNAPSHOT_SCRIPT: &str = include_str!("../../agent/process-snapshot.ps1");
const MEMORY_SNAPSHOT_SCRIPT: &str = include_str!("../../agent/memory-snapshot.ps1");
const HARDWARE_SNAPSHOT_SCRIPT: &str = include_str!("../../agent/hardware-snapshot.ps1");

#[derive(Clone, Default)]
struct GpuInfo {
    model: String,
    driver: String,
    utilization: f64,
    total_bytes: f64,
    used_bytes: f64,
    temperature: f64,
    power: f64,
    power_limit: f64,
    /// Populated when more than one discrete GPU answers the vendor query.
    pub siblings: Vec<GpuInfo>,
}

pub struct Collector {
    system: System,
    networks: Networks,
    disks: Disks,
    alert_engine: AlertEngine,
    started: Instant,
    last_sample: Instant,
    session_wh: f64,
    gpu: GpuInfo,
    gpu_processes: HashMap<u32, Value>,
    process_details: HashMap<u32, Value>,
    sensors: Value,
    memory_details: Value,
    hardware_details: Value,
    local_ai: Value,
    /// Measured inference throughput (metrics endpoint or runtime log).
    inference: InferenceThroughput,
    /// Exact GGUF metadata keyed by model name, refreshed every 10 minutes.
    gguf_meta: HashMap<String, gguf::GgufMeta>,
    last_gguf: Option<Instant>,
    last_error: Option<String>,
    last_gpu: Option<Instant>,
    last_gpu_processes: Option<Instant>,
    last_process_details: Option<Instant>,
    last_sensors: Option<Instant>,
    last_memory_details: Option<Instant>,
    last_hardware_details: Option<Instant>,
    last_local_ai: Option<Instant>,
}

impl Collector {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            alert_engine: AlertEngine::new(),
            started: Instant::now(),
            last_sample: Instant::now(),
            session_wh: 0.0,
            gpu: GpuInfo::default(),
            gpu_processes: HashMap::new(),
            process_details: HashMap::new(),
            sensors: json!({ "cpuTemperature": 0, "cpuSource": null, "storage": [], "hardwareMonitorAvailable": false }),
            memory_details: json!({}),
            hardware_details: json!({}),
            local_ai: json!({ "scannedAt": null, "adapters": [], "models": [], "applications": [], "activeModelCount": 0, "loadedModelCount": 0 }),
            inference: InferenceThroughput::default(),
            gguf_meta: HashMap::new(),
            last_gguf: None,
            last_error: None,
            last_gpu: None,
            last_gpu_processes: None,
            last_process_details: None,
            last_sensors: None,
            last_memory_details: None,
            last_hardware_details: None,
            last_local_ai: None,
        }
    }

    pub fn set_alert_rule(&mut self, id: &str, enabled: bool) -> bool {
        self.alert_engine.set_enabled(id, enabled)
    }

    /// Refreshes only the process table for action-time validation.
    pub fn refresh_processes_only(&mut self) {
        self.system.refresh_processes(ProcessesToUpdate::All, true);
    }

    /// Read access to the process table for live action validation.
    pub fn system_ref(&self) -> &System {
        &self.system
    }

    pub fn force_refresh(&mut self) {
        self.last_gpu = None;
        self.last_gpu_processes = None;
        self.last_process_details = None;
        self.last_sensors = None;
        self.last_memory_details = None;
        self.last_hardware_details = None;
        self.last_local_ai = None;
        self.last_gguf = None;
    }

    /// Reads GGUF headers for installed Ollama models and caches exact
    /// metadata (parameter count, KV heads, MoE, tensor bytes). Bounded: one
    /// header read per model, refreshed at most every 10 minutes.
    fn probe_model_files(&mut self) {
        self.gguf_meta.clear();
        for (path, model_name) in local_ai::model_probe_targets() {
            if let Some(meta) = gguf::probe(&path) {
                self.gguf_meta.insert(model_name, meta);
            }
        }
    }

    pub fn collect(&mut self) -> Value {
        let collection_started = Instant::now();
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_sample).as_secs_f64().max(0.1);
        self.last_sample = now;
        self.system.refresh_all();
        self.networks.refresh(true);
        self.disks.refresh(true);
        let mut probe_error: Option<String> = None;

        if stale(self.last_gpu, now, Duration::from_secs(2)) {
            match read_nvidia() {
                Some(gpu) => self.gpu = gpu,
                None => {
                    if self.gpu.temperature > 0.0 || self.gpu.utilization > 0.0 {
                        probe_error = Some(
                            "nvidia-smi stopped responding; GPU telemetry is unavailable."
                                .to_string(),
                        );
                    }
                }
            }
            self.last_gpu = Some(now);
        }
        if stale(self.last_gpu_processes, now, Duration::from_secs(3)) {
            self.gpu_processes = read_gpu_processes();
            self.last_gpu_processes = Some(now);
        }
        if stale(self.last_process_details, now, Duration::from_secs(2)) {
            if let Some(details) = run_powershell_json(PROCESS_SNAPSHOT_SCRIPT) {
                self.process_details = details
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|item| {
                        item.get("pid")
                            .and_then(Value::as_u64)
                            .map(|pid| (pid as u32, item))
                    })
                    .collect();
            }
            self.last_process_details = Some(now);
        }
        if stale(self.last_sensors, now, Duration::from_secs(12)) {
            if let Some(sensors) = run_powershell_json(SENSOR_SCRIPT) {
                self.sensors = sensors;
            }
            self.last_sensors = Some(now);
        }
        if stale(self.last_memory_details, now, Duration::from_secs(2)) {
            if let Some(memory) = run_powershell_json(MEMORY_SNAPSHOT_SCRIPT) {
                self.memory_details = memory;
            }
            self.last_memory_details = Some(now);
        }
        if stale(self.last_hardware_details, now, Duration::from_secs(300)) {
            if let Some(hardware) = run_powershell_json(HARDWARE_SNAPSHOT_SCRIPT) {
                self.hardware_details = hardware;
            }
            self.last_hardware_details = Some(now);
        }

        let logical_cores = self.system.cpus().len().max(1) as f64;
        let cpu = self.system.global_cpu_usage() as f64;
        let core_loads: Vec<f64> = self
            .system
            .cpus()
            .iter()
            .map(|core| energy::round(core.cpu_usage() as f64, 1))
            .collect();
        let cpu_brand = self
            .system
            .cpus()
            .first()
            .map(|core| core.brand().trim().to_string())
            .unwrap_or_else(|| "Unknown processor".to_string());
        let cpu_speed = self
            .system
            .cpus()
            .iter()
            .map(|core| core.frequency())
            .sum::<u64>() as f64
            / logical_cores
            / 1000.0;
        let cpu_speed_max = self
            .system
            .cpus()
            .iter()
            .map(|core| core.frequency())
            .max()
            .unwrap_or_default() as f64
            / 1000.0;
        let native_memory_total = number(&self.memory_details, "totalBytes");
        let native_memory_available = number(&self.memory_details, "availableBytes");
        let memory_total = if native_memory_total > 0.0 {
            native_memory_total
        } else {
            self.system.total_memory() as f64
        };
        let memory_available = if native_memory_available > 0.0 {
            native_memory_available
        } else {
            self.system.available_memory() as f64
        };
        let memory_used = (memory_total - memory_available).max(0.0);

        let (received, transmitted) =
            self.networks
                .iter()
                .fold((0_u64, 0_u64), |totals, (_, data)| {
                    (
                        totals.0.saturating_add(data.received()),
                        totals.1.saturating_add(data.transmitted()),
                    )
                });
        let download = received as f64 * 8.0 / elapsed / 1_000_000.0;
        let upload = transmitted as f64 * 8.0 / elapsed / 1_000_000.0;

        let disk_io = self
            .system
            .processes()
            .values()
            .fold((0_u64, 0_u64), |totals, process| {
                let usage = process.disk_usage();
                (
                    totals.0.saturating_add(usage.read_bytes),
                    totals.1.saturating_add(usage.written_bytes),
                )
            });
        let disk_read = disk_io.0 as f64 / elapsed / MIB;
        let disk_write = disk_io.1 as f64 / elapsed / MIB;
        let disk_activity = ((disk_read + disk_write) / 4.0).min(100.0);
        let ssd_temp = self
            .sensors
            .get("storage")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("temperature").and_then(Value::as_f64))
            .fold(0.0_f64, f64::max);
        let cpu_temp = self
            .sensors
            .get("cpuTemperature")
            .and_then(Value::as_f64)
            .unwrap_or_default();

        let physical_cores =
            System::physical_core_count().unwrap_or(self.system.cpus().len().max(1));
        let cpu_tdp = energy::infer_cpu_tdp(physical_cores, &cpu_brand);
        let speed_ratio = if cpu_speed_max > 0.0 {
            cpu_speed / cpu_speed_max
        } else {
            1.0
        };
        let energy_estimate = energy::estimate(energy::EnergyInputs {
            cpu_load: cpu,
            gpu_load: self.gpu.utilization,
            memory_total_gb: memory_total / GIB,
            disk_activity,
            cpu_tdp,
            speed_ratio,
            measured_gpu_power: self.gpu.power,
            gpu_power_limit: if self.gpu.power_limit > 0.0 {
                self.gpu.power_limit
            } else {
                220.0
            },
        });
        self.session_wh += energy_estimate.watts * elapsed / 3600.0;

        let mut processes = self.build_processes(logical_cores, elapsed);
        attribute_energy(
            &mut processes,
            &energy_estimate,
            cpu,
            self.gpu.utilization,
            memory_used / GIB,
        );
        processes.sort_by(|left, right| {
            number(right, "energyWatts")
                .total_cmp(&number(left, "energyWatts"))
                .then_with(|| number(right, "cpu").total_cmp(&number(left, "cpu")))
        });
        processes.truncate(180);

        if stale(self.last_local_ai, now, Duration::from_secs(4)) {
            self.local_ai = local_ai::build(&processes);
            if stale(self.last_gguf, now, Duration::from_secs(600)) {
                self.probe_model_files();
                self.last_gguf = Some(now);
            }
            if !self.gguf_meta.is_empty() {
                local_ai::apply_gguf_metadata(&mut self.local_ai, &self.gguf_meta);
            }
            self.last_local_ai = Some(now);
        }
        let gaming = processes
            .iter()
            .find(|process| {
                process.get("kind").and_then(Value::as_str) == Some("Game")
                    && (number(process, "gpu") >= 20.0 || number(process, "cpu") >= 10.0)
            })
            .map(|process| {
                (
                    string(process, "name"),
                    process
                        .get("id")
                        .and_then(Value::as_u64)
                        .unwrap_or_default() as u32,
                )
            });

        let storage_temperatures = self
            .sensors
            .get("storage")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let mut temperature_readings = self
            .sensors
            .get("readings")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if self.gpu.temperature > 0.0 {
            temperature_readings.push(json!({
                "component": "GPU",
                "name": "GPU Core",
                "value": energy::round(self.gpu.temperature, 1),
                "min": null,
                "max": null,
                "source": "nvidia-smi",
                "accuracy": "graphics-driver"
            }));
        }
        let mut hardware_sensors = self
            .sensors
            .get("sensors")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        hardware_sensors.push(sensor_row(
            "CPU",
            "CPU Total",
            "Load",
            cpu,
            "%",
            "sysinfo",
            "os-counter",
        ));
        hardware_sensors.push(sensor_row(
            "CPU",
            "Average clock",
            "Clock",
            cpu_speed,
            "GHz",
            "sysinfo",
            "os-counter",
        ));
        for (index, load) in core_loads.iter().enumerate() {
            hardware_sensors.push(sensor_row(
                "CPU",
                &format!("Logical CPU {index}"),
                "Load",
                *load,
                "%",
                "sysinfo",
                "os-counter",
            ));
        }
        hardware_sensors.push(sensor_row(
            "Memory",
            "Physical memory",
            "Load",
            if memory_total > 0.0 {
                memory_used / memory_total * 100.0
            } else {
                0.0
            },
            "%",
            "sysinfo",
            "os-counter",
        ));
        hardware_sensors.push(sensor_row(
            "Storage",
            "Disk activity",
            "Load",
            disk_activity,
            "%",
            "sysinfo",
            "os-counter",
        ));
        hardware_sensors.push(sensor_row(
            "Storage",
            "Disk read",
            "Throughput",
            disk_read,
            "MB/s",
            "sysinfo",
            "os-counter",
        ));
        hardware_sensors.push(sensor_row(
            "Storage",
            "Disk write",
            "Throughput",
            disk_write,
            "MB/s",
            "sysinfo",
            "os-counter",
        ));
        hardware_sensors.push(sensor_row(
            "Network",
            "Download",
            "Throughput",
            download,
            "Mbps",
            "sysinfo",
            "os-counter",
        ));
        hardware_sensors.push(sensor_row(
            "Network",
            "Upload",
            "Throughput",
            upload,
            "Mbps",
            "sysinfo",
            "os-counter",
        ));
        if !self.gpu.model.is_empty() {
            hardware_sensors.push(sensor_row(
                "GPU",
                "GPU Core",
                "Load",
                self.gpu.utilization,
                "%",
                "nvidia-smi",
                "graphics-driver",
            ));
            hardware_sensors.push(sensor_row(
                "GPU",
                "GPU memory",
                "Load",
                if self.gpu.total_bytes > 0.0 {
                    self.gpu.used_bytes / self.gpu.total_bytes * 100.0
                } else {
                    0.0
                },
                "%",
                "nvidia-smi",
                "graphics-driver",
            ));
            hardware_sensors.push(sensor_row(
                "GPU",
                "Board power",
                "Power",
                energy_estimate.gpu,
                "W",
                if self.gpu.power > 0.0 {
                    "nvidia-smi"
                } else {
                    "VISOR Energy Lens"
                },
                if self.gpu.power > 0.0 {
                    "graphics-driver"
                } else {
                    "modeled-estimate"
                },
            ));
            for (index, sibling) in self.gpu.siblings.iter().enumerate() {
                hardware_sensors.push(sensor_row(
                    "GPU",
                    &format!("GPU {index} utilization"),
                    "Load",
                    sibling.utilization,
                    "%",
                    "nvidia-smi",
                    "graphics-driver",
                ));
                hardware_sensors.push(sensor_row(
                    "GPU",
                    &format!("GPU {index} memory"),
                    "Load",
                    if sibling.total_bytes > 0.0 {
                        sibling.used_bytes / sibling.total_bytes * 100.0
                    } else {
                        0.0
                    },
                    "%",
                    "nvidia-smi",
                    "graphics-driver",
                ));
            }
        }
        let metrics = json!({
            "cpu": energy::round(cpu, 1),
            "gpu": energy::round(self.gpu.utilization, 1),
            "ram": energy::round(if memory_total > 0.0 { memory_used / memory_total * 100.0 } else { 0.0 }, 1),
            "vram": energy::round(if self.gpu.total_bytes > 0.0 { self.gpu.used_bytes / self.gpu.total_bytes * 100.0 } else { 0.0 }, 1),
            "cpuTemp": energy::round(cpu_temp, 1),
            "gpuTemp": energy::round(self.gpu.temperature, 1),
            "ssdTemp": energy::round(ssd_temp, 1),
            "storageTemperatures": storage_temperatures,
            "temperatureReadings": temperature_readings,
            "hardwareSensors": hardware_sensors,
            "sensorGuidance": self.sensors.get("cpuSensorGuidance").cloned().unwrap_or(Value::Null),
            "throughput": throughput::as_json(&self.inference),
            "sensorSources": {
                "cpu": self.sensors.get("cpuSource").cloned().unwrap_or(Value::Null),
                "gpu": if self.gpu.temperature > 0.0 { json!("nvidia-smi") } else { Value::Null },
                "storage": self.sensors.get("storage").and_then(Value::as_array).map(|items| items.iter().filter_map(|item| item.get("source").and_then(Value::as_str)).collect::<Vec<_>>()).unwrap_or_default()
            },
            "cpuPower": energy::round(energy_estimate.cpu, 1),
            "gpuPower": energy::round(energy_estimate.gpu, 1),
            "download": energy::round(download, 2),
            "upload": energy::round(upload, 2),
            "diskRead": energy::round(disk_read, 2),
            "diskWrite": energy::round(disk_write, 2),
            "cpuSpeedGhz": energy::round(cpu_speed, 2),
            "cpuCoreLoads": core_loads,
            "diskActivity": energy::round(disk_activity, 1),
            "memory": {
                "totalBytes": memory_total,
                "usedBytes": memory_used,
                "availableBytes": memory_available,
                "cachedBytes": number(&self.memory_details, "cachedBytes"),
                "committedBytes": number(&self.memory_details, "committedBytes"),
                "commitLimitBytes": number(&self.memory_details, "commitLimitBytes"),
                "pagedPoolBytes": number(&self.memory_details, "pagedPoolBytes"),
                "nonPagedPoolBytes": number(&self.memory_details, "nonPagedPoolBytes"),
                "pagesPerSecond": number(&self.memory_details, "pagesPerSecond"),
                "source": if native_memory_total > 0.0 { self.memory_details.get("source").cloned().unwrap_or_else(|| json!("Windows memory manager")) } else { json!("sysinfo") },
                "swapTotalBytes": self.system.total_swap(),
                "swapUsedBytes": self.system.used_swap()
            },
            "gpuMemory": { "totalBytes": self.gpu.total_bytes, "usedBytes": self.gpu.used_bytes },
            "gpuCount": 1 + self.gpu.siblings.len(),
            "gpus": {
                "primary": {
                    "model": self.gpu.model,
                    "utilization": energy::round(self.gpu.utilization, 1),
                    "totalBytes": self.gpu.total_bytes,
                    "usedBytes": self.gpu.used_bytes,
                    "temperature": energy::round(self.gpu.temperature, 1),
                    "power": energy::round(self.gpu.power, 1),
                    "powerLimit": energy::round(self.gpu.power_limit, 1)
                },
                "siblings": self.gpu.siblings.iter().map(|gpu| json!({
                    "model": gpu.model,
                    "utilization": energy::round(gpu.utilization, 1),
                    "totalBytes": gpu.total_bytes,
                    "usedBytes": gpu.used_bytes,
                    "temperature": energy::round(gpu.temperature, 1),
                    "power": energy::round(gpu.power, 1),
                    "powerLimit": energy::round(gpu.power_limit, 1)
                })).collect::<Vec<_>>()
            }
        });
        let alerts = self.alert_engine.evaluate(&metrics, gaming);
        self.inference = throughput::measure();
        let current_pid = std::process::id();
        let current_process = processes
            .iter()
            .find(|process| process.get("id").and_then(Value::as_u64) == Some(current_pid as u64));
        let blocked = self
            .system
            .processes()
            .values()
            .filter(|process| format!("{:?}", process.status()).eq_ignore_ascii_case("Stop"))
            .count();
        self.last_error = probe_error;

        json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "source": "tauri-native",
            "pollMs": 1100,
            "hardware": self.hardware(&cpu_brand, physical_cores, cpu_speed, cpu_speed_max, cpu_tdp),
            "metrics": metrics,
            "energy": energy::as_json(&energy_estimate, self.session_wh),
            "processes": processes,
            "processCounts": {
                "all": self.system.processes().len(),
                "running": self.system.processes().len().saturating_sub(blocked),
                "blocked": blocked,
                "sleeping": 0
            },
            "localAI": self.local_ai,
            "alerts": alerts,
            "agent": {
                "cpuPercent": current_process.map(|process| number(process, "cpu")).unwrap_or_default(),
                "memoryMb": current_process.map(|process| number(process, "memory") * 1024.0).unwrap_or_default(),
                "pid": current_pid,
                "uptimeSeconds": self.started.elapsed().as_secs(),
                "sampleDurationMs": collection_started.elapsed().as_millis(),
                "gpuAttributionAvailable": !self.gpu_processes.is_empty(),
                "sensorAttributionAvailable": cpu_temp > 0.0 || ssd_temp > 0.0,
                "lastError": self.last_error.clone()
            }
        })
    }

    fn build_processes(&self, logical_cores: f64, elapsed: f64) -> Vec<Value> {
        self.system.processes().iter().map(|(pid, process)| {
            let id = pid.as_u32();
            let details = self.process_details.get(&id);
            let raw_name = process.name().to_string_lossy().to_string();
            let name = if cfg!(windows) && !raw_name.contains('.') && !["System", "Registry", "Idle"].contains(&raw_name.as_str()) { format!("{raw_name}.exe") } else { raw_name };
            let path = process.exe().map(|value| value.to_string_lossy().to_string()).unwrap_or_default();
            let command = process.cmd().iter().map(|value| value.to_string_lossy()).collect::<Vec<_>>().join(" ");
            let haystack = format!("{name} {path} {command}").to_ascii_lowercase();
            let kind = classify(&haystack);
            let gpu = self.gpu_processes.get(&id).and_then(|item| item.get("gpu")).and_then(Value::as_f64).unwrap_or_default().min(100.0);
            let vram_bytes = self.gpu_processes.get(&id).and_then(|item| item.get("dedicatedBytes")).and_then(Value::as_f64).unwrap_or_default();
            let cpu = (process.cpu_usage() as f64 / logical_cores).min(100.0);
            let working_set_bytes = details
                .map(|item| number(item, "workingSetBytes"))
                .filter(|value| *value > 0.0)
                .unwrap_or(process.memory() as f64);
            let private_bytes = details
                .map(|item| number(item, "privateBytes"))
                .unwrap_or_default();
            let virtual_bytes = process.virtual_memory() as f64;
            let memory = working_set_bytes / GIB;
            let usage = process.disk_usage();
            let disk_read = usage.read_bytes as f64 / elapsed / MIB;
            let disk_write = usage.written_bytes as f64 / elapsed / MIB;
            let disk = disk_read + disk_write;
            let protected = is_protected(id, &name);
            json!({
                "id": id,
                "parentId": process.parent().map(|value| value.as_u32()).unwrap_or_default(),
                "name": name,
                "subtitle": if let Some(value) = kind { value } else if path.is_empty() { "System process" } else { "Desktop application" },
                "icon": process.name().to_string_lossy().chars().next().unwrap_or('?').to_ascii_uppercase().to_string(),
                "color": color(id),
                "cpu": energy::round(cpu, 1),
                "gpu": energy::round(gpu, 1),
                "memory": energy::round(memory, 2),
                "memoryPercent": energy::round(if self.system.total_memory() > 0 { working_set_bytes / self.system.total_memory() as f64 * 100.0 } else { 0.0 }, 2),
                "workingSetBytes": working_set_bytes,
                "privateBytes": private_bytes,
                "virtualBytes": virtual_bytes,
                "vram": energy::round(vram_bytes / GIB, 2),
                "disk": energy::round(disk, 2),
                "diskRead": energy::round(disk_read, 2),
                "diskWrite": energy::round(disk_write, 2),
                "diskReadTotalBytes": usage.total_read_bytes,
                "diskWriteTotalBytes": usage.total_written_bytes,
                "network": 0,
                "power": "Very low",
                "kind": kind,
                "energyWatts": 0,
                "protected": protected,
                "priority": details.map(|item| number(item, "priority") as i64).unwrap_or_default(),
                "state": format!("{:?}", process.status()),
                "path": path,
                "command": command,
                "handles": details.map(|item| number(item, "handles") as u64).unwrap_or_default(),
                "threads": details.map(|item| number(item, "threads") as u64).unwrap_or_default(),
                "startedAt": details.and_then(|item| item.get("started")).cloned().unwrap_or(Value::Null),
                "uptimeSeconds": process.run_time(),
                "responding": details.and_then(|item| item.get("responding")).and_then(Value::as_bool),
                "gpuEngines": self.gpu_processes.get(&id).and_then(|item| item.get("engines")).cloned().unwrap_or_else(|| json!({}))
            })
        }).collect()
    }

    fn hardware(
        &self,
        cpu_brand: &str,
        physical_cores: usize,
        speed: f64,
        speed_max: f64,
        tdp: f64,
    ) -> Value {
        let fallback_storage = self
            .disks
            .list()
            .iter()
            .map(|disk| {
                json!({
                    "name": disk.name().to_string_lossy(),
                    "type": format!("{:?}", disk.kind()),
                    "sizeBytes": disk.total_space(),
                    "smartStatus": "",
                    "busType": "",
                    "firmware": "",
                    "partitions": 0,
                    "temperature": 0,
                    "temperatureSource": null
                })
            })
            .collect::<Vec<_>>();
        let mut storage = array_or(&self.hardware_details, "storage", fallback_storage);
        let temperature_sensors = self
            .sensors
            .get("storage")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for (index, disk) in storage.iter_mut().enumerate() {
            let disk_name = string(disk, "name").to_ascii_lowercase();
            let sensor = temperature_sensors
                .iter()
                .find(|sensor| {
                    let sensor_name = string(sensor, "name").to_ascii_lowercase();
                    !sensor_name.is_empty()
                        && !disk_name.is_empty()
                        && (sensor_name.contains(&disk_name) || disk_name.contains(&sensor_name))
                })
                .or_else(|| temperature_sensors.get(index));
            if let Some(sensor) = sensor {
                disk["temperature"] = json!(number(sensor, "temperature"));
                disk["temperatureSource"] = sensor.get("source").cloned().unwrap_or(Value::Null);
            }
        }

        let details_cpu = self.hardware_details.get("cpu").unwrap_or(&Value::Null);
        let details_memory = self.hardware_details.get("memory").unwrap_or(&Value::Null);
        let detail_gpus = array_or(&self.hardware_details, "gpus", Vec::new());
        let mut primary_gpu = detail_gpus.first().cloned().unwrap_or(Value::Null);
        if !self.gpu.model.is_empty() {
            let vendor = if self.gpu.model.to_ascii_lowercase().contains("amd") {
                "AMD"
            } else if self.gpu.model.to_ascii_lowercase().contains("intel") {
                "Intel"
            } else {
                "NVIDIA"
            };
            primary_gpu = json!({
                "vendor": vendor,
                "model": self.gpu.model,
                "vramBytes": self.gpu.total_bytes,
                "driverVersion": self.gpu.driver,
                "driverDate": "",
                "videoMode": "",
                "resolution": "",
                "refreshRate": 0,
                "status": "OK",
                "powerLimit": self.gpu.power_limit
            });
        }
        let mut all_gpus = Vec::with_capacity(1 + self.gpu.siblings.len());
        if !self.gpu.model.is_empty() {
            all_gpus.push(json!({
                "vendor": "NVIDIA",
                "model": self.gpu.model,
                "vramBytes": self.gpu.total_bytes,
                "driverVersion": self.gpu.driver,
                "status": "OK",
                "powerLimit": self.gpu.power_limit
            }));
            for sibling in &self.gpu.siblings {
                all_gpus.push(json!({
                    "vendor": "NVIDIA",
                    "model": sibling.model,
                    "vramBytes": sibling.total_bytes,
                    "driverVersion": sibling.driver,
                    "status": "OK",
                    "powerLimit": sibling.power_limit
                }));
            }
        }
        if all_gpus.is_empty() {
            all_gpus = detail_gpus.clone();
        }
        let fallback_networks = self
            .networks.keys().map(|name| {
                json!({ "name": name, "manufacturer": "", "type": "Network interface", "speedBits": 0, "connection": name, "status": "Connected" })
            })
            .collect::<Vec<_>>();

        let system_details = self
            .hardware_details
            .get("system")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let os_details = self
            .hardware_details
            .get("os")
            .cloned()
            .unwrap_or_else(|| json!({}));
        json!({
            "collectedAt": self.hardware_details.get("collectedAt").cloned().unwrap_or(Value::Null),
            "system": {
                "manufacturer": text_or(&system_details, "manufacturer", "Unknown"),
                "model": text_or(&system_details, "model", &System::host_name().unwrap_or_else(|| "Local computer".to_string())),
                "version": string(&system_details, "version")
            },
            "motherboard": self.hardware_details.get("motherboard").cloned().unwrap_or_else(|| json!({ "manufacturer": "", "model": "", "version": "" })),
            "bios": self.hardware_details.get("bios").cloned().unwrap_or_else(|| json!({ "vendor": "", "version": "", "date": "", "smbiosVersion": "" })),
            "os": {
                "platform": text_or(&os_details, "platform", std::env::consts::OS),
                "distro": text_or(&os_details, "distro", &System::name().unwrap_or_else(|| std::env::consts::OS.to_string())),
                "release": text_or(&os_details, "release", &System::os_version().unwrap_or_default()),
                "build": text_or(&os_details, "build", &System::kernel_version().unwrap_or_default()),
                "arch": text_or(&os_details, "arch", std::env::consts::ARCH),
                "hostname": text_or(&os_details, "hostname", &System::host_name().unwrap_or_default())
            },
            "cpu": {
                "manufacturer": text_or(details_cpu, "manufacturer", cpu_brand.split_whitespace().next().unwrap_or("Unknown")),
                "brand": text_or(details_cpu, "brand", cpu_brand),
                "cores": positive_or(number(details_cpu, "cores"), self.system.cpus().len() as f64),
                "physicalCores": positive_or(number(details_cpu, "physicalCores"), physical_cores as f64),
                "socket": string(details_cpu, "socket"),
                "speed": positive_or(number(details_cpu, "speed"), speed),
                "speedMax": positive_or(number(details_cpu, "speedMax"), speed_max),
                "l2CacheBytes": number(details_cpu, "l2CacheBytes"),
                "l3CacheBytes": number(details_cpu, "l3CacheBytes"),
                "virtualization": details_cpu.get("virtualization").and_then(Value::as_bool).unwrap_or(false),
                "estimatedTdp": tdp
            },
            "gpu": primary_gpu,
            "gpus": all_gpus,
            "memory": {
                "totalBytes": positive_or(number(details_memory, "totalBytes"), self.system.total_memory() as f64),
                "modules": details_memory.get("modules").and_then(Value::as_array).cloned().unwrap_or_default()
            },
            "storage": storage,
            "networks": array_or(&self.hardware_details, "networks", fallback_networks),
            "displays": array_or(&self.hardware_details, "displays", Vec::new())
        })
    }
}

fn stale(last: Option<Instant>, now: Instant, interval: Duration) -> bool {
    last.map(|last| now.duration_since(last) >= interval)
        .unwrap_or(true)
}

fn number(value: &Value, field: &str) -> f64 {
    value.get(field).and_then(Value::as_f64).unwrap_or_default()
}

fn string(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn text_or(value: &Value, field: &str, fallback: &str) -> String {
    let current = string(value, field);
    if current.trim().is_empty() {
        fallback.to_string()
    } else {
        current
    }
}

fn positive_or(value: f64, fallback: f64) -> f64 {
    if value > 0.0 {
        value
    } else {
        fallback
    }
}

fn array_or(value: &Value, field: &str, fallback: Vec<Value>) -> Vec<Value> {
    value
        .get(field)
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .cloned()
        .unwrap_or(fallback)
}

fn sensor_row(
    component: &str,
    name: &str,
    sensor_type: &str,
    value: f64,
    unit: &str,
    source: &str,
    accuracy: &str,
) -> Value {
    json!({
        "component": component,
        "name": name,
        "sensorType": sensor_type,
        "value": energy::round(value, 2),
        "min": null,
        "max": null,
        "unit": unit,
        "source": source,
        "accuracy": accuracy
    })
}

fn color(pid: u32) -> &'static str {
    ["#58d9ff", "#9b8cff", "#63e6be", "#ffbe5c", "#ff758f"][pid as usize % 5]
}

fn classify(haystack: &str) -> Option<&'static str> {
    if [
        "ollama",
        "lm studio",
        "lmstudio",
        "llama",
        "kobold",
        "comfyui",
        "stable-diffusion",
        "fooocus",
        "invokeai",
    ]
    .iter()
    .any(|item| haystack.contains(item))
    {
        Some("AI")
    } else if [
        "steamapps",
        "epic games",
        "gog galaxy",
        "riot games",
        "cyberpunk",
        "eldenring",
        "valorant",
        "fortnite",
        "overwatch",
        "helldivers",
        "witcher3",
        "rdr2.exe",
    ]
    .iter()
    .any(|item| haystack.contains(item))
    {
        Some("Game")
    } else if [
        "photoshop",
        "afterfx",
        "premiere",
        "blender",
        "davinci",
        "resolve",
        "illustrator",
    ]
    .iter()
    .any(|item| haystack.contains(item))
    {
        Some("Creative")
    } else {
        None
    }
}

pub fn is_protected(pid: u32, name: &str) -> bool {
    if pid <= 4 || pid == std::process::id() {
        return true;
    }
    matches!(
        name.trim_end_matches(".exe").to_ascii_lowercase().as_str(),
        "system"
            | "system idle process"
            | "registry"
            | "memory compression"
            | "smss"
            | "csrss"
            | "wininit"
            | "services"
            | "lsass"
            | "winlogon"
            | "svchost"
            | "fontdrvhost"
            | "launchd"
            | "kernel_task"
            | "windowserver"
            | "loginwindow"
            | "systemd"
            | "systemd-journald"
            | "systemd-logind"
            | "dbus-daemon"
            | "sshd"
    )
}

fn attribute_energy(
    processes: &mut [Value],
    estimate: &EnergyEstimate,
    system_cpu: f64,
    system_gpu: f64,
    used_memory_gb: f64,
) {
    let observed_cpu = processes
        .iter()
        .map(|process| number(process, "cpu"))
        .sum::<f64>();
    let observed_gpu = processes
        .iter()
        .map(|process| number(process, "gpu"))
        .sum::<f64>();
    let total_cpu = observed_cpu.max(system_cpu).max(0.1);
    let total_gpu = observed_gpu.max(system_gpu).max(0.1);
    for process in processes {
        let watts = estimate.cpu * number(process, "cpu") / total_cpu
            + estimate.gpu * number(process, "gpu") / total_gpu
            + estimate.memory
                * (number(process, "memory") / used_memory_gb.max(0.25)).clamp(0.0, 1.0);
        process["energyWatts"] = json!(energy::round(watts, 1));
        process["power"] = Value::String(
            if watts >= 30.0 {
                "High"
            } else if watts >= 10.0 {
                "Moderate"
            } else if watts >= 2.0 {
                "Low"
            } else {
                "Very low"
            }
            .to_string(),
        );
    }
}

fn read_nvidia() -> Option<GpuInfo> {
    let fields = "index,name,driver_version,utilization.gpu,memory.total,memory.used,temperature.gpu,power.draw,power.limit";
    let output = timed_command::output(
        hidden_command(if cfg!(windows) {
            "nvidia-smi.exe"
        } else {
            "nvidia-smi"
        })
        .args([
            format!("--query-gpu={fields}"),
            "--format=csv,noheader,nounits".to_string(),
        ]),
        None,
        Duration::from_secs(3),
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut gpus: Vec<GpuInfo> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let row = line.split(',').map(str::trim).collect::<Vec<_>>();
            if row.len() < 9 {
                return GpuInfo::default();
            }
            GpuInfo {
                model: row[1].to_string(),
                driver: row[2].to_string(),
                utilization: row[3].parse().unwrap_or_default(),
                total_bytes: row[4].parse::<f64>().unwrap_or_default() * MIB,
                used_bytes: row[5].parse::<f64>().unwrap_or_default() * MIB,
                temperature: row[6].parse().unwrap_or_default(),
                power: row[7].parse().unwrap_or_default(),
                power_limit: row[8].parse().unwrap_or_default(),
                siblings: Vec::new(),
            }
        })
        .collect();
    if gpus.iter().any(|gpu| gpu.model.is_empty()) || gpus.is_empty() {
        return None;
    }
    // Prefer the adapter with real memory use as the primary; expose the rest.
    gpus.sort_by(|left, right| {
        right
            .used_bytes
            .total_cmp(&left.used_bytes)
            .then_with(|| right.total_bytes.total_cmp(&left.total_bytes))
    });
    let mut primary = gpus.remove(0);
    primary.siblings = gpus;
    Some(primary)
}

#[cfg(windows)]
fn read_gpu_processes() -> HashMap<u32, Value> {
    run_powershell_json(GPU_COUNTER_SCRIPT)
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            item.get("pid")
                .and_then(Value::as_u64)
                .map(|pid| (pid as u32, item))
        })
        .collect()
}

#[cfg(not(windows))]
fn read_gpu_processes() -> HashMap<u32, Value> {
    HashMap::new()
}

#[cfg(windows)]
fn run_powershell_json(script: &str) -> Option<Value> {
    let output = timed_command::output(
        hidden_command("powershell.exe").args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "-",
        ]),
        Some(script.as_bytes()),
        Duration::from_secs(8),
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

#[cfg(not(windows))]
fn run_powershell_json(_script: &str) -> Option<Value> {
    None
}

fn hidden_command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_critical_system_processes() {
        assert!(is_protected(4, "System"));
        assert!(is_protected(200, "lsass.exe"));
        assert!(is_protected(200, "launchd"));
        assert!(is_protected(200, "systemd-journald"));
        assert!(!is_protected(20_000, "notepad.exe"));
    }

    #[test]
    fn detects_workload_categories() {
        assert_eq!(
            classify("c:\\program files\\ollama\\ollama.exe"),
            Some("AI")
        );
        assert_eq!(classify("d:\\steamapps\\common\\game.exe"), Some("Game"));
        assert_eq!(
            classify("c:\\program files\\blender\\blender.exe"),
            Some("Creative")
        );
    }
}
