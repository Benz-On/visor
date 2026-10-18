$ErrorActionPreference = 'SilentlyContinue'

# LibreHardwareMonitor 0.9.x no longer registers the WMI namespace that older
# builds and OpenHardwareMonitor exposed, so its loopback web server is the only
# channel on those installs. Both are read, web server first, and every sensor
# keeps the channel it arrived on so the UI can attribute the number honestly.
#
# The web server formats every value with the current culture - "55,6 degC" on a
# French host, and RawValue is formatted the same way - so the digits are parsed
# back with that culture rather than assuming a dot decimal separator.

$numberStyles = [System.Globalization.NumberStyles]::Float -bor [System.Globalization.NumberStyles]::AllowThousands
$cultures = @([System.Globalization.CultureInfo]::CurrentCulture, [System.Globalization.CultureInfo]::InvariantCulture)

function ConvertTo-SensorNumber {
    param($Value)
    if ($null -eq $Value) { return $null }
    if ($Value -is [double] -or $Value -is [int] -or $Value -is [long] -or $Value -is [decimal]) {
        return [double]$Value
    }
    # Unit text and space separators carry no digits, so removing them leaves one
    # numeric run whether the culture groups with spaces or not.
    $compact = ([string]$Value) -replace '[\s\u00A0\u2007\u2009\u202F]', ''
    $compact = $compact -replace '\u2212', '-'
    $match = [regex]::Match($compact, '^[-+]?[0-9][0-9.,]*')
    if (-not $match.Success) { return $null }
    $candidate = $match.Value.Trim('.,')
    foreach ($culture in $cultures) {
        $parsed = 0.0
        if ([double]::TryParse($candidate, $numberStyles, $culture, [ref]$parsed)) { return $parsed }
    }
    return $null
}

function Get-SensorUnit {
    param($SensorType)
    $degree = [string][char]176
    switch ($SensorType) {
        'Temperature' { return $degree + 'C' }
        'Load' { return '%' }
        'Control' { return '%' }
        'Clock' { return 'MHz' }
        'Fan' { return 'RPM' }
        'Voltage' { return 'V' }
        'Current' { return 'A' }
        'Power' { return 'W' }
        'Energy' { return 'mWh' }
        'Data' { return 'GB' }
        'SmallData' { return 'MB' }
        'Throughput' { return 'MB/s' }
        'Level' { return '%' }
        default { return '' }
    }
}

# SensorId is a path the hardware monitor owns, so it classifies far more
# reliably than the display name: "DIMM #1" is memory, a "/lpc/" "CPU Core" is
# the motherboard's view of the CPU, and drive setpoints sit under /nvme/ too.
function Get-SensorComponent {
    param($Identifier, $Name)
    if ($Identifier -match '^/(amdcpu|intelcpu)/') { return 'CPU' }
    if ($Identifier -match '^/gpu') { return 'GPU' }
    if ($Identifier -match '^/(nvme|ssd|hdd)/') { return 'Storage' }
    if ($Identifier -match '^/memory/') { return 'Memory' }
    if ($Identifier -match '^/lpc/') {
        if ($Name -match 'CPU|Core|Tctl|Tdie') { return 'CPU' }
        if ($Name -match 'GPU|VGA|Graphics') { return 'GPU' }
        if ($Name -match 'NVMe|SSD|Drive') { return 'Storage' }
        return 'System'
    }
    if ($Name -match 'CPU|Core|Tctl|Tdie') { return 'CPU' }
    if ($Name -match 'GPU|GeForce|Radeon|Graphics') { return 'GPU' }
    if ($Name -match 'NVMe|SSD|Drive') { return 'Storage' }
    return 'System'
}

# A drive reports its warning and critical setpoints as Temperature sensors and a
# DIMM reports its probe resolution. They are limits, not readings, and must not
# reach a chart or be selected as a drive or CPU temperature.
function Test-ThresholdSensor {
    param($Name)
    return [bool]($Name -match 'Limit|Resolution|Threshold|Warning|Critical|Offset|Margin|Protection')
}

