# reg_script.ps1
#
# Windows TSF TIP 注册脚本（脱离 regsvr32 流程的备用通道）。
#
# 作用：直接通过 PowerShell + Advapi32 注册表 API 写入 CLSID 与 TIP 节点，
# 等同于 regsvr32 im_engine.dll 的 DllRegisterServer 调用，便于 Velopack
# 安装 Hook 在没有任何控制台窗口时静默触发注册。
#
# 使用：
#   .\reg_script.ps1 -Action register   -DllPath "C:\path\to\im_engine.dll"
#   .\reg_script.ps1 -Action unregister
#
# 与 im_engine/src/registrar.rs 中的 register_tip() 写入相同的注册表结构，
# 仅在调用方一侧以 PowerShell 复述。
param(
    [Parameter(Mandatory = $false)]
    [ValidateSet("register", "unregister")]
    [string]$Action = "register",

    [Parameter(Mandatory = $false)]
    [string]$DllPath
)

# ── 常量：与 src/guids.rs 中保持一致 ────────────────────────────────
$ClsidTextService = "{C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD8}"
$ProfileGuid       = "{C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD9}"
$TextServiceName   = "MyWubi 形码输入法"
$ProgId            = "MyWubi.TextService.1"

function Write-RegistrySz {
    param(
        [string]$Path,
        [string]$Name,         # "" 表示默认值
        [string]$Value
    )
    $key = $null
    try {
        $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey(
            $Path,
            [Microsoft.Win32.RegistryKeyPermissionCheck]::ReadWriteSubTree
        ) -or
        ([Microsoft.Win32.Registry]::LocalMachine.CreateSubKey(
            $Path,
            [Microsoft.Win32.RegistryKeyPermissionCheck]::ReadWriteSubTree
        ))
    } catch {
        throw "Failed to open registry path '$Path': $($_.Exception.Message)"
    }
    if (-not $key) {
        throw "Failed to open registry path '$Path'"
    }
    try {
        if ([string]::IsNullOrEmpty($Name)) {
            $key.SetValue($null, $Value, [Microsoft.Win32.RegistryValueKind]::String)
        } else {
            $key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::String)
        }
    } finally {
        $key.Close()
    }
}

function Invoke-Register {
    if ([string]::IsNullOrEmpty($DllPath)) {
        # 未提供 DllPath：尝试从 DLL 同目录推断。
        if (-not (Test-Path "im_engine.dll")) {
            throw "未提供 -DllPath，且当前目录下没有 im_engine.dll"
        }
        $DllPath = (Resolve-Path "im_engine.dll").Path
    } elseif (-not (Test-Path $DllPath)) {
        throw "指定的 -DllPath 不存在: $DllPath"
    } else {
        $DllPath = (Resolve-Path $DllPath).Path
    }

    $hkeyRoot = "HKLM:\SOFTWARE\Classes\CLSID\$ClsidTextService"
    $ctfRoot  = "HKLM:\SOFTWARE\Microsoft\CTF\TIP\$ClsidTextService"

    # 1. HKCR\CLSID\{CLSID} = 显示名
    if (-not (Test-Path $hkeyRoot)) { New-Item -Path $hkeyRoot -Force | Out-Null }
    Set-ItemProperty -Path $hkeyRoot -Name "(Default)" -Value $TextServiceName

    # 2. ... \InprocServer32 = DLL 路径 + ThreadingModel = Apartment
    $inproc = "$hkeyRoot\InprocServer32"
    if (-not (Test-Path $inproc)) { New-Item -Path $inproc -Force | Out-Null }
    Set-ItemProperty -Path $inproc -Name "(Default)" -Value $DllPath
    Set-ItemProperty -Path $inproc -Name "ThreadingModel" -Value "Apartment"

    # 3. ... \ProgID = MyWubi.TextService.1
    $progid = "$hkeyRoot\ProgID"
    if (-not (Test-Path $progid)) { New-Item -Path $progid -Force | Out-Null }
    Set-ItemProperty -Path $progid -Name "(Default)" -Value $ProgId

    # 4. HKLM\SOFTWARE\Microsoft\CTF\TIP\{CLSID}
    if (-not (Test-Path $ctfRoot)) { New-Item -Path $ctfRoot -Force | Out-Null }
    Set-ItemProperty -Path $ctfRoot -Name "(Default)" -Value $TextServiceName

    # 5. ... \CLSID = {CLSID}
    $tipClsid = "$ctfRoot\CLSID"
    if (-not (Test-Path $tipClsid)) { New-Item -Path $tipClsid -Force | Out-Null }
    Set-ItemProperty -Path $tipClsid -Name "(Default)" -Value $ClsidTextService

    # 6. ... \LanguageProfile\{Profile GUID}
    $lp = "$ctfRoot\LanguageProfile"
    if (-not (Test-Path $lp)) { New-Item -Path $lp -Force | Out-Null }
    Set-ItemProperty -Path $lp -Name "(Default)" -Value $ProfileGuid

    Write-Host "[reg_script] Registered TIP CLSID=$ClsidTextService dll=$DllPath"
}

function Invoke-Unregister {
    $hkeyRoot = "HKLM:\SOFTWARE\Classes\CLSID\$ClsidTextService"
    $ctfRoot  = "HKLM:\SOFTWARE\Microsoft\CTF\TIP\$ClsidTextService"

    if (Test-Path $hkeyRoot) {
        Remove-Item -Path $hkeyRoot -Recurse -Force
        Write-Host "[reg_script] Removed $hkeyRoot"
    } else {
        Write-Host "[reg_script] (skip) not present: $hkeyRoot"
    }

    if (Test-Path $ctfRoot) {
        Remove-Item -Path $ctfRoot -Recurse -Force
        Write-Host "[reg_script] Removed $ctfRoot"
    } else {
        Write-Host "[reg_script] (skip) not present: $ctfRoot"
    }
}

switch ($Action) {
    "register"   { Invoke-Register }
    "unregister" { Invoke-Unregister }
}