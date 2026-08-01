import { createServer } from 'node:http';
import { execFile } from 'node:child_process';
import { cpus, hostname, platform, release, totalmem, freemem, setPriority, constants as osConstants } from 'node:os';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import si from 'systeminformation';
import { createAlertEngine } from './alert-engine.mjs';
import { attributeProcessEnergy, estimateEnergy, inferCpuTdp, projectEnergy } from './energy-model.mjs';
import { buildLocalAiSnapshot, detectAiApplication, discoverLocalModels } from './local-ai.mjs';

const execFileAsync = promisify(execFile);
const GiB = 1024 ** 3;
const MiB = 1024 ** 2;
const PORT = Number.parseInt(process.env.VISOR_AGENT_PORT || '1421', 10);
const POLL_MS = Math.min(5000, Math.max(650, Number.parseInt(process.env.VISOR_POLL_MS || '1000', 10)));
const GPU_COUNTER_INTERVAL_MS = 5000;
const SENSOR_INTERVAL_MS = 5000;
const LOCAL_AI_INTERVAL_MS = 2500;
const GPU_COUNTER_SCRIPT = fileURLToPath(new URL('./gpu-counters.ps1', import.meta.url));
const PROCESS_SNAPSHOT_SCRIPT = fileURLToPath(new URL('./process-snapshot.ps1', import.meta.url));
const SENSOR_SCRIPT = fileURLToPath(new URL('./sensors.ps1', import.meta.url));
const MEMORY_SCRIPT = fileURLToPath(new URL('./memory-snapshot.ps1', import.meta.url));
const allowedOrigins = new Set([
  'http://localhost:1420',
  'http://127.0.0.1:1420',
  'https://tauri.localhost',
  'tauri://localhost',
]);

const criticalProcesses = new Set([
  'system',
  'system idle process',
  'registry',
  'memory compression',
  'smss.exe',
  'csrss.exe',
  'wininit.exe',
  'services.exe',
  'lsass.exe',
  'winlogon.exe',
  'svchost.exe',
  'fontdrvhost.exe',
]);

const aiPatterns = /ollama|lm studio|llama|kobold|comfyui|python.*(torch|tensorflow)|stable.?diffusion|invokeai|fooocus|chatgpt|codex|claude|kimi|copilot|gpt4all|anythingllm|vllm|localai/i;
const creativePatterns = /photoshop|afterfx|premiere|blender|davinci|resolve|fusion|illustrator/i;
const browserPatterns = /chrome|msedge|firefox|brave|opera/i;
const gamePatterns = /\\steamapps\\|\\epic games\\|\\gog galaxy\\|\\riot games\\|cyberpunk2077|eldenring|starfield|valorant|fortnite|overwatch|helldivers|witcher3|rdr2\.exe/i;
const finite = (value, fallback = 0) => Number.isFinite(Number(value)) ? Number(value) : fallback;
const clamp = (value, min, max) => Math.min(max, Math.max(min, value));
const round = (value, digits = 1) => {
  const multiplier = 10 ** digits;
  return Math.round(finite(value) * multiplier) / multiplier;
};

let hardware = null;
let latestSnapshot = null;
let gpuProcessCache = new Map();
let gpuCounterPending = false;
let lastGpuCounterAt = 0;
let collectionPending = false;
let lastCollectionError = null;
let sessionWh = 0;
let lastEnergyWatts = 0;
let lastEnergyAt = Date.now();
let lastAgentCpu = process.cpuUsage();
let lastAgentCpuAt = process.hrtime.bigint();
let previousProcessCpu = new Map();
let lastProcessSampleAt = Date.now();
let sensorCache = { cpuTemperature: 0, cpuSource: null, storage: [], readings: [], sensors: [], hardwareMonitorAvailable: false, cpuSensorGuidance: null };
let sensorPending = false;
let lastSensorAt = 0;
let localAiDiscovery = { scannedAt: null, adapters: [], models: [], modelRoots: [] };
let localAiPending = false;
let lastLocalAiAt = 0;
const alertEngine = createAlertEngine();

function fallbackCpu() {
  const logical = cpus();
  const model = logical[0]?.model || 'Unknown processor';
  return {
    manufacturer: model.split(' ')[0] || 'Unknown',
    brand: model,
    cores: logical.length,
    physicalCores: Math.max(1, Math.ceil(logical.length / 2)),
    speed: round((logical[0]?.speed || 0) / 1000, 2),
    speedMax: round(Math.max(...logical.map((core) => core.speed || 0)) / 1000, 2),
    virtualization: false,
    cache: {},
  };
}

async function safe(call, fallback, timeoutMs = 5_000) {
  try {
    const timeout = new Promise((resolve) => {
      const timer = setTimeout(() => resolve(fallback), timeoutMs);
      timer.unref();
    });
    const value = await Promise.race([Promise.resolve().then(call), timeout]);
    return value ?? fallback;
  } catch {
    return fallback;
  }
}

