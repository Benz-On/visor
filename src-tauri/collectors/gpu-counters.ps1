$ErrorActionPreference = 'Stop'

$engineSamples = (Get-Counter '\GPU Engine(*)\Utilization Percentage' -MaxSamples 1).CounterSamples
$memorySamples = (Get-Counter '\GPU Process Memory(*)\Local Usage' -MaxSamples 1).CounterSamples
$byPid = @{}

foreach ($sample in $engineSamples) {
    if ($sample.InstanceName -match '^pid_(\d+)_.*engtype_(.+)$') {
        $pidValue = [int]$Matches[1]
        if (-not $byPid.ContainsKey($pidValue)) {
            $byPid[$pidValue] = [ordered]@{ pid = $pidValue; gpu = 0.0; dedicatedBytes = 0.0; engines = @{} }
        }
        $engine = $Matches[2]
        $usage = [math]::Max(0.0, [double]$sample.CookedValue)
        $byPid[$pidValue].gpu += $usage
        $byPid[$pidValue].engines[$engine] = [math]::Round($usage, 2)
    }
}

foreach ($sample in $memorySamples) {
    if ($sample.InstanceName -match '^pid_(\d+)_') {
        $pidValue = [int]$Matches[1]
        if (-not $byPid.ContainsKey($pidValue)) {
            $byPid[$pidValue] = [ordered]@{ pid = $pidValue; gpu = 0.0; dedicatedBytes = 0.0; engines = @{} }
        }
        $byPid[$pidValue].dedicatedBytes += [math]::Max(0.0, [double]$sample.CookedValue)
    }
}

$result = foreach ($entry in $byPid.Values) {
    [pscustomobject]@{
        pid = $entry.pid
        gpu = [math]::Round([math]::Min(100.0, $entry.gpu), 2)
        dedicatedBytes = [math]::Round($entry.dedicatedBytes)
        engines = $entry.engines
    }
}

ConvertTo-Json -InputObject @($result) -Compress -Depth 4
