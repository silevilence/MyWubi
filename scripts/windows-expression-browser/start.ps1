$ErrorActionPreference = 'Stop'
try {
    New-Item -ItemType Directory -Path C:\Browser-App | Out-Null
    Copy-Item -Path C:\Browser-Source\app\* -Destination C:\Browser-App -Recurse
    $register = Start-Process C:\Windows\System32\regsvr32.exe -ArgumentList '/s C:\Browser-App\im_engine.dll' -PassThru -Wait -WindowStyle Hidden
    if ($register.ExitCode -ne 0) {throw "Registration failed: $($register.ExitCode)"}
    $tip = '0804:{C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD8}{C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD9}'
    $languages = New-WinUserLanguageList 'zh-CN'
    $languages[0].InputMethodTips.Clear()
    $languages[0].InputMethodTips.Add($tip)
    $languages.Add('en-US')
    Set-WinUserLanguageList $languages -Force
    Set-WinDefaultInputMethodOverride -InputTip $tip
    Start-Process C:\Windows\System32\ctfmon.exe -WindowStyle Hidden
    Get-FileHash C:\Browser-App\im_engine.dll | ConvertTo-Json | Set-Content C:\Browser-Results\dll.json
    $testHost = Start-Process powershell.exe -ArgumentList '-NoProfile -ExecutionPolicy RemoteSigned -STA -File C:\Browser-Source\scripts\host.ps1' -WindowStyle Hidden -PassThru
    $verification = Start-Process powershell.exe -ArgumentList '-NoProfile -ExecutionPolicy RemoteSigned -STA -File C:\Browser-Source\scripts\verify.ps1' -WindowStyle Hidden -Wait -PassThru
    if ($verification.ExitCode -ne 0) {throw 'UI automation failed; see failure.txt and checks.json.'}
} catch {$_ | Out-String | Set-Content C:\Browser-Results\startup-error.txt}
# Always stop the test host. A successful run also closes Sandbox automatically.
if ($testHost) {Stop-Process -Id $testHost.Id -ErrorAction SilentlyContinue}
if (Test-Path C:\Browser-Results\passed.txt) {shutdown.exe /s /t 0}