async function initializeHardware() {
  const [system, cpu, memoryLayout, osInfo, graphics, diskLayout, baseboard, bios, networkAdapters, nvidia] = await Promise.all([
    safe(() => si.system(), {}),
    safe(() => si.cpu(), fallbackCpu()),
    safe(() => si.memLayout(), []),
    safe(() => si.osInfo(), {}),
    safe(() => si.graphics(), { controllers: [], displays: [] }),
    safe(() => si.diskLayout(), []),
    safe(() => si.baseboard(), {}),
    safe(() => si.bios(), {}),
    safe(() => si.networkInterfaces(), []),
    readNvidiaSnapshot(),
  ]);
  const cpuData = cpu?.brand ? cpu : fallbackCpu();
  const controllers = Array.isArray(graphics.controllers) ? graphics.controllers : [];
  const primaryGpu = nvidia || choosePrimaryGpu(controllers);

  hardware = {
    collectedAt: new Date().toISOString(),
    system: {
      manufacturer: system.manufacturer || baseboard.manufacturer || 'Unknown',
      model: system.model || baseboard.model || 'Windows PC',
      version: system.version || '',
    },
    motherboard: {
      manufacturer: baseboard.manufacturer || '',
      model: baseboard.model || '',
      version: baseboard.version || '',
    },
    bios: {
      vendor: bios.vendor || '',
      version: bios.version || '',
      date: bios.releaseDate || '',
      smbiosVersion: bios.revision || '',
    },
    os: {
      platform: osInfo.platform || platform(),
      distro: osInfo.distro || 'Windows',
      release: osInfo.release || release(),
      build: osInfo.build || '',
      arch: osInfo.arch || process.arch,
      hostname: osInfo.hostname || hostname(),
    },
    cpu: {
      manufacturer: cpuData.manufacturer || 'Unknown',
      brand: cpuData.brand || fallbackCpu().brand,
      cores: finite(cpuData.cores, cpus().length),
      physicalCores: finite(cpuData.physicalCores, Math.ceil(cpus().length / 2)),
      speed: finite(cpuData.speed),
      speedMax: finite(cpuData.speedMax),
      socket: cpuData.socket || '',
      virtualization: Boolean(cpuData.virtualization),
      l2CacheBytes: finite(cpuData.cache?.l2),
      l3CacheBytes: finite(cpuData.cache?.l3),
      estimatedTdp: inferCpuTdp(cpuData),
    },
    gpu: primaryGpu ? normalizeGpu(primaryGpu) : null,
    gpus: controllers.map(normalizeGpu),
    memory: {
      totalBytes: totalmem(),
      modules: memoryLayout.map((module) => ({
        sizeBytes: finite(module.size),
        type: module.type || '',
        clockMhz: finite(module.clockSpeed),
        ratedClockMhz: finite(module.clockSpeed),
        manufacturer: module.manufacturer || '',
        partNumber: module.partNum || '',
        bank: module.bank || '',
        slot: module.bank || '',
      })),
    },
    storage: diskLayout.map((disk) => ({
      name: disk.name || disk.device || 'Disk',
      type: disk.type || disk.interfaceType || '',
      sizeBytes: finite(disk.size),
      vendor: disk.vendor || '',
      smartStatus: disk.smartStatus || '',
      busType: disk.interfaceType || '',
      firmware: disk.firmwareRevision || '',
      partitions: 0,
    })),
    networks: (Array.isArray(networkAdapters) ? networkAdapters : [networkAdapters]).filter((adapter) => !adapter.virtual).map((adapter) => ({
      name: adapter.ifaceName || adapter.iface || 'Network adapter',
      manufacturer: adapter.manufacturer || '',
      type: adapter.type || '',
      speedBits: finite(adapter.speed) * 1_000_000,
      connection: adapter.iface || adapter.ifaceName || '',
      status: adapter.operstate || '',
    })),
    displays: (graphics.displays || []).map((display) => ({
      model: display.model || 'Display',
      main: Boolean(display.main),
      resolution: display.currentResX && display.currentResY ? `${display.currentResX} × ${display.currentResY}` : '',
      refreshRate: finite(display.currentRefreshRate),
    })),
  };
}

function choosePrimaryGpu(controllers = []) {
  return [...controllers].sort((left, right) => {
    const leftScore = (finite(left.memoryTotal, finite(left.vram) * MiB) || 0) + (/nvidia|amd|radeon/i.test(left.vendor) ? 10 ** 14 : 0);
    const rightScore = (finite(right.memoryTotal, finite(right.vram) * MiB) || 0) + (/nvidia|amd|radeon/i.test(right.vendor) ? 10 ** 14 : 0);
    return rightScore - leftScore;
  })[0] || null;
}

function normalizeGpu(gpu) {
  return {
    vendor: gpu.vendor || 'Unknown',
    model: gpu.model || gpu.name || 'Graphics adapter',
    vramBytes: finite(gpu.memoryTotal, finite(gpu.vram) * MiB),
    driverVersion: gpu.driverVersion || '',
    bus: gpu.bus || gpu.pciBus || '',
    powerLimit: finite(gpu.powerLimit),
    driverDate: gpu.driverDate || '',
    videoMode: gpu.name || '',
    resolution: '',
    refreshRate: 0,
    status: gpu.status || '',
  };
}

