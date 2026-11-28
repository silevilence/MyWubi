# Run inside Windows Sandbox with 32-bit Windows PowerShell -STA.
# The sandbox's input list must contain US English and MyWubi with Wubi 86.
$ErrorActionPreference = 'Stop'
if ([IntPtr]::Size -ne 4 -or [Threading.Thread]::CurrentThread.ApartmentState -ne 'STA') {
    throw 'Run with SysWOW64\WindowsPowerShell\v1.0\powershell.exe -STA.'
}
Add-Type -AssemblyName System.Windows.Forms
Add-Type -Path (Join-Path $PSScriptRoot 'WinFormsInputProbe.cs') -ReferencedAssemblies System.Windows.Forms,System.Drawing

$form = New-Object Windows.Forms.Form
$form.Text = 'MyWubi 32-bit WinForms input test'
$form.TopMost = $true
$editor = New-Object Windows.Forms.TextBox
$editor.Dock = 'Fill'
$form.Controls.Add($editor)
$focusButton = New-Object Windows.Forms.Button
$focusButton.Text = 'Focus editor'
$focusButton.Dock = 'Bottom'
$focusButton.Add_Click({ [void]$editor.Focus() })
$form.Controls.Add($focusButton)
$form.Add_Shown({
    $form.Activate()
    [void]$editor.Focus()
    $script:verification = [WinFormsInputProbe]::Run($editor, $focusButton)
})
try {
    [void]$form.ShowDialog()
    if (!$verification -or !$verification.IsCompleted) {
        throw 'Test window closed before input verification finished.'
    }
    $verification.GetAwaiter().GetResult()
} finally {
    $form.Dispose()
}
