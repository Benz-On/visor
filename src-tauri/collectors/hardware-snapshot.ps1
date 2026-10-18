$ErrorActionPreference = 'SilentlyContinue'

function First-CimRow([string]$className) {
    $rows = @(Get-CimInstance -ClassName $className)
    if ($rows.Count -gt 0) { return $rows[0] }
    return $null
}

function Memory-TypeName([int]$code) {
    switch ($code) {
        20 { 'DDR' }
        21 { 'DDR2' }
        24 { 'DDR3' }
        26 { 'DDR4' }
        27 { 'LPDDR' }
        28 { 'LPDDR2' }
        29 { 'LPDDR3' }
        30 { 'LPDDR4' }
        34 { 'DDR5' }
        35 { 'LPDDR5' }
        default { if ($code -gt 0) { "SMBIOS $code" } else { 'Unknown' } }
    }
}

$computer = First-CimRow 'Win32_ComputerSystem'
$baseboard = First-CimRow 'Win32_BaseBoard'
$bios = First-CimRow 'Win32_BIOS'
$processor = First-CimRow 'Win32_Processor'
$operatingSystem = First-CimRow 'Win32_OperatingSystem'

$memoryModules = foreach ($module in @(Get-CimInstance -ClassName Win32_PhysicalMemory)) {
    $memoryType = if ([int]$module.SMBIOSMemoryType -gt 0) { [int]$module.SMBIOSMemoryType } else { [int]$module.MemoryType }
    [pscustomobject]@{
        sizeBytes = [double]$module.Capacity
        type = Memory-TypeName $memoryType
        clockMhz = if ([int]$module.ConfiguredClockSpeed -gt 0) { [int]$module.ConfiguredClockSpeed } else { [int]$module.Speed }
        ratedClockMhz = [int]$module.Speed
        manufacturer = ([string]$module.Manufacturer).Trim()
        partNumber = ([string]$module.PartNumber).Trim()
        bank = ([string]$module.BankLabel).Trim()
        slot = ([string]$module.DeviceLocator).Trim()
        formFactor = [int]$module.FormFactor
    }
}

$physicalDisks = @(Get-PhysicalDisk)
$storage = foreach ($disk in @(Get-CimInstance -ClassName Win32_DiskDrive)) {
    $friendlyName = ([string]$disk.Model).Trim()
    $physical = $physicalDisks | Where-Object {
        ([string]$_.FriendlyName).Trim() -eq $friendlyName -or
        $friendlyName -like "*$([string]$_.FriendlyName)*" -or
        ([string]$_.FriendlyName) -like "*$friendlyName*"
    } | Select-Object -First 1
    [pscustomobject]@{
        name = $friendlyName
        type = if ($physical.MediaType) { [string]$physical.MediaType } else { [string]$disk.MediaType }
        sizeBytes = [double]$disk.Size
        smartStatus = if ($physical.HealthStatus) { [string]$physical.HealthStatus } else { [string]$disk.Status }
        busType = if ($physical.BusType) { [string]$physical.BusType } else { [string]$disk.InterfaceType }
        firmware = ([string]$disk.FirmwareRevision).Trim()
        partitions = [int]$disk.Partitions
        temperature = 0
        temperatureSource = $null
    }
}

$gpus = foreach ($gpu in @(Get-CimInstance -ClassName Win32_VideoController)) {
    $resolution = if ([int]$gpu.CurrentHorizontalResolution -gt 0 -and [int]$gpu.CurrentVerticalResolution -gt 0) {
        "$([int]$gpu.CurrentHorizontalResolution)x$([int]$gpu.CurrentVerticalResolution)"
    } else { '' }
    [pscustomobject]@{
        vendor = ([string]$gpu.AdapterCompatibility).Trim()
        model = ([string]$gpu.Name).Trim()
        vramBytes = [double]$gpu.AdapterRAM
        driverVersion = ([string]$gpu.DriverVersion).Trim()
        driverDate = if ($gpu.DriverDate) { [string]$gpu.DriverDate } else { '' }
        videoMode = ([string]$gpu.VideoModeDescription).Trim()
        resolution = $resolution
        refreshRate = [int]$gpu.CurrentRefreshRate
        status = ([string]$gpu.Status).Trim()
        powerLimit = 0
    }
}