async function readNvidiaSnapshot() {
  if (process.platform !== 'win32') return null;
  try {
    const fields = [
      'name',
      'driver_version',
      'utilization.gpu',
      'memory.total',
      'memory.used',
      'temperature.gpu',
      'power.draw',
      'power.limit',
      'clocks.current.graphics',
      'clocks.current.memory',
      'fan.speed',
    ];
    const { stdout } = await execFileAsync('nvidia-smi.exe', [
      `--query-gpu=${fields.join(',')}`,
      '--format=csv,noheader,nounits',
    ], { windowsHide: true, timeout: 4000, maxBuffer: 256 * 1024 });
    const firstLine = String(stdout || '').trim().split(/\r?\n/)[0];
    if (!firstLine) return null;
    const values = firstLine.split(',').map((value) => value.trim());
    return {
      vendor: 'NVIDIA',
      model: values[0],
      driverVersion: values[1],
      utilizationGpu: finite(values[2]),
      memoryTotal: finite(values[3]) * MiB,
      memoryUsed: finite(values[4]) * MiB,
      temperatureGpu: finite(values[5]),
      powerDraw: finite(values[6]),
      powerLimit: finite(values[7]),
      clockCore: finite(values[8]),
      clockMemory: finite(values[9]),
      fanSpeed: finite(values[10]),
      vram: finite(values[3]),
      telemetrySource: 'nvidia-smi',
    };
  } catch {
    return null;
  }
}

async function readFallbackProcesses() {
  if (process.platform !== 'win32') return { all: 0, list: [] };
  try {
    const { stdout } = await execFileAsync('powershell.exe', [
      '-NoProfile',
      '-NonInteractive',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      PROCESS_SNAPSHOT_SCRIPT,
    ], { windowsHide: true, timeout: 7000, maxBuffer: 8 * MiB });
    const parsed = JSON.parse(String(stdout || '[]').trim() || '[]');
    const rows = Array.isArray(parsed) ? parsed : [parsed];
    const sampledAt = Date.now();
    const elapsedSeconds = Math.max(0.1, (sampledAt - lastProcessSampleAt) / 1000);
    const logicalCores = Math.max(1, cpus().length);
    const nextCpu = new Map();
    const list = rows.map((row) => {
      const pid = Number(row.pid);
      const cpuSeconds = finite(row.cpuSeconds, -1);
      const previous = previousProcessCpu.get(pid);
      const cpu = previous && cpuSeconds >= 0
        ? clamp((cpuSeconds - previous.cpuSeconds) / elapsedSeconds / logicalCores * 100, 0, 100)
        : 0;
      if (cpuSeconds >= 0) nextCpu.set(pid, { cpuSeconds });
      return {
        pid,
        parentPid: 0,
        name: row.name || `PID ${pid}`,
        cpu,
        mem: totalmem() > 0 ? finite(row.workingSetBytes) / totalmem() * 100 : 0,
        memRss: finite(row.workingSetBytes),
        memVsz: finite(row.virtualBytes),
        workingSetBytes: finite(row.workingSetBytes),
        privateBytes: finite(row.privateBytes),
        virtualBytes: finite(row.virtualBytes),
        priority: finite(row.priority),
        started: row.started || '',
        state: row.responding === false ? 'not responding' : 'running',
        user: '',
        command: row.name || '',
        params: '',
        path: row.path || '',
        handles: finite(row.handles),
        threads: finite(row.threads),
        responding: row.responding ?? null,
      };
    });
    previousProcessCpu = nextCpu;
    lastProcessSampleAt = sampledAt;
    return { all: list.length, running: list.filter((item) => item.state === 'running').length, list };
  } catch {
    return { all: 0, list: [] };
  }
}

async function readMemorySnapshot() {
  if (process.platform !== 'win32') return null;
  try {
    const { stdout } = await execFileAsync('powershell.exe', [
      '-NoProfile',
      '-NonInteractive',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      MEMORY_SCRIPT,
    ], { windowsHide: true, timeout: 7000, maxBuffer: 512 * 1024 });
    return JSON.parse(String(stdout || '{}').trim() || '{}');
  } catch {
    return null;
  }
}

async function pollGpuProcessCounters() {
  if (process.platform !== 'win32' || gpuCounterPending || Date.now() - lastGpuCounterAt < GPU_COUNTER_INTERVAL_MS) return;
  gpuCounterPending = true;
  lastGpuCounterAt = Date.now();
  try {
    const { stdout } = await execFileAsync('powershell.exe', [
      '-NoProfile',
      '-NonInteractive',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      GPU_COUNTER_SCRIPT,
    ], { windowsHide: true, timeout: 15_000, maxBuffer: 2 * MiB });
    const parsed = JSON.parse(String(stdout || '[]').trim() || '[]');
    const rows = Array.isArray(parsed) ? parsed : [parsed];
    gpuProcessCache = new Map(rows.map((row) => [Number(row.pid), {
      gpu: clamp(finite(row.gpu), 0, 100),
      dedicatedBytes: Math.max(0, finite(row.dedicatedBytes)),
      engines: row.engines || {},
    }]));
  } catch {
    // Per-process GPU counters may be unavailable on older Windows builds.
  } finally {
    lastGpuCounterAt = Date.now();
    gpuCounterPending = false;
  }
}

