$ErrorActionPreference = 'SilentlyContinue'

$os = Get-CimInstance -ClassName Win32_OperatingSystem
$counter = Get-CimInstance -ClassName Win32_PerfFormattedData_PerfOS_Memory

$totalBytes = if ($null -ne $os.TotalVisibleMemorySize) { [double]$os.TotalVisibleMemorySize * 1024 } else { 0 }
$availableBytes = if ($null -ne $os.FreePhysicalMemory) { [double]$os.FreePhysicalMemory * 1024 } else { 0 }
$cachedBytes = if ($null -ne $counter.CacheBytes) { [double]$counter.CacheBytes } else { 0 }
$committedBytes = if ($null -ne $counter.CommittedBytes) { [double]$counter.CommittedBytes } else { 0 }
$commitLimitBytes = if ($null -ne $counter.CommitLimit) { [double]$counter.CommitLimit } else { 0 }

[pscustomobject]@{
    totalBytes = $totalBytes
    availableBytes = $availableBytes
    cachedBytes = $cachedBytes
    committedBytes = $committedBytes
    commitLimitBytes = $commitLimitBytes
    pagedPoolBytes = if ($null -ne $counter.PoolPagedBytes) { [double]$counter.PoolPagedBytes } else { 0 }
    nonPagedPoolBytes = if ($null -ne $counter.PoolNonpagedBytes) { [double]$counter.PoolNonpagedBytes } else { 0 }
    pagesPerSecond = if ($null -ne $counter.PagesPersec) { [double]$counter.PagesPersec } else { 0 }
    source = 'Windows memory manager'
} | ConvertTo-Json -Compress -Depth 3
