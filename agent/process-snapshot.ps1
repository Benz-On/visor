$ErrorActionPreference = 'SilentlyContinue'

$result = foreach ($processItem in Get-Process) {
    $priorityValue = $null
    $startValue = $null
    $pathValue = $null
    try { $priorityValue = [int]$processItem.PriorityClass } catch {}
    try { $startValue = $processItem.StartTime.ToUniversalTime().ToString('o') } catch {}
    try { $pathValue = $processItem.Path } catch {}

    $processName = $processItem.ProcessName
    if ($processName -notmatch '\.exe$' -and $processName -notin @('System', 'Idle', 'Registry')) {
        $processName += '.exe'
    }

    [pscustomobject]@{
        pid = [int]$processItem.Id
        name = $processName
        cpuSeconds = if ($null -ne $processItem.CPU) { [double]$processItem.CPU } else { $null }
        workingSetBytes = [double]$processItem.WorkingSet64
        privateBytes = [double]$processItem.PrivateMemorySize64
        virtualBytes = [double]$processItem.VirtualMemorySize64
        handles = [int]$processItem.HandleCount
        threads = [int]$processItem.Threads.Count
        priority = $priorityValue
        started = $startValue
        path = $pathValue
        responding = [bool]$processItem.Responding
    }
}

ConvertTo-Json -InputObject @($result) -Compress -Depth 3
