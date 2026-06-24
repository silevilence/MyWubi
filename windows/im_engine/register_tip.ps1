# MyWubi TIP 完整注册/卸载脚本 — 请在管理员 PowerShell 中运行
# 右键 PowerShell → "以管理员身份运行"，然后执行:
#   注册: .\windows\im_engine\register_tip.ps1
#   卸载: .\windows\im_engine\register_tip.ps1 -Unregister

param([switch]$Unregister)

$clsid = "{C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD8}"
$profileGuid = "{C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD9}"
$dllPath = "c:\code\dotnet\MyWubi\target\release\im_engine.dll"
$name = "MyWubi 形码输入法"
$langId = "0x00000804"   # 简体中文

if ($Unregister) {
    Write-Host "=== 卸载 MyWubi TIP ==="
    Remove-Item -Path "HKLM:\SOFTWARE\Classes\CLSID\$clsid" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -Path "HKLM:\SOFTWARE\Microsoft\CTF\TIP\$clsid" -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "已卸载。"
    exit 0
}

Write-Host "=== 注册 MyWubi TIP ==="

# 1. HKLM\SOFTWARE\Classes\CLSID\{CLSID}
New-Item -Path "HKLM:\SOFTWARE\Classes\CLSID\$clsid" -Force | Out-Null
Set-ItemProperty -Path "HKLM:\SOFTWARE\Classes\CLSID\$clsid" -Name "(Default)" -Value $name

# 2. InprocServer32
New-Item -Path "HKLM:\SOFTWARE\Classes\CLSID\$clsid\InprocServer32" -Force | Out-Null
Set-ItemProperty -Path "HKLM:\SOFTWARE\Classes\CLSID\$clsid\InprocServer32" -Name "(Default)" -Value $dllPath
Set-ItemProperty -Path "HKLM:\SOFTWARE\Classes\CLSID\$clsid\InprocServer32" -Name "ThreadingModel" -Value "Apartment"

# 3. ProgID
New-Item -Path "HKLM:\SOFTWARE\Classes\CLSID\$clsid\ProgID" -Force | Out-Null
Set-ItemProperty -Path "HKLM:\SOFTWARE\Classes\CLSID\$clsid\ProgID" -Name "(Default)" -Value "MyWubi.TextService.1"

# 4. HKLM\SOFTWARE\Microsoft\CTF\TIP\{CLSID}
New-Item -Path "HKLM:\SOFTWARE\Microsoft\CTF\TIP\$clsid" -Force | Out-Null
Set-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\CTF\TIP\$clsid" -Name "(Default)" -Value $name

# 5. CLSID subkey
New-Item -Path "HKLM:\SOFTWARE\Microsoft\CTF\TIP\$clsid\CLSID" -Force | Out-Null
Set-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\CTF\TIP\$clsid\CLSID" -Name "(Default)" -Value $clsid

# 6. LanguageProfile —— 关键！必须关联到语言才能出现在键盘列表
$lpPath = "HKLM:\SOFTWARE\Microsoft\CTF\TIP\$clsid\LanguageProfile\$langId\$profileGuid"
New-Item -Path $lpPath -Force | Out-Null
Set-ItemProperty -Path $lpPath -Name "Description" -Value $name
Set-ItemProperty -Path $lpPath -Name "IconFile" -Value $dllPath
Set-ItemProperty -Path $lpPath -Name "IconIndex" -Value 0 -Type DWord
Set-ItemProperty -Path $lpPath -Name "Enable" -Value 1 -Type DWord

Write-Host ""
Write-Host "验证:"
$ok1 = Test-Path "HKLM:\SOFTWARE\Microsoft\CTF\TIP\$clsid"
$ok2 = Test-Path "HKLM:\SOFTWARE\Classes\CLSID\$clsid\InprocServer32"
$ok3 = Test-Path $lpPath
Write-Host "  CTF TIP:   $(if($ok1){'✅'}else{'❌'})"
Write-Host "  InprocSvr: $(if($ok2){'✅'}else{'❌'})"
Write-Host "  LangProf:  $(if($ok3){'✅'}else{'❌'})"
Write-Host ""
Write-Host "下一步: 打开记事本 → Win+Space → 找到 MyWubi 形码输入法"
