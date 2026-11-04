param(
    [string]$Sdk = 'C:\Program Files (x86)\Android\android-sdk',
    [string[]]$Tasks = @(':app:assembleDebug', ':app:testDebugUnitTest', ':app:assembleDebugAndroidTest')
)
$ErrorActionPreference = 'Stop'
$env:ANDROID_HOME = (Resolve-Path -LiteralPath $Sdk).Path
$env:ANDROID_NDK = Join-Path $env:ANDROID_HOME 'ndk\27.2.12479018'
if (-not (Test-Path -LiteralPath (Join-Path $env:ANDROID_NDK 'source.properties'))) { throw 'NDK 27.2.12479018 is required' }
$env:ANDROID_API_LEVEL = '26'
$env:MYWUBI_ORT_DYLIB = $null # Android runtime comes from the fixed official AAR.
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $vswhere = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio\Installer\vswhere.exe'
    $vs = & $vswhere -latest -products '*' -property installationPath
    $cmakeTools = Join-Path $vs 'Common7\IDE\CommonExtensions\Microsoft\CMake'
    $env:PATH = "$cmakeTools\CMake\bin;$cmakeTools\Ninja;$env:PATH"
}
if (-not $env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }
$env:CMAKE_GENERATOR = 'Ninja'
$env:CMAKE_BUILD_PARALLEL_LEVEL = '4'
$env:CARGO_BUILD_JOBS = '4'
foreach ($triple in @('aarch64-linux-android', 'x86_64-linux-android')) {
    $key = $triple.Replace('-', '_')
    [Environment]::SetEnvironmentVariable("CFLAGS_$key", "--target=${triple}26", 'Process')
    [Environment]::SetEnvironmentVariable("CXXFLAGS_$key", "--target=${triple}26", 'Process')
}
Push-Location $PSScriptRoot
try {
    & ./gradlew.bat '-PmodelBackends=true' @Tasks
    if ($LASTEXITCODE -ne 0) { throw "Model-enabled Android build failed: $LASTEXITCODE" }
} finally { Pop-Location }
