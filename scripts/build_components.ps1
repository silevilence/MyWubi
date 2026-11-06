param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$OnnxRuntime,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Tag,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $SkipBuild) {
    & (Join-Path $root 'model_input/build.ps1') -Release -WorkerOnly -OnnxRuntime $OnnxRuntime
}
$stage = Join-Path $root ('target/component-stage-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
Copy-Item -LiteralPath (Join-Path $root 'target/release/model-worker.exe') -Destination $stage
& (Join-Path $PSScriptRoot 'stage_model_runtime.ps1') -Destination $stage -OnnxRuntime $OnnxRuntime
Copy-Item -LiteralPath (Join-Path $root 'model_input/licenses') -Destination (Join-Path $stage 'model-licenses') -Recurse
$arguments = @((Join-Path $PSScriptRoot 'package_components.py'), '--directory', $stage, '--output', $OutputDirectory, '--version', $Version)
if ($Tag) { $arguments += @('--tag', $Tag) }
python @arguments
if ($LASTEXITCODE -ne 0) { throw 'Component packaging or native dependency validation failed' }
