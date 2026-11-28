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
    $prefix.MoveEndpointByRange([Windows.Automation.Text.TextPatternRangeEndpoint]::End,$caret,[Windows.Automation.Text.TextPatternRangeEndpoint]::Start)
    $start = $prefix.GetText(-1).Length
    $end = $start + $caret.GetText(-1).Length
    if ($start -ne 2 -or $end -ne 2) { throw "Host caret did not collapse at end of commit ($start, $end)" }
    $manifest = Get-Content -Raw -Encoding UTF8 C:\Browser-Source\job.json | ConvertFrom-Json
    $loaded = Get-FileHash -LiteralPath ("C:\Browser-App\skins\" + $manifest.file)
    if ($loaded.Hash.ToLowerInvariant() -ne $manifest.sha256) { throw 'Guest skin hash differs' }
    $identity = [IO.Path]::GetFileNameWithoutExtension($manifest.file)
    $expected = "[skin] loaded pid=$hostId package=$identity component=$($manifest.windowsComponent)"
    $log = 'C:\Browser-App\log\im_engine.log'
    Wait-For { (Test-Path $log) -and (Select-String -LiteralPath $log -Encoding UTF8 -SimpleMatch $expected -Quiet) } 'The editor host did not load the expected skin'
    $expected | Set-Content C:\Browser-Results\loaded-skin.txt -Encoding UTF8
    @{text=$value.Current.Value; focus=$editor.Current.HasKeyboardFocus; selectionStart=$start; selectionEnd=$end; sha256=$manifest.sha256} | ConvertTo-Json | Set-Content C:\Browser-Results\assertions.json -Encoding UTF8
    'passed' | Set-Content C:\Browser-Results\passed.txt
} catch {
    $_ | Out-String | Set-Content C:\Browser-Results\failure.txt
    exit 1
}
