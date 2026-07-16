$ErrorActionPreference = 'SilentlyContinue'

$cpuTemperature = $null
$cpuSource = $null
$storageSensors = @()
$hardwareSensors = @()

foreach ($namespace in @('root\LibreHardwareMonitor', 'root\OpenHardwareMonitor')) {
    try {
        $rows = @(Get-CimInstance -Namespace $namespace -ClassName Sensor | Where-Object {
            $_.SensorType -eq 'Temperature' -and [double]$_.Value -gt 0
        })
        foreach ($row in $rows) {
            $hardwareSensors += [pscustomobject]@{
                name = [string]$row.Name
                value = [double]$row.Value
                identifier = [string]$row.Identifier
                source = $namespace
            }
        }
    } catch {}
}

$cpuCandidates = @($hardwareSensors | Where-Object {
    $_.name -match 'CPU Package|CPU \(Tctl/Tdie\)|CPU Tctl|Core Average|CPU Die'
} | Sort-Object {
    if ($_.name -match 'CPU Package|Tctl/Tdie') { 0 } else { 1 }
})

if ($cpuCandidates.Count -gt 0) {
    $cpuTemperature = [double]$cpuCandidates[0].value
    $cpuSource = [string]$cpuCandidates[0].source
}

$storageFromHardware = @($hardwareSensors | Where-Object {
    $_.identifier -match '/nvme/|/hdd/' -or $_.name -match 'NVMe|SSD|Drive'
})
foreach ($sensor in $storageFromHardware) {
    $storageSensors += [pscustomobject]@{
        name = [string]$sensor.name
        deviceId = $null
        temperature = [double]$sensor.value
        temperatureMax = $null
        wear = $null
        source = [string]$sensor.source
    }
}

try {
    foreach ($disk in @(Get-PhysicalDisk)) {
        try {
            $reliability = $disk | Get-StorageReliabilityCounter
            $temperature = [double]$reliability.Temperature
            if ($temperature -gt 0) {
                $storageSensors += [pscustomobject]@{
                    name = [string]$disk.FriendlyName
                    deviceId = [string]$disk.DeviceId
                    temperature = $temperature
                    temperatureMax = if ([double]$reliability.TemperatureMax -gt 0) { [double]$reliability.TemperatureMax } else { $null }
                    wear = if ($null -ne $reliability.Wear) { [double]$reliability.Wear } else { $null }
                    source = 'Windows Storage Reliability'
                }
            }
        } catch {}
    }
} catch {}

[pscustomobject]@{
    cpuTemperature = $cpuTemperature
    cpuSource = $cpuSource
    storage = @($storageSensors)
    hardwareMonitorAvailable = $hardwareSensors.Count -gt 0
} | ConvertTo-Json -Compress -Depth 5
