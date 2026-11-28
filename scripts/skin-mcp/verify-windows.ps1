$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,Accessibility,System.Drawing
Add-Type -Path C:\Browser-Source\scripts\interop.cs -ReferencedAssemblies Accessibility,System.Drawing
function Wait-For($test, $message) {
    $deadline = (Get-Date).AddSeconds(25)
    do {
        if (& $test) { return }
        Start-Sleep -Milliseconds 120
    } while ((Get-Date) -lt $deadline)
    throw $message
}
try {
    Wait-For { Test-Path C:\Browser-Results\host-pid.txt } 'Editor host did not start'
    $hostId = [int](Get-Content C:\Browser-Results\host-pid.txt)
    $window = [Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,$hostId))
    [BrowserAutomation]::SetForegroundWindow([IntPtr]$window.Current.NativeWindowHandle) | Out-Null
    function Control($name) {
        $window.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,$name))
    }
    $button = Control 'FocusEditor'
    ([Windows.Automation.InvokePattern]$button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke()
    $editor = Control 'Editor'
    $editor.SetFocus()
    Wait-For { $editor.Current.HasKeyboardFocus } 'Editor has no focus'
    [BrowserAutomation]::Key(65)
    Start-Sleep -Milliseconds 150
    [BrowserAutomation]::Key(66)
    Wait-For { try { [bool]([BrowserAutomation]::Names([BrowserAutomation]::Snapshot()) | Where-Object { $_.Contains('以及') }) } catch { $false } } 'TSF candidate missing'
    [BrowserAutomation]::Capture('C:\Browser-Results\windows.png')
    $snapshot = [BrowserAutomation]::Snapshot()
    $name = [BrowserAutomation]::Names($snapshot) | Where-Object { $_.Contains('以及') } | Select-Object -First 1
    [BrowserAutomation]::Click($snapshot,[BrowserAutomation]::Find($snapshot,$name))
    $value = [Windows.Automation.ValuePattern]$editor.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
    Wait-For { $value.Current.Value -ceq '以及' -and $editor.Current.HasKeyboardFocus } 'Unexpected host text or focus'
    $pattern = [Windows.Automation.TextPattern]$editor.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
    $caret = $pattern.GetSelection()[0]
    $prefix = $pattern.DocumentRange.Clone()
    $prefix.MoveEndpointByRange([Windows.Automation.TextPatternRangeEndpoint]::End,$caret,[Windows.Automation.TextPatternRangeEndpoint]::Start)
    if ($caret.GetText(-1) -ne '' -or $prefix.GetText(-1) -cne '以及') { throw 'Host caret did not collapse at end of commit' }
    $manifest = Get-Content -Raw -Encoding UTF8 C:\Browser-Source\job.json | ConvertFrom-Json
    $loaded = Get-FileHash -LiteralPath ("C:\Browser-App\skins\" + $manifest.file)
    if ($loaded.Hash.ToLowerInvariant() -ne $manifest.sha256) { throw 'Guest skin hash differs' }
    $logs = Get-ChildItem C:\Browser-App -Recurse -Filter debug.log
    foreach ($log in $logs) {
        if (Select-String -LiteralPath $log.FullName -SimpleMatch '[skin] 初始皮肤加载失败' -Quiet) {throw 'TSF fell back to the default skin'}
    }
    @{text=$value.Current.Value; focus=$editor.Current.HasKeyboardFocus; selectionStart=2; selectionEnd=2; sha256=$manifest.sha256} | ConvertTo-Json | Set-Content C:\Browser-Results\assertions.json -Encoding UTF8
    'passed' | Set-Content C:\Browser-Results\passed.txt
} catch {
    $_ | Out-String | Set-Content C:\Browser-Results\failure.txt
    exit 1
}
