$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,Accessibility,System.Drawing
Add-Type -Path C:\Browser-Source\scripts\interop.cs -ReferencedAssemblies Accessibility,System.Drawing
$results = [Collections.Generic.List[object]]::new()
function Wait-Until([scriptblock]$Condition,[string]$Message) {
    $deadline = (Get-Date).AddSeconds(20)
    do {if (& $Condition) {return}; Start-Sleep -Milliseconds 100} while ((Get-Date) -lt $deadline)
    throw $Message
}
function Check([bool]$Condition,[string]$Name) {
    if (-not $Condition) {throw $Name}
    $results.Add(@{name=$Name;passed=$true})
    $results | ConvertTo-Json -Depth 5 | Set-Content C:\Browser-Results\checks.json -Encoding UTF8
}
function Snapshot { [BrowserAutomation]::Snapshot() }
function Invoke-Control([string]$Name) {
    $a = Snapshot
    [BrowserAutomation]::Invoke($a,[BrowserAutomation]::Find($a,$Name))
    Start-Sleep -Milliseconds 180
}
function Names { [BrowserAutomation]::Names((Snapshot)) }
function Text { ([Windows.Automation.ValuePattern]$script:editor.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).Current.Value }
function Expect-Text([string]$Expected,[string]$Name) {
    Wait-Until { (Text) -ceq $Expected } "$Name; expected='$Expected', actual='$(Text)'"
    Check ($script:editor.Current.HasKeyboardFocus) "$Name / 编辑框保持焦点"
}
function Open-Browser {
    [BrowserAutomation]::Key(0xC0)
    Wait-Until { [BrowserAutomation]::Palette() -ne [IntPtr]::Zero } '辅助输入菜单未打开'
    Invoke-Control 'Emoji，'
    Invoke-Control '全部'
    Check ((Names) -contains '搜索') '空关键词打开全部浏览'
}
function Resource([string]$Slug) {
    $name = if ($Slug -eq 'kaomoji') {'imported-kaomoji'} else {$Slug}
    (([IO.File]::ReadAllText("C:\Browser-App\expressions\$name.mwe") -split "`n",2)[1] | ConvertFrom-Json).entries
}
try {
    $hostCondition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,'MyWubi browser automation host')
    Wait-Until { $script:hostWindow=[Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children,$hostCondition); $null -ne $script:hostWindow } '测试宿主未启动'
    [BrowserAutomation]::SetForegroundWindow([IntPtr]$hostWindow.Current.NativeWindowHandle) | Out-Null
    $script:editor=$hostWindow.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'Editor'))
    $button=$hostWindow.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'FocusEditor'))
    ([Windows.Automation.InvokePattern]$button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke()
    Start-Sleep -Milliseconds 500
    [BrowserAutomation]::Key(0x1B)
    [BrowserAutomation]::CtrlA()
    [BrowserAutomation]::Key(0x08)
    Open-Browser
    $emoji=@(Resource emoji)
    $expected=''
    $stale=Snapshot
    $id=[BrowserAutomation]::Find($stale,$emoji[0].text+'，')
    [BrowserAutomation]::Click($stale,$id)
    $expected += $emoji[0].text
    Expect-Text $expected '真实鼠标点击上屏'
    Check ([BrowserAutomation]::Palette() -ne [IntPtr]::Zero) '上屏后面板保留'
    [BrowserAutomation]::Invoke($stale,$id)
    Start-Sleep -Milliseconds 250
    Check ((Text) -ceq $expected) '过期无障碍操作不重复提交'
    Invoke-Control ($emoji[0].text+'，')
    $expected += $emoji[0].text
    Expect-Text $expected '连续上屏'
    [BrowserAutomation]::Key(0x27); [BrowserAutomation]::Key(0x0D)
    $expected += $emoji[1].text
    Expect-Text $expected '方向键和回车选择'
    [BrowserAutomation]::Key(0x22)
    Start-Sleep -Milliseconds 200
    [BrowserAutomation]::Key(0x24); [BrowserAutomation]::Key(0x0D)
    $expected += $emoji[24].text
    Expect-Text $expected 'PageDown 后选择下一页首项'
    [BrowserAutomation]::Capture('C:\Browser-Results\emoji-light.png')
    foreach ($category in @(@('kaomoji','颜文字'),@('kana','假名'),@('math','数学'),@('punctuation','标点'),@('symbols','其他符号'))) {
        Invoke-Control $category[1]
        $entries=@(Resource $category[0])
        [BrowserAutomation]::Key(0x0D)
        $expected += $entries[0].text
        Expect-Text $expected ($category[1]+'分类原文上屏')
        [BrowserAutomation]::Capture("C:\Browser-Results\$($category[0])-light.png")
        if ($category[0] -eq 'kaomoji') {
            $before=(Names) -join '|'
            [BrowserAutomation]::Wheel(-480)
            Start-Sleep -Milliseconds 250
            Check (((Names) -join '|') -ne $before) '长颜文字网格可滚动'
            Check ((Names) -contains '搜索') '滚动后搜索按钮固定可用'
        }
    }
    Invoke-Control '假名'
    $kana=@(Resource kana)
    $pages=1
    while ((Names) -contains '下一页') {
        Invoke-Control '下一页'; $pages++
        if ($pages -gt 30) {throw '假名分页未到达末尾'}
    }
    [BrowserAutomation]::Key(0x23); [BrowserAutomation]::Key(0x0D)
    $expected += $kana[-1].text
    Expect-Text $expected '末页最后一项可达'
    Check (-not ((Names) -contains '下一页')) '连续输入保留末页位置'
    Invoke-Control '搜索'
    Invoke-Control '切换为字面别名'
    foreach ($key in 0x53,0x48,0x49) {[BrowserAutomation]::Key($key)}
    Start-Sleep -Milliseconds 150
    Invoke-Control '全部'
    Invoke-Control '搜索'
    [BrowserAutomation]::Key(0x09)
    Start-Sleep -Milliseconds 250
    Invoke-Control 'し，'
    $expected += 'し'
    Expect-Text $expected '全部与搜索切换保留 shi 草稿'
    Open-Browser
    [BrowserAutomation]::Key(0x1B)
    Wait-Until { [BrowserAutomation]::Palette() -eq [IntPtr]::Zero } 'Esc 未关闭面板'
    Check ((Text) -ceq $expected) 'Esc 不写入正文'
    Open-Browser
    $other=$hostWindow.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'Other'))
    $other.SetFocus()
    Wait-Until { [BrowserAutomation]::Palette() -eq [IntPtr]::Zero } '失焦后面板未关闭'
    Check $true '切换编辑框关闭浏览'
    $editor.SetFocus()
    Wait-Until { $editor.Current.HasKeyboardFocus } '选区替换前编辑框未获得焦点'
    Start-Sleep -Milliseconds 250
    [BrowserAutomation]::CtrlA()
    Start-Sleep -Milliseconds 150
    Open-Browser
    Invoke-Control ($emoji[0].text+'，')
    Expect-Text $emoji[0].text '替换宿主选区'
    [BrowserAutomation]::Key(0x1B)
    $config=[IO.File]::ReadAllText('C:\Browser-App\config.toml').Replace('0xFFFFFFFF','0xFF202420').Replace('0xFFFFD54F','0xFF395B49').Replace("[expressions.math]`nsearch = true","[expressions.math]`nsearch = false")
    [IO.File]::WriteAllText('C:\Browser-App\config.toml',$config,[Text.UTF8Encoding]::new($false))
    Start-Sleep -Seconds 2
    Open-Browser
    Check (-not ((Names) -contains '数学')) '关闭专用检索的分类不可操作'
    [BrowserAutomation]::Capture('C:\Browser-Results\emoji-dark.png')
    Invoke-Control '关闭'
    Wait-Until { [BrowserAutomation]::Palette() -eq [IntPtr]::Zero } '关闭按钮未生效'
    Check $true '关闭按钮结束浏览'
    @{passed=$true;checks=$results.Count;time=(Get-Date).ToString('o');os=[Environment]::OSVersion.VersionString;automation='WPF UIA + native MSAA + synthesized keyboard/mouse; no ComputerUse';resources='local bundle; imported kaomoji order fixture'} | ConvertTo-Json | Set-Content C:\Browser-Results\result.json -Encoding UTF8
    'passed' | Set-Content C:\Browser-Results\passed.txt
} catch {
    $_ | Out-String | Set-Content C:\Browser-Results\failure.txt -Encoding UTF8
    try { (Names) | ConvertTo-Json | Set-Content C:\Browser-Results\failure-controls.json -Encoding UTF8; [BrowserAutomation]::Capture('C:\Browser-Results\failure.png') } catch {}
    exit 1
}
