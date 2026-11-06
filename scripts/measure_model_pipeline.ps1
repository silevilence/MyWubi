param(
    [Parameter(Mandatory)][string]$BinaryDirectory,
    [Parameter(Mandatory)][string]$Store,
    [Parameter(Mandatory)][string]$ModelId,
    [Parameter(Mandatory)][string]$ModelVersion,
    [ValidateSet('cpu','vulkan','cuda','auto')][string]$Device = 'cpu',
    [ValidateSet('on_demand','preload_keep')][string]$Loading = 'preload_keep',
    [ValidateRange(0,3600)][int]$Seconds = 0,
    [ValidateRange(1,10000)][int]$Iterations = 60,
    [Parameter(Mandatory)][string]$OutputPrefix
)
$ErrorActionPreference='Stop'
$binaryPath=(Resolve-Path -LiteralPath $BinaryDirectory).Path
$storePath=(Resolve-Path -LiteralPath $Store).Path
$outputPath=[IO.Path]::GetFullPath($OutputPrefix)
if ($ModelId -notmatch '^[a-zA-Z0-9_-]+$' -or $ModelVersion -notmatch '^[a-zA-Z0-9_-]+$') {throw 'Invalid model identity'}
New-Item -ItemType Directory -Path (Split-Path $outputPath -Parent) -Force | Out-Null
foreach($suffix in @('.jsonl','.resources.jsonl','.stderr','.meta.json')) { if(Test-Path -LiteralPath "$outputPath$suffix"){throw 'Use a fresh output prefix to preserve prior evidence'} }
@{timestamp_utc=[DateTime]::UtcNow.ToString('o'); device=$Device; loading=$Loading; duration_seconds=$Seconds;
    worker_sha256=(Get-FileHash -LiteralPath (Join-Path $binaryPath 'model-worker.exe') -Algorithm SHA256).Hash.ToLowerInvariant();
    probe_sha256=(Get-FileHash -LiteralPath (Join-Path $binaryPath 'probe-pipeline.exe') -Algorithm SHA256).Hash.ToLowerInvariant();
    cpu_logical_count=[Environment]::ProcessorCount; temperature_celsius=$null; power_watts=$null;
    unavailable='No calibrated package-power or GPU-temperature provider is available in this harness';
    measurement_boundary='confirmed shared-session commit -> independent worker IPC -> shared candidate publication; excludes GDI paint/display scanout'} |
    ConvertTo-Json | Set-Content -LiteralPath "$outputPath.meta.json" -Encoding utf8
$probe=Start-Process -FilePath (Join-Path $binaryPath 'probe-pipeline.exe') -ArgumentList @( ('"{0}"' -f $storePath),$ModelId,$ModelVersion,$Device,$Loading,$Seconds,$Iterations) -WindowStyle Hidden -PassThru -RedirectStandardOutput "$outputPath.jsonl" -RedirectStandardError "$outputPath.stderr"
$watch=[Diagnostics.Stopwatch]::StartNew()
$workerId=$null
$lastCpu=$null
$lastTime=$null
try {
    while(-not $probe.HasExited) {
        if($watch.Elapsed.TotalSeconds -gt ($Seconds+180)) {throw 'Benchmark exceeded watchdog deadline'}
        if(-not $workerId) {
            $child=Get-CimInstance Win32_Process -Filter "ParentProcessId = $($probe.Id) AND Name = 'model-worker.exe'" | Select-Object -First 1
            if($child){$workerId=$child.ProcessId}
        }
        if($workerId) {
            $process=Get-Process -Id $workerId -ErrorAction SilentlyContinue
            if($process) {
                $now=$watch.Elapsed.TotalSeconds
                $cpu=$process.TotalProcessorTime.TotalSeconds
                $cpuPercent=if($null -ne $lastCpu){100*($cpu-$lastCpu)/($now-$lastTime)}else{$null}
                $gpuMemory=$null; $gpuEngine=$null; $counterError=$null
                try {
                    $gpuMemory=@(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory -Filter "Name LIKE 'pid_${workerId}_%'")
                    $gpuEngine=@(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine -Filter "Name LIKE 'pid_${workerId}_%'")
                } catch {$counterError=$_.Exception.Message}
                @{seconds=$now;pid=$workerId;working_set=$process.WorkingSet64;private_bytes=$process.PrivateMemorySize64;
                    cpu_one_core_percent=$cpuPercent;
                    gpu_engine_max_percent=if($gpuEngine.Count){($gpuEngine | Measure-Object UtilizationPercentage -Maximum).Maximum}else{$null};
                    gpu_dedicated_bytes=if($gpuMemory.Count){($gpuMemory | Measure-Object DedicatedUsage -Sum).Sum}else{$null};
                    gpu_shared_bytes=if($gpuMemory.Count){($gpuMemory | Measure-Object SharedUsage -Sum).Sum}else{$null};
                    gpu_counter_error=$counterError} | ConvertTo-Json -Compress | Add-Content -LiteralPath "$outputPath.resources.jsonl" -Encoding utf8
                $lastCpu=$cpu; $lastTime=$now
            }
        }
        Start-Sleep -Seconds 2
        $probe.Refresh()
    }
    $probe.WaitForExit()
    if($probe.ExitCode -ne 0){throw "Probe failed: $($probe.ExitCode); see $outputPath.stderr"}
} finally {
    if(-not $probe.HasExited) {
        if($workerId){Stop-Process -Id $workerId -ErrorAction SilentlyContinue}
        Stop-Process -Id $probe.Id -ErrorAction SilentlyContinue
    }
}
Write-Output "Completed $Device / ${Loading}: $outputPath"
