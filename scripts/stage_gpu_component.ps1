param(
    [Parameter(Mandatory)][string]$Worker,
    [Parameter(Mandatory)][ValidateSet('vulkan', 'cuda')][string]$Backend,
    [Parameter(Mandatory)][string]$Destination,
    [Parameter(Mandatory)][string]$OnnxRuntime,
    [string]$CudaRuntimeDirectory,
    [string]$CudaLicense,
    [string]$CudaArchitectures
)
$ErrorActionPreference = 'Stop'
$workerPath = (Resolve-Path -LiteralPath $Worker).Path
$destinationPath = [IO.Path]::GetFullPath($Destination)
if (Test-Path -LiteralPath $destinationPath) { throw 'Use a new component directory to avoid mixing backends or stale dependencies' }
$cudaFiles = @()
if ($Backend -eq 'cuda') {
    if (-not $CudaRuntimeDirectory -or -not $CudaLicense -or -not (Test-Path -LiteralPath $CudaLicense -PathType Leaf)) { throw 'CUDA distribution requires its runtime directory and the matching NVIDIA redistribution license notice' }
    if ($CudaArchitectures -notmatch '^\d+(;\d+)*$') { throw 'Record the exact SM architectures used for this CUDA build' }
    foreach ($pattern in @('cudart64_*.dll', 'cublas64_*.dll', 'cublasLt64_*.dll')) {
        $cudaMatches = @(Get-ChildItem -LiteralPath $CudaRuntimeDirectory -Filter $pattern -File)
        if ($cudaMatches.Count -ne 1) { throw "Require exactly one matching runtime for $pattern" }
        $cudaFiles += $cudaMatches
    }
}
$bytes = [IO.File]::ReadAllBytes($workerPath)
$pe = [BitConverter]::ToInt32($bytes, 0x3c)
if ($bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a -or [BitConverter]::ToUInt32($bytes, $pe) -ne 0x4550 -or [BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664) {
    throw 'The optional Windows component must be PE x64'
}
New-Item -ItemType Directory -Path $destinationPath | Out-Null
Copy-Item -LiteralPath $workerPath -Destination (Join-Path $destinationPath 'model-worker.exe')
& (Join-Path $PSScriptRoot 'stage_model_runtime.ps1') -Destination $destinationPath -OnnxRuntime $OnnxRuntime
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../model_input/licenses') -Destination (Join-Path $destinationPath 'model-licenses') -Recurse
foreach ($file in $cudaFiles) { Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $destinationPath $file.Name) }
if ($Backend -eq 'cuda') { Copy-Item -LiteralPath $CudaLicense -Destination (Join-Path $destinationPath 'model-licenses/NVIDIA-CUDA-LICENSE.txt') }
$output = Join-Path $destinationPath 'features.json'
$process = Start-Process -FilePath (Join-Path $destinationPath 'model-worker.exe') -ArgumentList '--features' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput $output
if ($process.ExitCode -ne 0) { throw "Component startup failed: $($process.ExitCode); retain the CPU component" }
$features = Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
if (-not $features.$Backend -or -not $features.llama -or -not $features.onnx) { throw 'Component capabilities do not match requested package' }
$records = Get-ChildItem -LiteralPath $destinationPath -File -Recurse | ForEach-Object {
    @{ path = [IO.Path]::GetRelativePath($destinationPath, $_.FullName); bytes = $_.Length; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
}
@{ schema = 1; backend = $Backend; architecture = 'x86_64'; llama_cpp = '0.1.161';
    cuda_architectures = $CudaArchitectures;
    system_dependency = $(if ($Backend -eq 'vulkan') { 'GPU vendor Vulkan driver and vulkan-1.dll; SDK is build-only' } else { 'Compatible NVIDIA driver/nvcuda.dll; CUDA Toolkit is build-only. Runtime DLLs and license are included.' });
    cpu_fallback = 'Recoverable native failures use the included CPU backend. Keep the separate CPU component if this executable cannot start.';
    files = @($records) } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $destinationPath 'gpu-component.json') -Encoding utf8
