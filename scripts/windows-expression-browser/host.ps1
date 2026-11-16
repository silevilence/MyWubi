$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework
[xml]$xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Title="MyWubi browser automation host" Width="900" Height="360" Left="40" Top="40">
<StackPanel Margin="16"><TextBlock Text="特殊输入自动化验收" FontSize="20"/><TextBox Name="Editor" FontSize="24" Height="80" Margin="0,16" AcceptsReturn="True"/><Button Name="FocusEditor" Content="输入区" Width="140" Height="32"/><TextBox Name="Other" Height="32" Margin="0,8"/></StackPanel>
</Window>
'@
$window = [Windows.Markup.XamlReader]::Load((New-Object Xml.XmlNodeReader $xaml))
$editor = $window.FindName('Editor')
function Focus-Editor {
    [Windows.Input.InputLanguageManager]::Current.CurrentInputLanguage = [Globalization.CultureInfo]::GetCultureInfo('zh-CN')
    $editor.Focus() | Out-Null
    [Windows.Input.InputMethod]::Current.ImeState = 'On'
}
$window.FindName('FocusEditor').Add_Click({Focus-Editor})
$window.Add_Loaded({Focus-Editor; $PID | Set-Content C:\Browser-Results\host-pid.txt})
$editor.Add_TextChanged({
    @{text=$editor.Text;focus=$editor.IsKeyboardFocused;modules=@((Get-Process -Id $PID).Modules | Where-Object ModuleName -like 'im_engine*.dll' | Select-Object FileName)} | ConvertTo-Json -Depth 5 | Set-Content C:\Browser-Results\host.json -Encoding UTF8
})
$window.ShowDialog() | Out-Null
