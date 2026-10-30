param(
    [string]$Sdk = $env:ANDROID_HOME,
    [string]$Jdk = $env:JAVA_HOME,
    [switch]$DeviceTests
)
$ErrorActionPreference = 'Stop'
if (-not $Sdk) { $Sdk = 'C:\Program Files (x86)\Android\android-sdk' }
if (-not $Jdk) { $Jdk = 'C:\Program Files\Android\openjdk\jdk-21.0.8' }
if (-not (Test-Path -LiteralPath "$Sdk\ndk\27.2.12479018\source.properties")) { throw '请安装 Android NDK 27.2.12479018' }
if (-not (Test-Path -LiteralPath "$Jdk\bin\java.exe")) { throw '请通过 -Jdk 指定 JDK 21' }
$env:ANDROID_HOME = $Sdk
$env:JAVA_HOME = $Jdk
Push-Location $PSScriptRoot
try {
    $tasks = @(':app:assembleDebug', ':app:assembleDebugAndroidTest')
    if ($DeviceTests) { $tasks += ':app:connectedDebugAndroidTest' }
    & .\gradlew.bat @tasks --console=plain
    if ($LASTEXITCODE -ne 0) { throw "Gradle 失败：$LASTEXITCODE" }
} finally { Pop-Location }
