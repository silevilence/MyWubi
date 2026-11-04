param(
    [ValidateSet('windows', 'android-arm64', 'android-x64')]
    [string]$Target = 'windows',
    [string]$Sdk = 'C:\Program Files (x86)\Android\android-sdk',
    [string]$OnnxRuntime = $env:MYWUBI_ORT_DYLIB,
    [switch]$Test
)
$ErrorActionPreference = 'Stop'
if ($Target -ne 'windows' -and -not $PSBoundParameters.ContainsKey('OnnxRuntime')) {
    # The host DLL environment must not become an Android library named .so.
    $OnnxRuntime = ''
    $env:MYWUBI_ORT_DYLIB = $null
}
if ($OnnxRuntime) { $env:MYWUBI_ORT_DYLIB = (Resolve-Path -LiteralPath $OnnxRuntime).Path }
$vswhere = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $vs = & $vswhere -latest -products '*' -property installationPath
    if (-not $vs) { throw 'CMake or Visual Studio C++ tools are required' }
    $cmakeTools = Join-Path $vs 'Common7\IDE\CommonExtensions\Microsoft\CMake'
    $env:PATH = "$cmakeTools\CMake\bin;$cmakeTools\Ninja;$env:PATH"
}
if (-not $env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }
if (-not (Test-Path -LiteralPath (Join-Path $env:LIBCLANG_PATH 'libclang.dll'))) { throw 'Set LIBCLANG_PATH to LLVM libclang directory' }
$cargoArgs = @('build', '-p', 'model_input', '--locked', '--features', 'llama-cpp-2,ort')
if ($Test) { $cargoArgs[0] = 'test' }
if ($Target -ne 'windows') {
    if ($Test) { throw 'Cross-built tests require an Android device runner; do not count build as execution' }
    $ndk = Join-Path $Sdk 'ndk\27.2.12479018'
    if (-not (Test-Path -LiteralPath (Join-Path $ndk 'source.properties'))) { throw 'NDK 27.2.12479018 is required' }
    $triple = if ($Target -eq 'android-arm64') { 'aarch64-linux-android' } else { 'x86_64-linux-android' }
    $ndkBin = Join-Path $ndk 'toolchains\llvm\prebuilt\windows-x86_64\bin'
    $env:ANDROID_NDK = $ndk
    $env:ANDROID_API_LEVEL = '26'
    $env:CMAKE_GENERATOR = 'Ninja'
    $env:PATH = "$ndkBin;$env:PATH"
    $targetKey = $triple.Replace('-', '_').ToUpperInvariant()
    $ccTarget = $triple.Replace('-', '_')
    [Environment]::SetEnvironmentVariable("CFLAGS_$ccTarget", "--target=${triple}26", 'Process')
    [Environment]::SetEnvironmentVariable("CXXFLAGS_$ccTarget", "--target=${triple}26", 'Process')
    $env:CMAKE_BUILD_PARALLEL_LEVEL = '4'
    [Environment]::SetEnvironmentVariable("CARGO_TARGET_${targetKey}_LINKER", (Join-Path $ndkBin 'clang.exe'), 'Process')
    [Environment]::SetEnvironmentVariable("CARGO_TARGET_${targetKey}_RUSTFLAGS", "-C link-arg=--target=${triple}26 -C link-arg=-Wl,-z,max-page-size=16384", 'Process')
    $cargoArgs += @('--target', $triple, '--lib', '--features', 'llama-cpp-2/android-static-stdcxx')
}
Push-Location (Join-Path $PSScriptRoot '..')
try {
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "Native model build failed: $LASTEXITCODE" }
} finally { Pop-Location }
