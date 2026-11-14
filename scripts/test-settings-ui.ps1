<#
.SYNOPSIS
Runs settings and skin-editor operations against local fixtures, without clicking.
.EXAMPLE
./scripts/test-settings-ui.ps1
.EXAMPLE
./scripts/test-settings-ui.ps1 -Capture
#>
param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [switch]$Capture,
    [string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$binaryDir = Join-Path $repo "target\$Profile"
foreach ($name in @('settings.exe', 'skin_editor.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $binaryDir $name))) { throw "Missing $name. Run cargo build -p settings --offline --locked first." }
}
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repo ("target\settings-verification-" + [guid]::NewGuid().ToString('N')) }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Use a new output directory to preserve prior evidence.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
& (Join-Path $PSScriptRoot 'copy-expression-bundle.ps1') -Destination (Join-Path $OutputDirectory 'expressions')
$config = Join-Path $OutputDirectory 'config.toml'
$table = Join-Path $OutputDirectory 'test.dict'
@'
---
display_name: UI regression fixture
type: shape
charset: abcdefghijklmnopqrstuvwxyz
max_code_len: 4
---
a	工	1
aa	式	1
'@ | Set-Content -LiteralPath $table -Encoding utf8NoBOM
@"
[dictionary]
system_table = '$table'
user_table = 'user.dict'
"@ | Set-Content -LiteralPath $config -Encoding utf8NoBOM

function Invoke-OwnedProcess([string]$Name, [string[]]$Arguments, [string]$Label) {
    $quoted = @($Arguments | ForEach-Object { '"' + $_ + '"' })
    $process = Start-Process -FilePath (Join-Path $binaryDir $Name) -ArgumentList $quoted -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $OutputDirectory "$Label.stderr.txt")
    try {
        if (-not $process.WaitForExit(55000)) { throw "$Label timed out" }
        if ($process.ExitCode -ne 0) { throw "$Label failed with exit code $($process.ExitCode); inspect output in $OutputDirectory" }
    } finally {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -ErrorAction Stop }
        $process.Dispose()
    }
}

function Invoke-Actions([string]$Name, [object[]]$Actions, [string]$Label) {
    $request = Join-Path $OutputDirectory "$Label.request.json"
    $report = Join-Path $OutputDirectory "$Label.report.json"
    @{ config = $config; actions = $Actions } | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $request -Encoding utf8NoBOM
    Invoke-OwnedProcess $Name @('--automation', $request, '--output', $report) $Label
    $result = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
    if (-not $result.ok) { throw $result.error }
    return $result
}

$result = Invoke-Actions 'settings.exe' @(
    @{ action = 'navigate'; page = 'basic' },
    @{ action = 'set'; field = 'basic.candidate_count'; value = 7 },
    @{ action = 'set'; field = 'basic.switch_key'; value = 'ctrl_space' },
    @{ action = 'set'; field = 'appearance.font_size'; value = 20 },
    @{ action = 'save' }, @{ action = 'reload'; discard = $false }
) 'settings'
if ($result.results[-1].config.basic.candidate_count -ne 7 -or $result.results[-1].dirty) { throw 'Settings round trip failed' }
$skinFile = Join-Path $OutputDirectory 'test.mywubi-skin'
$result = Invoke-Actions 'skin_editor.exe' @(
    @{ action = 'skin_new'; preset = 'jade' },
    @{ action = 'skin_set'; platform = 'windows'; component = 'jade'; field = 'font_size'; value = 24 },
    @{ action = 'skin_export'; path = $skinFile },
    @{ action = 'skin_apply' }, @{ action = 'skin_discard' },
    @{ action = 'skin_open'; path = $skinFile }
) 'skin'
if ($result.results[-1].skin_dirty -or $result.results[-1].config.basic.candidate_count -ne 7) { throw 'Skin workflow failed' }

if ($Capture) {
    foreach ($name in @('settings', 'skin_editor')) {
        $captureDir = Join-Path $OutputDirectory $name
        Invoke-OwnedProcess "$name.exe" @('--config', $config, '--capture-ui', $captureDir) "$name-capture"
        $expected = if ($name -eq 'settings') { 9 } else { 6 }
        $images = @(Get-ChildItem -LiteralPath $captureDir -Filter '*.png')
        if ($images.Count -ne $expected -or @($images | Where-Object Length -LT 100).Count -gt 0) { throw "$name produced $($images.Count) captures; expected $expected" }
    }
}
Write-Host "Passed. Local fixtures only; no resources downloaded. Evidence: $OutputDirectory"