# The tree is root -> host -> device -> group -> sensor and a leaf only reports
# "Composite Temperature", so the owning device name is carried down from the
# device level. Depth is passed as an integer on purpose: a path array collapses
# into a single string as soon as it holds one element.
function Get-SensorLeaf {
    param($Node, $Depth, $Device, $Accumulator)
    if ($null -eq $Node) { return }
    $ownDevice = if ($Depth -eq 2) { [string]$Node.Text } else { $Device }
    if ($Node.Type) {
        [void]$Accumulator.Add([pscustomobject]@{ Sensor = $Node; Device = [string]$Device })
    }
    foreach ($child in $Node.Children) {
        Get-SensorLeaf -Node $child -Depth ($Depth + 1) -Device $ownDevice -Accumulator $Accumulator
    }
}

# Ranked so the package/whole-die reading wins over a single core or CCD, which
# is what "CPU temperature" has to mean for the thermal and energy claims. AMD
# names that sensor "Core (Tctl/Tdie)", which the previous filter - looking for
# "CPU (Tctl/Tdie)" - could not match even when its WMI channel was answering.
function Get-CpuTemperatureRank {
    param($Name, $Identifier)
    if ($Name -match 'Tctl/Tdie|CPU Package|^Package$|CPU Die|^CPU Tctl') { return 0 }
    if ($Name -match 'CCDs Max|Core Average|Average') { return 1 }
    if ($Identifier -match '^/(amdcpu|intelcpu)/') { return 2 }
    return 3
}

$cpuTemperature = $null
$cpuSource = $null
$cpuPower = $null
$storageSensors = @()
$hardwareSensors = @()
$temperatureReadings = @()

$lhmPort = 8085
if ($env:VISOR_LHM_PORT -match '^\d{2,5}$') { $lhmPort = [int]$env:VISOR_LHM_PORT }

try {
    $uri = 'http://127.0.0.1:{0}/data.json' -f $lhmPort
    $response = Invoke-WebRequest -UseBasicParsing -Uri $uri -TimeoutSec 4
    if ([int]$response.StatusCode -eq 200) {
        $leaves = New-Object System.Collections.ArrayList
        Get-SensorLeaf -Node (ConvertFrom-Json $response.Content) -Depth 0 -Device '' -Accumulator $leaves
        $channel = 'LibreHardwareMonitor web server :{0}' -f $lhmPort
        foreach ($entry in $leaves) {
            $leaf = $entry.Sensor
            $value = ConvertTo-SensorNumber $leaf.Value
            if ($null -eq $value) { continue }
            $sensorType = [string]$leaf.Type
            $identifier = [string]$leaf.SensorId
            $name = [string]$leaf.Text
            $hardwareSensors += [pscustomobject]@{
                name = $name
                value = $value
                min = ConvertTo-SensorNumber $leaf.Min
                max = ConvertTo-SensorNumber $leaf.Max
                identifier = $identifier
                sensorType = $sensorType
                unit = Get-SensorUnit $sensorType
                source = $channel
                component = Get-SensorComponent $identifier $name
                device = $entry.Device
            }
        }
    }
} catch {}

if ($hardwareSensors.Count -eq 0) {
    foreach ($namespace in @('root\LibreHardwareMonitor', 'root\OpenHardwareMonitor')) {
        try {
            $rows = @(Get-CimInstance -Namespace $namespace -ClassName Sensor |
                Where-Object { $null -ne $_.Value })
            foreach ($row in $rows) {
                $value = ConvertTo-SensorNumber $row.Value
                if ($null -eq $value) { continue }
                $sensorType = [string]$row.SensorType
                $identifier = [string]$row.Identifier
                $name = [string]$row.Name
                $hardwareSensors += [pscustomobject]@{
                    name = $name
                    value = $value
                    min = ConvertTo-SensorNumber $row.Min
                    max = ConvertTo-SensorNumber $row.Max
                    identifier = $identifier
                    sensorType = $sensorType
                    unit = Get-SensorUnit $sensorType
                    source = $namespace
                    component = Get-SensorComponent $identifier $name
                    device = ''
                }
            }
        } catch {}
    }
}