async function pollSensorSnapshot() {
  if (process.platform !== 'win32' || sensorPending || Date.now() - lastSensorAt < SENSOR_INTERVAL_MS) return;
  sensorPending = true;
  lastSensorAt = Date.now();
  try {
    const { stdout } = await execFileAsync('powershell.exe', [
      '-NoProfile',
      '-NonInteractive',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      SENSOR_SCRIPT,
    ], { windowsHide: true, timeout: 15_000, maxBuffer: 2 * MiB });
    const parsed = JSON.parse(String(stdout || '{}').trim() || '{}');
    sensorCache = {
      cpuTemperature: Math.max(0, finite(parsed.cpuTemperature)),
      cpuSource: parsed.cpuSource || null,
      storage: Array.isArray(parsed.storage) ? parsed.storage : parsed.storage ? [parsed.storage] : [],
      readings: Array.isArray(parsed.readings) ? parsed.readings : parsed.readings ? [parsed.readings] : [],
      sensors: Array.isArray(parsed.sensors) ? parsed.sensors : parsed.sensors ? [parsed.sensors] : [],
      hardwareMonitorAvailable: Boolean(parsed.hardwareMonitorAvailable),
      cpuSensorGuidance: parsed.cpuSensorGuidance || null,
    };
  } catch {
    // Optional hardware sensors stay explicitly unavailable.
  } finally {
    lastSensorAt = Date.now();
    sensorPending = false;
  }
}

async function pollLocalAiDiscovery() {
  if (localAiPending || Date.now() - lastLocalAiAt < LOCAL_AI_INTERVAL_MS) return;
  localAiPending = true;
  lastLocalAiAt = Date.now();
  try {
    localAiDiscovery = await discoverLocalModels();
  } catch {
    // Process-level detection remains available when a runtime API is offline.
  } finally {
    lastLocalAiAt = Date.now();
    localAiPending = false;
  }
}

function classifyProcess(processData) {
  const haystack = `${processData.name} ${processData.command} ${processData.path}`;
  const aiIdentity = detectAiApplication(processData);
  if (aiIdentity && aiIdentity.role !== 'ui-helper') return 'AI';
  if (!/msedgewebview2/i.test(processData.name || '') && aiPatterns.test(haystack)) return 'AI';
  if (creativePatterns.test(haystack)) return 'Creative';
  if (gamePatterns.test(haystack)) return 'Game';
  return undefined;
}

function colorForProcess(name) {
  let hash = 0;
  for (const character of String(name)) hash = ((hash << 5) - hash + character.charCodeAt(0)) | 0;
  const colors = ['#58d9ff', '#9c8cff', '#63e6be', '#ffbd5b', '#ff7190', '#6cbf7d', '#7785e8'];
  return colors[Math.abs(hash) % colors.length];
}

function isProtected(processData) {
  const name = String(processData.name || '').toLowerCase();
  return processData.pid <= 4 || processData.pid === process.pid || criticalProcesses.has(name);
}

function mergeProcessData(nativeProcesses, fallbackProcesses) {
  if (!nativeProcesses?.list?.length) return fallbackProcesses;

  const fallbackByPid = new Map(
    (fallbackProcesses?.list || []).map((item) => [Number(item.pid), item]),
  );

  return {
    ...nativeProcesses,
    list: nativeProcesses.list.map((item) => {
      const fallback = fallbackByPid.get(Number(item.pid));
      return {
        ...fallback,
        ...item,
        // systeminformation may return a valid Windows process row with zeroed
        // memory counters. Keep the richer Get-Process snapshot in that case.
        memRss: Math.max(finite(item.memRss), finite(fallback?.memRss)),
        memVsz: Math.max(finite(item.memVsz), finite(fallback?.memVsz)),
        handles: finite(fallback?.handles, finite(item.handles)),
        threads: finite(fallback?.threads, finite(item.threads)),
        path: item.path || fallback?.path || '',
        command: item.command || fallback?.command || '',
      };
    }),
  };
}

function normalizeProcesses(processData, memoryUsedBytes, memoryTotalBytes) {
  const list = Array.isArray(processData.list) ? processData.list : [];
  const normalized = list.map((item) => {
    const gpu = gpuProcessCache.get(Number(item.pid)) || {};
    const name = item.name || `PID ${item.pid}`;
    const aiIdentity = detectAiApplication(item);
    // Windows exposes unused CPU capacity as PID 0. It is not workload and must
    // never be attributed power, memory, or GPU usage.
    const idleProcess = Number(item.pid) === 0 || /^(system idle process|idle)(\.exe)?$/i.test(name);
    const memoryGb = idleProcess ? 0 : Math.max(0, finite(item.memRss) / GiB);
    return {
      id: Number(item.pid),
      parentId: Number(item.parentPid),
      name,
      subtitle: `${aiIdentity?.application || item.user || 'Local'} · PID ${item.pid}`,
      icon: name.replace(/\.exe$/i, '').slice(0, 2).toUpperCase(),
      color: colorForProcess(name),
      cpu: idleProcess ? 0 : round(Math.max(0, finite(item.cpu)), 1),
      gpu: idleProcess ? 0 : round(finite(gpu.gpu), 1),
      memory: round(memoryGb, 2),
      memoryPercent: memoryTotalBytes > 0 ? round(finite(item.workingSetBytes, item.memRss) / memoryTotalBytes * 100, 2) : 0,
      workingSetBytes: idleProcess ? 0 : finite(item.workingSetBytes, item.memRss),
      privateBytes: idleProcess ? 0 : finite(item.privateBytes),
      virtualBytes: idleProcess ? 0 : finite(item.virtualBytes, item.memVsz),
      vram: idleProcess ? 0 : round(finite(gpu.dedicatedBytes) / GiB, 2),
      disk: 0,
      network: 0,
      power: 'Very low',
      kind: classifyProcess(item),
      aiApplication: aiIdentity?.application,
      aiRuntime: aiIdentity?.runtime,
      aiRole: aiIdentity?.role,
      gpuEngines: gpu.engines || {},
      priority: finite(item.priority),
      state: item.state || '',
      user: item.user || '',
      path: item.path || '',
      command: item.command || '',
      handles: finite(item.handles),
      threads: finite(item.threads),
      startedAt: item.started || null,
      uptimeSeconds: item.started ? Math.max(0, Math.round((Date.now() - new Date(item.started).getTime()) / 1000)) : 0,
      responding: item.responding ?? (item.state === 'not responding' ? false : null),
      protected: isProtected(item),
    };
  });

  return {
    counts: {
      all: finite(processData.all, normalized.length),
      running: finite(processData.running),
      blocked: finite(processData.blocked),
      sleeping: finite(processData.sleeping),
    },
    list: normalized,
    memoryUsedGb: memoryUsedBytes / GiB,
  };
}

