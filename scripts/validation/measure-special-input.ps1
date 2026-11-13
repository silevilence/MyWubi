param([Parameter(Mandatory = $true)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$executable = Join-Path $repo 'target/release/examples/special_input_benchmark.exe'
$destination = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath (Join-Path $destination 'core-run-1.json')) {
    throw 'Choose a new output directory; measurements must not overwrite earlier runs.'
}
New-Item -ItemType Directory -Path $destination -ErrorAction SilentlyContinue | Out-Null
$records = @()
foreach ($run in 1..3) {
    $cpuBefore = (Get-CimInstance Win32_Processor).LoadPercentage
    $process = Start-Process -FilePath $executable -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput (Join-Path $destination "core-run-$run.json") `
        -RedirectStandardError (Join-Path $destination "core-run-$run.stderr.txt")
    $peak = 0L
    while (-not $process.HasExited) {
        $process.Refresh()
        $peak = [Math]::Max($peak, $process.PeakWorkingSet64)
        Start-Sleep -Milliseconds 20
    }
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw "Core benchmark run $run failed" }
    $records += @{
        run = $run; peak_working_set_bytes = $peak
        cpu_load_before_percent = $cpuBefore
        cpu_load_after_percent = (Get-CimInstance Win32_Processor).LoadPercentage
    }
    Write-Output "Completed run $run; peak $peak bytes"
}
$files = @('core_engine/examples/special_input_benchmark.rs', 'core_engine/src/dictionary.rs',
    'core_engine/src/runtime.rs', 'docs/validation/special-input/cases.json',
    'docs/validation/special-input/workloads.json', 'tables/wubi86.dict', 'tables/pinyin.dict', 'tables/english.dict')
$files += Get-ChildItem -LiteralPath (Join-Path $repo 'expressions') -Filter '*.mwe' | ForEach-Object { "expressions/$($_.Name)" }
$hashes = @{}
foreach ($file in $files) { $hashes[$file] = (Get-FileHash -LiteralPath (Join-Path $repo $file) -Algorithm SHA256).Hash.ToLowerInvariant() }
@{
    date = (Get-Date -Format o); compiler = (& rustc --version)
    executable_sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    source_and_resource_sha256 = $hashes; runs = $records
    scope = 'Windows Release core; OS cache retained; no JNI, UI or actual host; PeakWorkingSet64 sampled every 20 ms'
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $destination 'core-memory.json') -Encoding utf8
