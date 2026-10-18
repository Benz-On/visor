$ErrorActionPreference = 'SilentlyContinue'

$cpuTemperature = $null
$cpuSource = $null
$storageSensors = @()
$hardwareSensors = @()
$temperatureReadings = @()

foreach ($namespace in @('root\LibreHardwareMonitor', 'root\OpenHardwareMonitor')) {
    try {
        $rows = @(Get-CimInstance -Namespace $namespace -ClassName Sensor | Where-Object { $null -ne $_.Value })
        foreach ($row in $rows) {
            $sensorType = [string]$row.SensorType
            $unit = switch ($sensorType) {
                'Temperature' { '°C' }
                'Load' { '%' }
                'Control' { '%' }
                'Clock' { 'MHz' }
                'Fan' { 'RPM' }
                'Voltage' { 'V' }
                'Current' { 'A' }
                'Power' { 'W' }
                'Energy' { 'mWh' }
                'Data' { 'GB' }
                'SmallData' { 'MB' }
                'Throughput' { 'MB/s' }
                'Level' { '%' }
                'Factor' { '' }
                default { '' }
            }
            $hardwareSensors += [pscustomobject]@{
                name = [string]$row.Name
                value = [double]$row.Value
                min = if ($null -ne $row.Min) { [double]$row.Min } else { $null }
                max = if ($null -ne $row.Max) { [double]$row.Max } else { $null }
                identifier = [string]$row.Identifier
                sensorType = $sensorType
                unit = $unit
                source = $namespace
            }
        }
    } catch {}
}

foreach ($sensor in $hardwareSensors) {
    $component = if ($sensor.identifier -match '/cpu/' -or $sensor.name -match 'CPU|Core|Tctl|Tdie') {
        'CPU'
    } elseif ($sensor.identifier -match '/gpu/' -or $sensor.name -match 'GPU|Hot Spot|Memory Junction') {
        'GPU'
    } elseif ($sensor.identifier -match '/nvme/|/hdd/' -or $sensor.name -match 'NVMe|SSD|Drive') {
        'Storage'
    } elseif ($sensor.name -match 'Motherboard|Mainboard|System') {
        'System'
    } else {
        'Other'
    }
    $sensor | Add-Member -NotePropertyName component -NotePropertyValue $component -Force
    $sensor | Add-Member -NotePropertyName accuracy -NotePropertyValue 'hardware-monitor' -Force
    if ($sensor.sensorType -eq 'Temperature' -and [double]$sensor.value -gt 0) {
        $temperatureReadings += [pscustomobject]@{
            component = $component
            name = [string]$sensor.name
            value = [double]$sensor.value
            min = $sensor.min
            max = $sensor.max
            source = [string]$sensor.source
            accuracy = 'hardware-monitor'
        }
    }
}

# ACPI thermal zones are useful as a firmware/system reading, but they are not
# mislabeled as CPU package temperature because many PCs expose a different zone.
try {
    foreach ($zone in @(Get-CimInstance -Namespace 'root\wmi' -ClassName MSAcpi_ThermalZoneTemperature)) {
        $value = ([double]$zone.CurrentTemperature / 10.0) - 273.15
        if ($value -gt 0 -and $value -lt 150) {
            $temperatureReadings += [pscustomobject]@{
                component = 'System'
                name = if ($zone.InstanceName) { [string]$zone.InstanceName } else { 'ACPI thermal zone' }
                value = [math]::Round($value, 1)
                min = $null
                max = $null
                source = 'Windows ACPI firmware'
                accuracy = 'firmware-zone'
            }
        }
    }
} catch {}

$cpuCandidates = @($hardwareSensors | Where-Object {
    $_.sensorType -eq 'Temperature' -and $_.name -match 'CPU Package|CPU \(Tctl/Tdie\)|CPU Tctl|Core Average|CPU Die'
} | Sort-Object {
    if ($_.name -match 'CPU Package|Tctl/Tdie') { 0 } else { 1 }
})

if ($cpuCandidates.Count -gt 0) {
    $cpuTemperature = [double]$cpuCandidates[0].value
    $cpuSource = [string]$cpuCandidates[0].source
}

$storageFromHardware = @($hardwareSensors | Where-Object {
    $_.sensorType -eq 'Temperature' -and ($_.identifier -match '/nvme/|/hdd/' -or $_.name -match 'NVMe|SSD|Drive')
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
    readings = @($temperatureReadings | Sort-Object component, name -Unique)
    sensors = @($hardwareSensors | Sort-Object component, sensorType, name -Unique)
    hardwareMonitorAvailable = $hardwareSensors.Count -gt 0
    cpuSensorGuidance = if ($null -eq $cpuTemperature) { 'Start LibreHardwareMonitor or OpenHardwareMonitor as administrator to expose an exact CPU package sensor.' } else { $null }
} | ConvertTo-Json -Compress -Depth 5