function calculateAgentOverhead() {
  const now = process.hrtime.bigint();
  const elapsedMicros = Number(now - lastAgentCpuAt) / 1000;
  const delta = process.cpuUsage(lastAgentCpu);
  const logicalCores = Math.max(1, cpus().length);
  const cpuPercent = elapsedMicros > 0 ? ((delta.user + delta.system) / elapsedMicros) * 100 / logicalCores : 0;
  lastAgentCpu = process.cpuUsage();
  lastAgentCpuAt = now;
  return {
    cpuPercent: round(cpuPercent, 2),
    memoryMb: round(process.memoryUsage().rss / MiB, 1),
    pid: process.pid,
    uptimeSeconds: Math.round(process.uptime()),
  };
}

async function collectSnapshot() {
  if (collectionPending) return latestSnapshot;
  collectionPending = true;
  const startedAt = Date.now();

  try {
    void pollGpuProcessCounters();
    void pollSensorSnapshot();
    void pollLocalAiDiscovery();
    const [load, speed, temperatures, memory, graphics, fsStats, disksIo, networks, nativeProcesses, fallbackProcesses, memoryDetails, nvidia] = await Promise.all([
      safe(() => si.currentLoad(), { currentLoad: 0, cpus: [] }),
      safe(() => si.cpuCurrentSpeed(), { avg: 0, min: 0, max: 0, cores: [] }),
      safe(() => si.cpuTemperature(), { main: 0, max: 0, cores: [] }),
      safe(() => si.mem(), { total: totalmem(), used: totalmem() - freemem(), available: freemem(), swaptotal: 0, swapused: 0 }),
      safe(() => si.graphics(), { controllers: [] }),
      safe(() => si.fsStats(), { rx_sec: 0, wx_sec: 0 }),
      safe(() => si.disksIO(), { rWaitPercent: 0, wWaitPercent: 0, tWaitPercent: 0 }),
      safe(() => si.networkStats(), []),
      safe(() => si.processes(), { all: 0, list: [] }),
      readFallbackProcesses(),
      readMemorySnapshot(),
      readNvidiaSnapshot(),
    ]);

    const processData = mergeProcessData(nativeProcesses, fallbackProcesses);
    const gpu = nvidia || choosePrimaryGpu(graphics.controllers || []);
    if (gpu && !hardware.gpu) hardware.gpu = normalizeGpu(gpu);
    const memoryTotal = Math.max(1, finite(memory.total, totalmem()));
    const memoryUsed = Math.max(0, finite(memory.used, memoryTotal - finite(memory.available)));
    const gpuTotalBytes = Math.max(0, finite(gpu?.memoryTotal, finite(gpu?.vram) * MiB));
    const gpuUsedBytes = Math.max(0, finite(gpu?.memoryUsed));
    const networkRows = Array.isArray(networks) ? networks : [networks];
    const rxBytes = networkRows.reduce((sum, item) => sum + Math.max(0, finite(item.rx_sec)), 0);
    const txBytes = networkRows.reduce((sum, item) => sum + Math.max(0, finite(item.tx_sec)), 0);
    const cpuLoad = clamp(finite(load.currentLoad), 0, 100);
    const gpuLoad = clamp(finite(gpu?.utilizationGpu), 0, 100);
    const diskActivity = clamp(finite(disksIo.tWaitPercent, finite(disksIo.rWaitPercent) + finite(disksIo.wWaitPercent)), 0, 100);
    const cpuSpeedRatio = hardware.cpu.speedMax > 0 ? finite(speed.avg) / hardware.cpu.speedMax : 1;
    const energyEstimate = estimateEnergy({
      cpuLoad,
      gpuLoad,
      cpuTdp: hardware.cpu.estimatedTdp,
      cpuSpeedRatio,
      measuredGpuPower: finite(gpu?.powerDraw),
      gpuPowerLimit: finite(gpu?.powerLimit, hardware.gpu?.powerLimit || 220),
      memoryTotalGb: memoryTotal / GiB,
      diskActivity,
    });

    const now = Date.now();
    const elapsedHours = Math.min(5000, Math.max(0, now - lastEnergyAt)) / 3_600_000;
    if (lastEnergyWatts > 0) sessionWh += ((lastEnergyWatts + energyEstimate.watts) / 2) * elapsedHours;
    lastEnergyWatts = energyEstimate.watts;
    lastEnergyAt = now;

    const processResult = normalizeProcesses(processData, memoryUsed, memoryTotal);
    const attributed = attributeProcessEnergy(processResult.list, energyEstimate, processResult.memoryUsedGb, {
      systemCpuLoad: cpuLoad,
      systemGpuLoad: gpuLoad,
    })
      .map((item) => ({
        ...item,
        power: item.energyWatts >= 30 ? 'High' : item.energyWatts >= 10 ? 'Moderate' : item.energyWatts >= 2 ? 'Low' : 'Very low',
      }))
      .sort((left, right) => right.energyWatts - left.energyWatts);
    const cpuTemperature = round(Math.max(finite(temperatures.main), finite(sensorCache.cpuTemperature)), 1);
    const storageTemperatures = (sensorCache.storage || [])
      .map((item) => ({
        name: item.name || `Storage ${item.deviceId || ''}`.trim(),
        deviceId: item.deviceId ?? null,
        temperature: round(Math.max(0, finite(item.temperature)), 1),
        temperatureMax: round(Math.max(0, finite(item.temperatureMax)), 1),
        wear: Number.isFinite(Number(item.wear)) ? round(item.wear, 1) : null,
        source: item.source || 'Windows sensor',
      }))
      .filter((item) => item.temperature > 0);
    const ssdTemperature = storageTemperatures.reduce((highest, item) => Math.max(highest, item.temperature), 0);
    const temperatureReadings = (sensorCache.readings || []).map((item) => ({
      component: item.component || 'Other',
      name: item.name || 'Temperature sensor',
      value: round(Math.max(0, finite(item.value)), 1),
      min: Number.isFinite(Number(item.min)) ? round(item.min, 1) : null,
      max: Number.isFinite(Number(item.max)) ? round(item.max, 1) : null,
      source: item.source || 'Windows hardware monitor',
      accuracy: item.accuracy || 'hardware-monitor',
    })).filter((item) => item.value > 0);
    if (finite(gpu?.temperatureGpu) > 0 && !temperatureReadings.some((item) => item.component === 'GPU')) {
      temperatureReadings.push({ component: 'GPU', name: 'GPU core', value: round(gpu.temperatureGpu, 1), min: null, max: null, source: gpu?.telemetrySource || 'graphics driver', accuracy: 'graphics-driver' });
    }
    const hardwareSensors = (sensorCache.sensors || []).map((item) => ({
      component: item.component || 'Other',
      name: item.name || 'Hardware sensor',
      sensorType: item.sensorType || 'Other',
      value: round(finite(item.value), 2),
      min: Number.isFinite(Number(item.min)) ? round(item.min, 2) : null,
      max: Number.isFinite(Number(item.max)) ? round(item.max, 2) : null,
      unit: item.unit || '',
      source: item.source || 'Windows hardware monitor',
      accuracy: item.accuracy || 'hardware-monitor',
    }));
    const addSensor = (component, name, sensorType, value, unit, source = 'systeminformation', accuracy = 'os-counter') => hardwareSensors.push({ component, name, sensorType, value: round(value, 2), min: null, max: null, unit, source, accuracy });
    addSensor('CPU', 'CPU Total', 'Load', cpuLoad, '%');
    addSensor('CPU', 'Average clock', 'Clock', finite(speed.avg), 'GHz');
    (load.cpus || []).forEach((core, index) => addSensor('CPU', `Logical CPU ${index}`, 'Load', finite(core.load), '%'));
    addSensor('Memory', 'Physical memory', 'Load', memoryUsed / memoryTotal * 100, '%');
    addSensor('Storage', 'Disk activity', 'Load', diskActivity, '%');
    addSensor('Storage', 'Disk read', 'Throughput', Math.max(0, finite(fsStats.rx_sec)) / MiB, 'MB/s');
    addSensor('Storage', 'Disk write', 'Throughput', Math.max(0, finite(fsStats.wx_sec)) / MiB, 'MB/s');
    addSensor('Network', 'Download', 'Throughput', rxBytes * 8 / 1_000_000, 'Mbps');
    addSensor('Network', 'Upload', 'Throughput', txBytes * 8 / 1_000_000, 'Mbps');
    if (gpu) {
      addSensor('GPU', 'GPU Core', 'Load', gpuLoad, '%', gpu.telemetrySource || 'graphics driver', 'graphics-driver');
      addSensor('GPU', 'Board power', 'Power', finite(gpu.powerDraw, energyEstimate.breakdown.gpu), 'W', finite(gpu.powerDraw) > 0 ? gpu.telemetrySource || 'graphics driver' : 'VISOR Energy Lens', finite(gpu.powerDraw) > 0 ? 'graphics-driver' : 'modeled-estimate');
    }
    const gamingProcess = attributed
      .filter((item) => item.kind === 'Game' && (item.gpu >= 20 || item.cpu >= 10))
      .sort((left, right) => right.gpu - left.gpu || right.cpu - left.cpu)[0];
    const gaming = gamingProcess
      ? { active: true, processName: gamingProcess.name, pid: gamingProcess.id }
      : { active: false, processName: null, pid: null };
    const localAI = buildLocalAiSnapshot(attributed, localAiDiscovery);
    const projection = projectEnergy(
      energyEstimate.watts,
      sessionWh,
      finite(process.env.VISOR_TARIFF_EUR_KWH, 0.25),
      finite(process.env.VISOR_CARBON_G_KWH, 56),
    );

    const metricSnapshot = {
      cpu: round(cpuLoad, 1),
      gpu: round(gpuLoad, 1),
      ram: round(memoryUsed / memoryTotal * 100, 1),
      vram: gpuTotalBytes > 0 ? round(gpuUsedBytes / gpuTotalBytes * 100, 1) : 0,
      cpuTemp: cpuTemperature,
      gpuTemp: round(finite(gpu?.temperatureGpu), 1),
      ssdTemp: ssdTemperature,
      storageTemperatures,
      temperatureReadings,
      hardwareSensors,
      sensorGuidance: sensorCache.cpuSensorGuidance,
      sensorSources: {
        cpu: cpuTemperature > 0 ? (finite(temperatures.main) > 0 ? 'systeminformation' : sensorCache.cpuSource) : null,
        gpu: finite(gpu?.temperatureGpu) > 0 ? (gpu?.telemetrySource || 'graphics driver') : null,
        storage: [...new Set(storageTemperatures.map((item) => item.source))],
      },
      cpuPower: energyEstimate.breakdown.cpu,
      gpuPower: round(finite(gpu?.powerDraw, energyEstimate.breakdown.gpu), 1),
      cpuSpeedGhz: round(finite(speed.avg), 2),
      cpuCoreLoads: (load.cpus || []).map((core) => round(finite(core.load), 1)),
      download: round(rxBytes * 8 / 1_000_000, 2),
      upload: round(txBytes * 8 / 1_000_000, 2),
      diskRead: round(Math.max(0, finite(fsStats.rx_sec)) / MiB, 2),
      diskWrite: round(Math.max(0, finite(fsStats.wx_sec)) / MiB, 2),
      diskActivity: round(diskActivity, 1),
      memory: {
        totalBytes: finite(memoryDetails?.totalBytes, memoryTotal),
        usedBytes: finite(memoryDetails?.usedBytes, memoryUsed),
        availableBytes: finite(memoryDetails?.availableBytes, memory.available),
        cachedBytes: finite(memoryDetails?.cachedBytes, memory.cached),
        committedBytes: finite(memoryDetails?.committedBytes),
        commitLimitBytes: finite(memoryDetails?.commitLimitBytes),
        pagedPoolBytes: finite(memoryDetails?.pagedPoolBytes),
        nonPagedPoolBytes: finite(memoryDetails?.nonPagedPoolBytes),
        pagesPerSecond: finite(memoryDetails?.pagesPerSecond),
        source: finite(memoryDetails?.totalBytes) > 0 ? memoryDetails?.source || 'Windows memory manager' : 'systeminformation',
        swapTotalBytes: finite(memory.swaptotal),
        swapUsedBytes: finite(memory.swapused),
      },
      gpuMemory: { totalBytes: gpuTotalBytes, usedBytes: gpuUsedBytes },
    };
    const alerts = alertEngine.evaluate(metricSnapshot, gaming, now);
    const hardwareSnapshot = {
      ...hardware,
      storage: hardware.storage.map((disk, index) => {
        const normalizedName = String(disk.name || '').toLowerCase().replace(/[^a-z0-9]/g, '');
        const sensor = storageTemperatures.find((item) => {
          const sensorName = String(item.name || '').toLowerCase().replace(/[^a-z0-9]/g, '');
          return sensorName && normalizedName && (sensorName.includes(normalizedName) || normalizedName.includes(sensorName));
        }) || storageTemperatures[index];
        return { ...disk, temperature: sensor?.temperature || 0, temperatureSource: sensor?.source || null };
      }),
    };

    latestSnapshot = {
      timestamp: new Date(now).toISOString(),
      source: 'windows-agent',
      pollMs: POLL_MS,
      hardware: hardwareSnapshot,
      metrics: metricSnapshot,
      energy: {
        ...energyEstimate,
        ...projection,
      },
      processes: attributed,
      processCounts: processResult.counts,
      localAI,
      alerts,
      agent: {
        ...calculateAgentOverhead(),
        sampleDurationMs: Date.now() - startedAt,
        gpuAttributionAvailable: gpuProcessCache.size > 0,
        sensorAttributionAvailable: cpuTemperature > 0 || storageTemperatures.length > 0,
        lastError: lastCollectionError,
      },
    };
    lastCollectionError = null;
    return latestSnapshot;
  } catch (error) {
    lastCollectionError = error instanceof Error ? error.message : String(error);
    return latestSnapshot;
  } finally {
    collectionPending = false;
  }
}

