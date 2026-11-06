param(
    [Parameter(Mandatory)][string]$Worker,
    [Parameter(Mandatory)][ValidateSet('vulkan')][string]$Backend,
    [Parameter(Mandatory)][string]$Destination,
    [Parameter(Mandatory)][string]$OnnxRuntime
)
$ErrorActionPreference = 'Stop'
$workerPath = (Resolve-Path -LiteralPath $Worker).Path
$destinationPath = [IO.Path]::GetFullPath($Destination)
if (Test-Path -LiteralPath $destinationPath) { throw 'Use a new component directory to avoid mixing backends or stale dependencies' }
$bytes = [IO.File]::ReadAllBytes($workerPath)
$pe = [BitConverter]::ToInt32($bytes, 0x3c)
if ($bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a -or [BitConverter]::ToUInt32($bytes, $pe) -ne 0x4550 -or [BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664) {
    throw 'The optional Windows component must be PE x64'
}
New-Item -ItemType Directory -Path $destinationPath | Out-Null
Copy-Item -LiteralPath $workerPath -Destination (Join-Path $destinationPath 'model-worker.exe')
& (Join-Path $PSScriptRoot 'stage_model_runtime.ps1') -Destination $destinationPath -OnnxRuntime $OnnxRuntime
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../model_input/licenses') -Destination (Join-Path $destinationPath 'model-licenses') -Recurse
$output = Join-Path $destinationPath 'features.json'
$process = Start-Process -FilePath (Join-Path $destinationPath 'model-worker.exe') -ArgumentList '--features' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput $output
if ($process.ExitCode -ne 0) { throw "Component startup failed: $($process.ExitCode); retain the CPU component" }
$features = Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
if (-not $features.$Backend -or -not $features.llama -or -not $features.onnx) { throw 'Component capabilities do not match requested package' }
$records = Get-ChildItem -LiteralPath $destinationPath -File -Recurse | ForEach-Object {
    @{ path = [IO.Path]::GetRelativePath($destinationPath, $_.FullName); bytes = $_.Length; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
}
@{ schema = 1; backend = $Backend; architecture = 'x86_64'; llama_cpp = '0.1.161';
    system_dependency = 'GPU vendor Vulkan driver and vulkan-1.dll; SDK is build-only';
    cpu_fallback = 'Recoverable native failures use the included CPU backend. Keep the separate CPU component if this executable cannot start.';
    files = @($records) } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $destinationPath 'gpu-component.json') -Encoding utf8
