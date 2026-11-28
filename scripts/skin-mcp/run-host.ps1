param([ValidateSet('windows','android')][string]$Platform)
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$job = Get-Content -Raw -Encoding UTF8 -LiteralPath "$root\job.json" | ConvertFrom-Json
$out = "$root\$Platform"
New-Item -ItemType Directory -Path $out | Out-Null
function Write-Report($report) {
    $pending = Join-Path $out 'report.pending'
    [IO.File]::WriteAllText($pending,($report | ConvertTo-Json -Depth 4),[Text.UTF8Encoding]::new($false))
    [IO.File]::Move($pending,(Join-Path $out 'report.json'))
}
function Skip([string]$Reason) { Write-Report @{status='not_run';reason=$Reason} }
function Test-Windows {
    $sandbox = Get-Command WindowsSandbox.exe -ErrorAction SilentlyContinue
    if (!$sandbox) { Skip 'Windows Sandbox is unavailable'; return }
    if (!(Test-Path -LiteralPath "$root\app\im_engine.dll")) { Skip 'im_engine.dll was not found beside the editor'; return }
    if (Get-Process WindowsSandboxClient,WindowsSandboxRemoteSession -ErrorAction SilentlyContinue) { Skip 'Another Windows Sandbox is active'; return }
    $src = [Security.SecurityElement]::Escape($root)
    $dest = [Security.SecurityElement]::Escape($out)
    $xml = @"
<Configuration><Networking>Disable</Networking><MappedFolders><MappedFolder><HostFolder>$src</HostFolder><SandboxFolder>C:\Browser-Source</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder><MappedFolder><HostFolder>$dest</HostFolder><SandboxFolder>C:\Browser-Results</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder></MappedFolders><LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -STA -File C:\Browser-Source\scripts\guest.ps1</Command></LogonCommand></Configuration>
"@
    [IO.File]::WriteAllText("$root\skin.wsb",$xml)
    $instance = Start-Process -FilePath $sandbox.Source -ArgumentList ('"'+$root+'\skin.wsb"') -PassThru -WindowStyle Hidden
    $end = (Get-Date).AddMinutes(4)
    while ((Get-Date) -lt $end) {
        if (Test-Path "$out\passed.txt") { break }
        foreach ($name in 'failure.txt','startup-error.txt') {
            if (Test-Path "$out\$name") { throw (Get-Content -Raw "$out\$name") }
        }
        Start-Sleep -Milliseconds 500
    }
    if (!(Test-Path "$out\passed.txt")) { throw 'Timed out waiting for the Windows Sandbox host' }
}
function Test-Android {
    $adb = Get-Command adb.exe -ErrorAction SilentlyContinue
    if (!$adb -or !$job.serial) { Skip 'Select an adb test device by serial'; return }
    $serial = $job.serial
    $state = & $adb.Source devices
    if (!($state | Where-Object { ($_ -split '\s+')[0] -ceq $serial -and ($_ -split '\s+')[1] -eq 'device' })) { Skip "Device $serial is offline or unavailable"; return }
    function Adb {
        $result = & $adb.Source -s $serial @args
        if ($LASTEXITCODE -ne 0) { throw "adb failed: $result" }
        $result
    }
    if (!(Adb shell pm path org.mywubi.acceptancehost) -or !(Adb shell pm list instrumentation | Select-String 'org.mywubi.test/androidx.test.runner.AndroidJUnitRunner')) {
        Skip 'Install the MyWubi debug app, androidTest APK, and acceptance host on the test device'; return
    }
    $remote = "/sdcard/Android/data/org.mywubi/files/mcp-$($job.job)"
    $staged = "/data/local/tmp/mywubi-$($job.job).mywubi-skin"
    try {
        Adb push "$root\app\skins\$($job.file)" $staged | Out-Null
        $lines = Adb shell am instrument -w -r -e class 'org.mywubi.McpSkinHostTest#exportedSkinInExternalEditor' -e skinJob $job.job -e component $job.androidComponent -e sha256 $job.sha256 org.mywubi.test/androidx.test.runner.AndroidJUnitRunner
        $lines | Set-Content "$out\instrumentation.txt" -Encoding UTF8
        if (!($lines -match 'OK \(1 test\)')) { throw "Android host assertion failed: $($lines -join ' ')" }
        Adb pull "$remote/assertions.json" "$out\assertions.json" | Out-Null
        Adb pull "$remote/android.png" "$out\android.png" | Out-Null
    } finally {
        Adb shell rm -f $staged | Out-Null
        # Only this run's UUID directory is owned by this runner.
        foreach ($name in 'skin.mywubi-skin','main.dict','assertions.json','android.png') { Adb shell rm -f "$remote/$name" | Out-Null }
        Adb shell "if [ -d '$remote' ]; then rmdir '$remote'; fi" | Out-Null
    }
}
try {
    if ($Platform -eq 'android') { Test-Android } else { Test-Windows }
    if (!(Test-Path "$out\report.json")) {
        $assertions=Get-Content -Raw -Encoding UTF8 "$out\assertions.json" | ConvertFrom-Json
        if ($assertions.sha256 -ne $job.sha256 -or $assertions.text -cne '以及' -or !$assertions.focus -or $assertions.selectionStart -ne 2 -or $assertions.selectionEnd -ne 2 -or !(Test-Path "$out\$Platform.png")) { throw 'Incomplete host evidence' }
        Write-Report @{status='passed';assertions=$assertions;screenshot="$out\$Platform.png"}
    }
} catch {
    Write-Report @{status='failed';reason=$_.Exception.Message}
    exit 1
}