function corsHeaders(request) {
  const origin = request.headers.origin;
  const allowedOrigin = origin && allowedOrigins.has(origin) ? origin : 'http://localhost:1420';
  return {
    'Access-Control-Allow-Origin': allowedOrigin,
    'Access-Control-Allow-Headers': 'Content-Type, X-Visor-Action',
    'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
    'Cache-Control': 'no-store',
    'Content-Type': 'application/json; charset=utf-8',
    Vary: 'Origin',
  };
}

function sendJson(response, request, status, payload) {
  response.writeHead(status, corsHeaders(request));
  response.end(JSON.stringify(payload));
}

async function readJson(request) {
  let data = '';
  for await (const chunk of request) {
    data += chunk;
    if (data.length > 16_384) throw new Error('Request body is too large.');
  }
  return data ? JSON.parse(data) : {};
}

function validateActionRequest(request) {
  const origin = request.headers.origin;
  if (!origin || !allowedOrigins.has(origin)) return 'Origin is not allowed.';
  if (request.headers['x-visor-action'] !== 'confirmed') return 'VISOR action confirmation header is missing.';
  return null;
}

function findActionTarget(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) throw new Error('Invalid process identifier.');
  const target = latestSnapshot?.processes?.find((item) => item.id === pid);
  if (!target) throw new Error('The process is no longer running.');
  if (target.protected) throw new Error('VISOR protects this critical Windows process.');
  return target;
}