foreach ($sensor in $hardwareSensors) {
    $sensor | Add-Member -NotePropertyName accuracy -NotePropertyValue 'hardware-monitor' -Force
    if ($sensor.sensorType -eq 'Temperature' -and $sensor.value -gt 0 -and
        -not (Test-ThresholdSensor $sensor.name)) {
        $temperatureReadings += [pscustomobject]@{
            component = $sensor.component
            name = $sensor.name
            value = $sensor.value
            min = $sensor.min
            max = $sensor.max
            source = $sensor.source
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
    $_.sensorType -eq 'Temperature' -and $_.component -eq 'CPU' -and
    $_.value -gt 0 -and $_.value -lt 150 -and -not (Test-ThresholdSensor $_.name)
} | Sort-Object { Get-CpuTemperatureRank $_.name $_.identifier }, 'name')

if ($cpuCandidates.Count -gt 0) {
    $cpuTemperature = [double]$cpuCandidates[0].value
    $cpuSource = '{0} ({1})' -f [string]$cpuCandidates[0].source, [string]$cpuCandidates[0].name
}

# Package power is the figure the energy model needs; per-core readings sum to
# less than the package and would understate it, so only the package row is
# accepted and anything else leaves the estimate on its modeled path.
$cpuPowerCandidate = @($hardwareSensors | Where-Object {
    $_.sensorType -eq 'Power' -and $_.identifier -match '^/(amdcpu|intelcpu)/' -and
    $_.name -match 'Package' -and $_.value -gt 0 -and $_.value -lt 1000
} | Sort-Object name) | Select-Object -First 1

if ($cpuPowerCandidate) {
    $cpuPower = [double]$cpuPowerCandidate.value
}

$storageFromHardware = @($hardwareSensors | Where-Object {
    $_.sensorType -eq 'Temperature' -and $_.component -eq 'Storage' -and
    $_.value -gt 0 -and -not (Test-ThresholdSensor $_.name)
})
foreach ($sensor in $storageFromHardware) {
    $drive = if ($sensor.device) { $sensor.device } else { $sensor.name }
    $storageSensors += [pscustomobject]@{
        name = '{0} {1}' -f $drive, $sensor.name
        deviceId = $null
        temperature = $sensor.value
        temperatureMax = $sensor.max
        wear = $null
        source = $sensor.source
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

$forwardedTypes = @('Temperature', 'Power', 'Fan', 'Load', 'Clock', 'Voltage', 'Control')
$perCoreTypes = @('Load', 'Clock', 'Voltage', 'Control')
# This table travels with every snapshot, so per-core rows are dropped first:
# the aggregates are what the panels read, and the device column loses nothing.
$hardwareSensors = @($hardwareSensors | Where-Object {
        $forwardedTypes -ccontains $_.sensorType -and -not (Test-ThresholdSensor $_.name) -and
        -not ($perCoreTypes -ccontains $_.sensorType -and $_.name -match '#\d') -and
        -not ($_.component -eq 'GPU' -and $_.sensorType -eq 'Load' -and $_.name -notmatch '^GPU')
    } | Sort-Object component, sensorType, name |
    Select-Object name, value, min, max, identifier, sensorType, unit, source, component, accuracy, device)

if ($hardwareSensors.Count -gt 140) {
    $hardwareSensors = @($hardwareSensors | Select-Object -First 140)
}

[pscustomobject]@{
    cpuTemperature = $cpuTemperature
    cpuSource = $cpuSource
    cpuPower = $cpuPower
    storage = @($storageSensors)
    readings = @($temperatureReadings | Sort-Object component, name)
    sensors = @($hardwareSensors)
    hardwareMonitorAvailable = $hardwareSensors.Count -gt 0
    cpuSensorGuidance = if ($null -eq $cpuTemperature) { 'Start LibreHardwareMonitor (or OpenHardwareMonitor) as administrator, then enable Options > Remote Web Server so the CPU package sensor is readable on http://127.0.0.1:8085/data.json.' } else { $null }
} | ConvertTo-Json -Compress -Depth 5
