use crate::alerts::AlertEngine;
use crate::energy::{self, EnergyEstimate};
use crate::local_ai;
use crate::timed_command;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};
use sysinfo::{Disks, Networks, System};

const GIB: f64 = 1_073_741_824.0;
const MIB: f64 = 1_048_576.0;
const SENSOR_SCRIPT: &str = include_str!("../../agent/sensors.ps1");
const GPU_COUNTER_SCRIPT: &str = include_str!("../../agent/gpu-counters.ps1");

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
    sensors: Value,
    local_ai: Value,
    last_gpu: Option<Instant>,
    last_gpu_processes: Option<Instant>,
    last_sensors: Option<Instant>,
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
            sensors: json!({ "cpuTemperature": 0, "cpuSource": null, "storage": [], "hardwareMonitorAvailable": false }),
            local_ai: json!({ "scannedAt": null, "adapters": [], "models": [], "applications": [], "activeModelCount": 0, "loadedModelCount": 0 }),
            last_gpu: None,
            last_gpu_processes: None,
            last_sensors: None,
            last_local_ai: None,
        }
    }

    pub fn set_alert_rule(&mut self, id: &str, enabled: bool) -> bool {
        self.alert_engine.set_enabled(id, enabled)
    }

    pub fn collect(&mut self) -> Value {
        let collection_started = Instant::now();
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_sample).as_secs_f64().max(0.1);
        self.last_sample = now;
        self.system.refresh_all();
        self.networks.refresh(true);
        self.disks.refresh(true);

        if stale(self.last_gpu, now, Duration::from_secs(2)) {
            if let Some(gpu) = read_nvidia() {
                self.gpu = gpu;
            }
            self.last_gpu = Some(now);
        }
        if stale(self.last_gpu_processes, now, Duration::from_secs(3)) {
            self.gpu_processes = read_gpu_processes();
            self.last_gpu_processes = Some(now);
        }
        if stale(self.last_sensors, now, Duration::from_secs(12)) {
            if let Some(sensors) = run_powershell_json(SENSOR_SCRIPT) {
                self.sensors = sensors;
            }
            self.last_sensors = Some(now);
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
        let memory_total = self.system.total_memory() as f64;
        let memory_used = self.system.used_memory() as f64;
        let memory_available = self.system.available_memory() as f64;

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
        let metrics = json!({
            "cpu": energy::round(cpu, 1),
            "gpu": energy::round(self.gpu.utilization, 1),
            "ram": energy::round(if memory_total > 0.0 { memory_used / memory_total * 100.0 } else { 0.0 }, 1),
            "vram": energy::round(if self.gpu.total_bytes > 0.0 { self.gpu.used_bytes / self.gpu.total_bytes * 100.0 } else { 0.0 }, 1),
            "cpuTemp": energy::round(cpu_temp, 1),
            "gpuTemp": energy::round(self.gpu.temperature, 1),
            "ssdTemp": energy::round(ssd_temp, 1),
            "storageTemperatures": storage_temperatures,
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
                "cachedBytes": 0,
                "swapTotalBytes": self.system.total_swap(),
                "swapUsedBytes": self.system.used_swap()
            },
            "gpuMemory": { "totalBytes": self.gpu.total_bytes, "usedBytes": self.gpu.used_bytes }
        });
        let alerts = self.alert_engine.evaluate(&metrics, gaming);
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
                "lastError": null
            }
        })
    }

    fn build_processes(&self, logical_cores: f64, elapsed: f64) -> Vec<Value> {
        self.system.processes().iter().map(|(pid, process)| {
            let id = pid.as_u32();
            let raw_name = process.name().to_string_lossy().to_string();
            let name = if cfg!(windows) && !raw_name.contains('.') && !["System", "Registry", "Idle"].contains(&raw_name.as_str()) { format!("{raw_name}.exe") } else { raw_name };
            let path = process.exe().map(|value| value.to_string_lossy().to_string()).unwrap_or_default();
            let command = process.cmd().iter().map(|value| value.to_string_lossy()).collect::<Vec<_>>().join(" ");
            let haystack = format!("{name} {path} {command}").to_ascii_lowercase();
            let kind = classify(&haystack);
            let gpu = self.gpu_processes.get(&id).and_then(|item| item.get("gpu")).and_then(Value::as_f64).unwrap_or_default().min(100.0);
            let vram_bytes = self.gpu_processes.get(&id).and_then(|item| item.get("dedicatedBytes")).and_then(Value::as_f64).unwrap_or_default();
            let cpu = (process.cpu_usage() as f64 / logical_cores).min(100.0);
            let memory = process.memory() as f64 / GIB;
            let disk = {
                let usage = process.disk_usage();
                (usage.read_bytes + usage.written_bytes) as f64 / elapsed / MIB
            };
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
                "vram": energy::round(vram_bytes / GIB, 2),
                "disk": energy::round(disk, 2),
                "network": 0,
                "power": "Very low",
                "kind": kind,
                "energyWatts": 0,
                "protected": protected,
                "priority": 0,
                "state": format!("{:?}", process.status()),
                "path": path,
                "command": command,
                "handles": 0,
                "threads": 0,
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
        let storage: Vec<Value> = self
            .disks
            .list()
            .iter()
            .map(|disk| {
                json!({
                    "name": disk.name().to_string_lossy(),
                    "type": format!("{:?}", disk.kind()),
                    "sizeBytes": disk.total_space(),
                    "smartStatus": "",
                    "temperature": 0,
                    "temperatureSource": null
                })
            })
            .collect();
        json!({
            "system": { "manufacturer": "", "model": System::host_name().unwrap_or_else(|| "Local computer".to_string()), "version": "" },
            "os": {
                "platform": std::env::consts::OS,
                "distro": System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
                "release": System::os_version().unwrap_or_default(),
                "build": System::kernel_version().unwrap_or_default(),
                "arch": std::env::consts::ARCH,
                "hostname": System::host_name().unwrap_or_default()
            },
            "cpu": {
                "manufacturer": cpu_brand.split_whitespace().next().unwrap_or("Unknown"),
                "brand": cpu_brand,
                "cores": self.system.cpus().len(),
                "physicalCores": physical_cores,
                "speed": speed,
                "speedMax": speed_max,
                "estimatedTdp": tdp
            },
            "gpu": if self.gpu.model.is_empty() { Value::Null } else { json!({
                "vendor": "NVIDIA",
                "model": self.gpu.model,
                "vramBytes": self.gpu.total_bytes,
                "driverVersion": self.gpu.driver,
                "powerLimit": self.gpu.power_limit
            }) },
            "memory": { "totalBytes": self.system.total_memory(), "modules": [] },
            "storage": storage,
            "displays": []
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
    let fields = "name,driver_version,utilization.gpu,memory.total,memory.used,temperature.gpu,power.draw,power.limit";
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
    let row = text
        .lines()
        .next()?
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();
    if row.len() < 8 {
        return None;
    }
    Some(GpuInfo {
        model: row[0].to_string(),
        driver: row[1].to_string(),
        utilization: row[2].parse().unwrap_or_default(),
        total_bytes: row[3].parse::<f64>().unwrap_or_default() * MIB,
        used_bytes: row[4].parse::<f64>().unwrap_or_default() * MIB,
        temperature: row[5].parse().unwrap_or_default(),
        power: row[6].parse().unwrap_or_default(),
        power_limit: row[7].parse().unwrap_or_default(),
    })
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