async function killProcess(pid, body) {
  const target = findActionTarget(pid);
  if (String(body.confirmation) !== String(pid)) throw new Error('PID confirmation does not match.');
  const args = ['/PID', String(pid)];
  if (body.tree !== false) args.push('/T');
  if (body.force === true) args.push('/F');
  await execFileAsync('taskkill.exe', args, { windowsHide: true, timeout: 10_000, maxBuffer: 256 * 1024 });
  return { ok: true, action: 'kill', pid, name: target.name };
}

async function changePriority(pid, body) {
  const target = findActionTarget(pid);
  const priorities = {
    low: osConstants.priority.PRIORITY_LOW,
    belowNormal: osConstants.priority.PRIORITY_BELOW_NORMAL,
    normal: osConstants.priority.PRIORITY_NORMAL,
    aboveNormal: osConstants.priority.PRIORITY_ABOVE_NORMAL,
    high: osConstants.priority.PRIORITY_HIGH,
  };
  if (!(body.priority in priorities)) throw new Error('Unsupported priority class.');
  setPriority(pid, priorities[body.priority]);
  return { ok: true, action: 'priority', pid, name: target.name, priority: body.priority };
}

const server = createServer(async (request, response) => {
  if (request.method === 'OPTIONS') {
    response.writeHead(204, corsHeaders(request));
    response.end();
    return;
  }

  const url = new URL(request.url || '/', `http://${request.headers.host || '127.0.0.1'}`);
  if (request.method === 'GET' && url.pathname === '/api/health') {
    sendJson(response, request, 200, {
      ok: true,
      source: 'windows-agent',
      snapshotReady: Boolean(latestSnapshot),
      lastError: lastCollectionError,
      pid: process.pid,
    });
    return;
  }
  if (request.method === 'GET' && url.pathname === '/api/snapshot') {
    const snapshot = latestSnapshot || await collectSnapshot();
    sendJson(response, request, snapshot ? 200 : 503, snapshot || { error: 'Telemetry is not ready.' });
    return;
  }

  const processAction = url.pathname.match(/^\/api\/processes\/(\d+)\/(kill|priority)$/);
  if (request.method === 'POST' && processAction) {
    const requestError = validateActionRequest(request);
    if (requestError) {
      sendJson(response, request, 403, { ok: false, error: requestError });
      return;
    }
    try {
      const body = await readJson(request);
      const pid = Number(processAction[1]);
      const result = processAction[2] === 'kill' ? await killProcess(pid, body) : await changePriority(pid, body);
      await collectSnapshot();
      sendJson(response, request, 200, result);
    } catch (error) {
      sendJson(response, request, 400, { ok: false, error: error instanceof Error ? error.message : String(error) });
    }
    return;
  }

  const alertAction = url.pathname.match(/^\/api\/alerts\/([a-z0-9-]+)$/);
  if (request.method === 'POST' && alertAction) {
    const requestError = validateActionRequest(request);
    if (requestError) {
      sendJson(response, request, 403, { ok: false, error: requestError });
      return;
    }
    try {
      const body = await readJson(request);
      if (!alertEngine.setEnabled(alertAction[1], body.enabled)) throw new Error('Unknown alert rule.');
      await collectSnapshot();
      sendJson(response, request, 200, { ok: true, action: 'alert-rule', id: alertAction[1], enabled: Boolean(body.enabled) });
    } catch (error) {
      sendJson(response, request, 400, { ok: false, error: error instanceof Error ? error.message : String(error) });
    }
    return;
  }

  sendJson(response, request, 404, { error: 'Not found.' });
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`VISOR Windows agent listening on http://127.0.0.1:${PORT}`);
  console.log(`Sampling every ${POLL_MS} ms · PID ${process.pid}`);
});

await initializeHardware();
await collectSnapshot();
setInterval(() => void collectSnapshot(), POLL_MS).unref();

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => server.close(() => process.exit(0)));
}
