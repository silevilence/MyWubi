$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,Accessibility,System.Drawing
Add-Type -Path C:\Browser-Source\scripts\interop.cs -ReferencedAssemblies Accessibility,System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class FrequencyProbe {
    [StructLayout(LayoutKind.Sequential)] struct Buffer { public IntPtr data; public ulong len, allocation; }
    [DllImport(@"C:\Browser-App\core_engine.dll")] static extern uint mywubi_v1_call(byte[] data,ulong len,out Buffer result);
    [DllImport(@"C:\Browser-App\core_engine.dll")] static extern uint mywubi_v1_buffer_free(ulong allocation);
    [DllImport("user32.dll")] static extern void keybd_event(byte vk,byte scan,uint flags,UIntPtr extra);
    [StructLayout(LayoutKind.Sequential)] struct Rect { public int Left,Top,Right,Bottom; }
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr FindWindow(string cls,string title);
    [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr hwnd,uint msg,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] static extern bool GetMenuItemRect(IntPtr hwnd,IntPtr menu,uint item,out Rect rect);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetMenuString(IntPtr menu,uint item,StringBuilder text,int max,uint flags);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
    [DllImport("user32.dll")] static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hwnd,IntPtr after,int x,int y,int width,int height,uint flags);
    public static void PlaceHost(IntPtr hwnd,bool second) {
        SetWindowPos(hwnd,IntPtr.Zero,40,second?300:20,700,270,0x40);
    }
    public static void ClickPoint(int x,int y) {
        SetCursorPos(x,y);
        mouse_event(2,0,0,0,UIntPtr.Zero); mouse_event(4,0,0,0,UIntPtr.Zero);
    }
    public static bool RankMenu(bool click) {
        var window=FindWindow("#32768",null);
        if(window==IntPtr.Zero) return false;
        var menu=SendMessage(window,0x1e1,IntPtr.Zero,IntPtr.Zero);
        var text=new StringBuilder(100);
        Rect r;
        if(GetMenuString(menu,0,text,100,0x400)==0 || text.ToString()!="移到第 1 位" || !GetMenuItemRect(IntPtr.Zero,menu,0,out r)) return false;
        if(click) {
            SetCursorPos((r.Left+r.Right)/2,(r.Top+r.Bottom)/2);
            mouse_event(2,0,0,0,UIntPtr.Zero); mouse_event(4,0,0,0,UIntPtr.Zero);
        }
        return true;
    }
    public static void Control(byte key){ keybd_event(17,0,0,UIntPtr.Zero); BrowserKey(key); keybd_event(17,0,2,UIntPtr.Zero); }
    static void BrowserKey(byte key){ keybd_event(key,0,0,UIntPtr.Zero); keybd_event(key,0,2,UIntPtr.Zero); }
    public static string Call(string json){var bytes=Encoding.UTF8.GetBytes(json);Buffer b;if(mywubi_v1_call(bytes,(ulong)bytes.Length,out b)!=0)throw new Exception("protocol");try{var r=new byte[(int)b.len];Marshal.Copy(b.data,r,0,r.Length);return Encoding.UTF8.GetString(r);}finally{mywubi_v1_buffer_free(b.allocation);}}
}
"@
$report = [Collections.Generic.List[object]]::new()
function Until($condition,$description) {
    $end=(Get-Date).AddSeconds(25)
    do { if (& $condition) { return }; Start-Sleep -Milliseconds 80 } while ((Get-Date) -lt $end)
    throw $description
}
function EditorText { if (!$editor) { return "" }; ([Windows.Automation.ValuePattern]$editor.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).Current.Value }
function Report($row) { $report.Add($row); $report | ConvertTo-Json -Depth 6 | Set-Content C:\Browser-Results\frequency.json -Encoding UTF8 }
function Key($vk) { [BrowserAutomation]::Key($vk); Start-Sleep -Milliseconds 160 }
function Preview {
    $json = @{version=1;request=@{op='SaveFrequency';path='C:\Browser-App\config.toml'}} | ConvertTo-Json -Compress
    $r=[FrequencyProbe]::Call($json)|ConvertFrom-Json
    if (!$r.ok) { throw ($r|ConvertTo-Json) }
    $r.value
}
function Pick($word) {
    Key 65; Key 66
    Until { try { (@([BrowserAutomation]::Names([BrowserAutomation]::Snapshot())) -join '|').Contains($word) } catch { $false } } "no candidate $word"
    $snap=[BrowserAutomation]::Snapshot()
    $labels=[BrowserAutomation]::Names($snap)
    $target=$labels|Where-Object { $_.Contains($word) }|Select-Object -First 1
    [BrowserAutomation]::Invoke($snap,[BrowserAutomation]::Find($snap,$target))
    Until { (EditorText) -ceq $word } "commit expected '$word', got '$(EditorText)'"
    Start-Sleep -Milliseconds 260
}
function UseTable($name) {
    $file = "$run-$name.dict"
    Copy-Item C:\Browser-App\tables\main.dict "C:\Browser-App\tables\$file"
    [IO.File]::WriteAllText('C:\Browser-App\config.toml',$template.Replace('main.dict',$file),[Text.UTF8Encoding]::new($false))
    $path = "\\?\C:\Browser-App\tables\$file"
    $hasher = [Security.Cryptography.SHA256]::Create()
    try { $script:statsName = ([BitConverter]::ToString($hasher.ComputeHash([Text.Encoding]::UTF8.GetBytes($path)))).Replace('-','').ToLowerInvariant() + '.json' }
    finally { $hasher.Dispose() }
    Start-Sleep -Milliseconds 1000
}
function Stats {
    $base=Join-Path $env:LOCALAPPDATA 'MyWubi\portable'
    $file=Get-ChildItem $base -Filter $statsName -Recurse -ErrorAction SilentlyContinue | Where-Object DirectoryName -like '*\frequency' | Select-Object -First 1
    if ($file) { (Get-Content -Raw -Encoding UTF8 $file.FullName|ConvertFrom-Json).counts.ab }
}
function FocusHost($processId) {
    Until { $script:hostWindow=[Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,[int]$processId)); $null -ne $script:hostWindow } 'no WPF host'
    [FrequencyProbe]::PlaceHost([IntPtr]$hostWindow.Current.NativeWindowHandle,($processId -ne $first))
    [BrowserAutomation]::SetForegroundWindow([IntPtr]$hostWindow.Current.NativeWindowHandle) | Out-Null
    $script:editor=$hostWindow.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'Editor'))
    $button=$hostWindow.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'FocusEditor'))
    if ([BrowserAutomation]::GetForegroundWindow() -ne [IntPtr]$hostWindow.Current.NativeWindowHandle) {
        $rect=$button.Current.BoundingRectangle
        [FrequencyProbe]::ClickPoint([int]($rect.X+$rect.Width/2),[int]($rect.Y+$rect.Height/2))
        Start-Sleep -Milliseconds 150
    }
    ([Windows.Automation.InvokePattern]$button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke()
    $editor.SetFocus()
    Until { $editor.Current.HasKeyboardFocus } "editor in process $processId did not get focus"
}
try {
    Until { Test-Path C:\Browser-Results\host-pid.txt } 'host did not start'
    $first=if ($env:FQ_TEST_HOST) {[int]$env:FQ_TEST_HOST} else {[int](Get-Content C:\Browser-Results\host-pid.txt)}
    $run=[Guid]::NewGuid().ToString('N')
    $template=[IO.File]::ReadAllText('C:\Browser-Source\app\config.toml').Replace('user.dict',"$run-user.dict")
    foreach ($case in 'control','backspace','undo','replace') {
        try {
            UseTable $case
            FocusHost $first
            ([Windows.Automation.ValuePattern]$editor.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).SetValue('')
            Start-Sleep -Milliseconds 200
            if ($case -ne 'control') {
                Pick '以及'
                Until { (Stats).'以及' -eq 1 } "$case wrong not counted"
                switch ($case) {
                    backspace { Key 8; Until { (EditorText) -ceq '以' } 'partial delete'; Key 8; Until { (EditorText) -ceq '' } 'full delete' }
                    undo { [FrequencyProbe]::Control(90); Until { (EditorText) -ceq '' } "undo left '$(EditorText)'" }
                    replace { [BrowserAutomation]::CtrlA(); Start-Sleep -Milliseconds 250 }
                }
            }
            Pick '已经'
            Until { $s=Stats; [int]$s.'以及' -eq 0 -and [int]$s.'已经' -eq 1 } "$case counts: $(Stats|ConvertTo-Json -Compress)"
            $textPattern=[Windows.Automation.TextPattern]$editor.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
            $selection=$textPattern.GetSelection()[0]
            if (!$editor.Current.HasKeyboardFocus -or $selection.GetText(-1) -ne '' -or $selection.CompareEndpoints('Start',$textPattern.DocumentRange,'End') -ne 0) { throw 'focus or selection' }
            $preview=Preview
            if ($preview.count -ne 1) {throw "preview: $($preview|ConvertTo-Json)"}
            Key 65; Key 66
            $items=[BrowserAutomation]::Names([BrowserAutomation]::Snapshot()); Key 27
            $signature=($items -join '|')+' '+($preview.examples -join '|')
            if ($case -eq 'control') { $baseline=$signature } elseif ($baseline -cne $signature) {throw 'ranking or preview differs from control'}
            Report @{case=$case;passed=$true;text='已经';focus=$true;selection=@(2,2);counts=(Stats);order=$items;preview=$preview.examples}
        } catch { Report @{case=$case;passed=$false;reason=$_.Exception.Message;text=(EditorText)}; Key 27 }
    }
    try {
        FocusHost $first
        ([Windows.Automation.ValuePattern]$editor.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).SetValue('')
        Start-Sleep -Milliseconds 200
        Key 65; Key 66
        Until { [BrowserAutomation]::Palette() -ne [IntPtr]::Zero } "no candidate window for right click, editor shows '$(EditorText)'"
        $snap=[BrowserAutomation]::Snapshot()
        $name=[BrowserAutomation]::Names($snap)|Where-Object { $_.Contains('已经') }|Select-Object -First 1
        [BrowserAutomation]::RightClick($snap,[BrowserAutomation]::Find($snap,$name))
        Until { [FrequencyProbe]::RankMenu($false) } "right click did not show ranks"
        Key 27
        if (!(EditorText).Contains('ab') -or !$editor.Current.HasKeyboardFocus) {throw 'cancel lost preedit or focus'}
        [BrowserAutomation]::RightClick($snap,[BrowserAutomation]::Find($snap,$name))
        Until { [FrequencyProbe]::RankMenu($true) } "rank 1 menu action missing"
        Until { try { ([BrowserAutomation]::Names([BrowserAutomation]::Snapshot()))[0].Contains('已经') } catch { $false } } "confirm did not refresh first candidate"
        Key 49
        Until { (EditorText) -ceq '已经' } 'reordered first candidate did not commit'
        $userFile = "C:\Browser-App\tables\$run-user.dict"
        $saved = (Get-FileHash $userFile).Hash
        Key 65; Key 66
        $snap=[BrowserAutomation]::Snapshot()
        $name=[BrowserAutomation]::Names($snap)|Where-Object { $_.Contains('以及') }|Select-Object -First 1
        [BrowserAutomation]::RightClick($snap,[BrowserAutomation]::Find($snap,$name))
        Until { [FrequencyProbe]::RankMenu($false) } 'menu missing before focus switch'
        $nextEditor=$hostWindow.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'Other'))
        $rect=$nextEditor.Current.BoundingRectangle
        [FrequencyProbe]::ClickPoint([int]($rect.X+$rect.Width/2),[int]($rect.Y+$rect.Height/2))
        Until { ![FrequencyProbe]::RankMenu($false) } 'switching fields left old menu open'
        $nextEditor.SetFocus()
        Until { $nextEditor.Current.HasKeyboardFocus } 'second field did not receive focus'
        $nextValue=([Windows.Automation.ValuePattern]$nextEditor.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).Current.Value
        if ($nextValue -ne '' -or (Get-FileHash $userFile).Hash -ne $saved) { throw 'old menu changed the new field or dictionary' }
        Report @{case='right-click';passed=$true;committed='已经';afterFocusSwitch=(EditorText);other=$nextValue;dictionaryUnchanged=$true}
    } catch { Report @{case='right-click';passed=$false;reason=$_.Exception.Message} }
    try {
        UseTable parallel
        FocusHost $first
        ([Windows.Automation.ValuePattern]$editor.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).SetValue('')
        Start-Sleep -Milliseconds 200
        Pick '以及'
        Until { [int](Stats).'以及' -eq 1 } 'first host count'
        $other=Start-Process powershell.exe -ArgumentList '-NoProfile -ExecutionPolicy RemoteSigned -STA -File C:\Browser-Source\scripts\host.ps1' -WindowStyle Hidden -PassThru
        FocusHost $other.Id
        Pick '以及'
        Until { [int](Stats).'以及' -eq 2 } 'second host count'
        Key 8; Key 8
        Until { [int](Stats).'以及' -eq 1 -and (EditorText) -ceq '' } 'second host rollback'
        FocusHost $first
        if ((EditorText) -cne '以及') {throw 'first host was changed'}
        Report @{case='two-hosts';passed=$true;counts=(Stats);first=(EditorText);second=''}
    } catch { Report @{case='two-hosts';passed=$false;reason=$_.Exception.Message} }
    finally {if ($other -and !$other.HasExited) {Stop-Process -Id $other.Id}}
    if (@($report|Where-Object {!$_.passed}).Count) { throw 'Some host cases failed' }
    'passed'|Set-Content C:\Browser-Results\passed.txt
} catch { $_|Out-String|Set-Content C:\Browser-Results\failure.txt; exit 1 }
