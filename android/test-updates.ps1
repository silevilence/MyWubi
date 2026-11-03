param(
    [string]$Sdk = 'C:\Program Files (x86)\Android\android-sdk',
    [string]$Jdk = 'C:\Program Files\Android\openjdk\jdk-21.0.8',
    [string]$Serial = 'emulator-5554',
    [int]$BaseVersionCode = 0,
    [switch]$InstallUpgrade
)
$ErrorActionPreference = 'Stop'
if ($Serial -notmatch '^emulator-\d+$') { throw '此验收脚本仅用于测试模拟器，不能用于个人真机' }
$env:ANDROID_HOME = $Sdk
$env:JAVA_HOME = $Jdk
$adb = Join-Path $Sdk 'platform-tools/adb.exe'
function Invoke-Checked([string]$Executable, [string[]]$CommandArgs) {
    & $Executable @CommandArgs
    if ($LASTEXITCODE -ne 0) { throw "$Executable exited with $LASTEXITCODE" }
}
Push-Location $PSScriptRoot
try {
    if ($BaseVersionCode -eq 0) {
        $gradle = Get-Content -Raw -LiteralPath 'app/build.gradle'
        $match = [regex]::Match($gradle, "versionCode.*getOrElse\('([0-9]+)'\)")
        if (-not $match.Success) { throw 'Cannot resolve the default versionCode' }
        $BaseVersionCode = [int]$match.Groups[1].Value
    }
    $nextCode = $BaseVersionCode + 1
    Invoke-Checked $adb @('-s', $Serial, 'get-state')
    Invoke-Checked '.\gradlew.bat' @(':app:assembleDebug', ':app:assembleDebugAndroidTest', "-PversionCode=$nextCode", '--console=plain')
    $fixtures = 'app/build/au-fixtures'
    New-Item -ItemType Directory -Path $fixtures -ErrorAction SilentlyContinue | Out-Null
    Copy-Item -LiteralPath 'app/build/outputs/apk/debug/app-debug.apk' -Destination "$fixtures/au-new.apk"
    Copy-Item -LiteralPath 'app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk' -Destination "$fixtures/au-wrong-package.apk"
    $key = "$fixtures/wrong-signature.jks"
    if (-not (Test-Path -LiteralPath $key)) {
        Invoke-Checked (Join-Path $Jdk 'bin/keytool.exe') @('-genkeypair', '-keystore', $key, '-storepass', 'android', '-keypass', 'android', '-alias', 'fixture', '-dname', 'CN=AU Test Fixture', '-keyalg', 'RSA', '-validity', '365')
    }
    Invoke-Checked (Join-Path $Sdk 'build-tools/35.0.0/apksigner.bat') @('sign', '--ks', $key, '--ks-key-alias', 'fixture', '--ks-pass', 'pass:android', '--key-pass', 'pass:android', '--out', "$fixtures/au-wrong-signature.apk", "$fixtures/au-new.apk")
    Invoke-Checked '.\gradlew.bat' @(':app:assembleDebug', ':app:assembleDebugAndroidTest', ':app:testDebugUnitTest', ':app:lintDebug', "-PversionCode=$BaseVersionCode", '--console=plain')
    Copy-Item -LiteralPath 'app/build/outputs/apk/debug/app-debug.apk' -Destination "$fixtures/au-old.apk"
    foreach ($name in @('au-new', 'au-old', 'au-wrong-package', 'au-wrong-signature')) {
        Invoke-Checked $adb @('-s', $Serial, 'push', "$fixtures/$name.apk", "/data/local/tmp/$name.apk")
    }
    Invoke-Checked $adb @('-s', $Serial, 'install', '-r', 'app/build/outputs/apk/debug/app-debug.apk')
    Invoke-Checked $adb @('-s', $Serial, 'install', '-r', 'app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk')
    # Android kills the target process when this permission is revoked; do so before instrumentation.
    Invoke-Checked $adb @('-s', $Serial, 'shell', 'appops', 'set', 'org.mywubi', 'REQUEST_INSTALL_PACKAGES', 'deny')
    $tests = 'org.mywubi.AppUpdateTest,org.mywubi.UpdateToolbarTest,org.mywubi.UpdateDownloadTest,org.mywubi.UpdateIsolationTest,org.mywubi.UpdateInstallTest#permissionDenialAndInstallerCancellationRemainRecoverable'
    $result = & $adb -s $Serial shell am instrument -w -e class $tests -e update_fixtures true org.mywubi.test/androidx.test.runner.AndroidJUnitRunner
    $result | Set-Content -LiteralPath 'app/build/au-fixtures/device-tests.log'
    $result
    if ($LASTEXITCODE -ne 0 -or ($result -join "`n") -notmatch 'OK \(\d+ tests?\)') { throw 'Android update tests failed' }
    & $adb -s $Serial shell am instrument -w -e class 'org.mywubi.UpdateIdleHostTest#seedCachedUpdate' -e seed_cache true org.mywubi.test/androidx.test.runner.AndroidJUnitRunner
    Invoke-Checked $adb @('-s', $Serial, 'shell', 'am', 'force-stop', 'org.mywubi')
    $idleResult = & $adb -s $Serial shell am instrument -w -e class 'org.mywubi.UpdateIdleHostTest#realImeDefersCachedUpdateUntilIdleAcrossRotationAndRecreation' -e test_idle true org.mywubi.test/androidx.test.runner.AndroidJUnitRunner
    $idleResult
    if (($idleResult -join "`n") -notmatch 'OK \(1 test\)') { throw 'Real IME idle toolbar verification failed' }
    if ($InstallUpgrade) {
        # The target process is intentionally replaced here; verify using a separate invocation.
        & $adb -s $Serial shell am instrument -w -e class 'org.mywubi.UpdateInstallTest#completeSystemUpgrade' -e perform_upgrade true org.mywubi.test/androidx.test.runner.AndroidJUnitRunner
        for ($attempt = 0; $attempt -lt 30; $attempt++) {
            $packageState = (& $adb -s $Serial shell dumpsys package org.mywubi) -join "`n"
            if ($packageState -match "versionCode=$nextCode\b") { break }
            Start-Sleep -Milliseconds 500
        }
        if ($packageState -notmatch "versionCode=$nextCode\b") { throw 'System installation did not finish' }
        $verification = & $adb -s $Serial shell am instrument -w -e class 'org.mywubi.UpdateInstallTest#upgradePreservedUserData' -e verify_upgrade true org.mywubi.test/androidx.test.runner.AndroidJUnitRunner
        $verification
        if (($verification -join "`n") -notmatch 'OK \(1 test\)') { throw 'Post-upgrade verification failed' }
    }
} finally { Pop-Location }