$networks = foreach ($adapter in @(Get-CimInstance -ClassName Win32_NetworkAdapter | Where-Object { $_.PhysicalAdapter -and $_.NetEnabled })) {
    [pscustomobject]@{
        name = ([string]$adapter.Name).Trim()
        manufacturer = ([string]$adapter.Manufacturer).Trim()
        type = ([string]$adapter.AdapterType).Trim()
        speedBits = [double]$adapter.Speed
        connection = ([string]$adapter.NetConnectionID).Trim()
        status = ([string]$adapter.NetConnectionStatus).Trim()
    }
}

$displays = foreach ($gpu in $gpus | Where-Object { $_.resolution }) {
    [pscustomobject]@{
        model = if ($gpu.videoMode) { $gpu.videoMode } else { 'Connected display' }
        main = $true
        resolution = $gpu.resolution
        refreshRate = $gpu.refreshRate
    }
}

[pscustomobject]@{
    collectedAt = (Get-Date).ToUniversalTime().ToString('o')
    system = [pscustomobject]@{
        manufacturer = ([string]$computer.Manufacturer).Trim()
        model = ([string]$computer.Model).Trim()
        version = ([string]$computer.SystemFamily).Trim()
        totalMemoryBytes = [double]$computer.TotalPhysicalMemory
    }
    motherboard = [pscustomobject]@{
        manufacturer = ([string]$baseboard.Manufacturer).Trim()
        model = ([string]$baseboard.Product).Trim()
        version = ([string]$baseboard.Version).Trim()
    }
    bios = [pscustomobject]@{
        vendor = ([string]$bios.Manufacturer).Trim()
        version = if ($bios.SMBIOSBIOSVersion) { ([string]$bios.SMBIOSBIOSVersion).Trim() } else { ([string]$bios.Version).Trim() }
        date = if ($bios.ReleaseDate) { ([datetime]$bios.ReleaseDate).ToUniversalTime().ToString('yyyy-MM-dd') } else { '' }
        smbiosVersion = "$([int]$bios.SMBIOSMajorVersion).$([int]$bios.SMBIOSMinorVersion)"
    }
    os = [pscustomobject]@{
        platform = 'windows'
        distro = ([string]$operatingSystem.Caption).Trim()
        release = ([string]$operatingSystem.Version).Trim()
        build = ([string]$operatingSystem.BuildNumber).Trim()
        arch = ([string]$operatingSystem.OSArchitecture).Trim()
        hostname = [Environment]::MachineName
    }
    cpu = [pscustomobject]@{
        manufacturer = ([string]$processor.Manufacturer).Trim()
        brand = ([string]$processor.Name).Trim()
        cores = [int]$processor.NumberOfLogicalProcessors
        physicalCores = [int]$processor.NumberOfCores
        socket = ([string]$processor.SocketDesignation).Trim()
        speed = [double]$processor.CurrentClockSpeed / 1000
        speedMax = [double]$processor.MaxClockSpeed / 1000
        l2CacheBytes = [double]$processor.L2CacheSize * 1024
        l3CacheBytes = [double]$processor.L3CacheSize * 1024
        virtualization = [bool]$processor.VirtualizationFirmwareEnabled
    }
    gpus = @($gpus)
    memory = [pscustomobject]@{
        totalBytes = [double]$computer.TotalPhysicalMemory
        modules = @($memoryModules)
    }
    storage = @($storage)
    networks = @($networks)
    displays = @($displays)
} | ConvertTo-Json -Compress -Depth 6
