# MyWubi Velopack 打包脚本
#
# 一键完成：Release 编译 → 产物暂存 → vpk pack 生成安装包(Setup.exe)/便携包(Portable.zip)/增量包(delta)
#
# 用法:
#   .\build.ps1                       # 版本号取自 Cargo.toml [workspace.package].version
#   .\build.ps1 -Version 0.2.0        # 显式指定版本号
#   .\build.ps1 -SkipBuild            # 跳过 cargo build（已有产物时）
#
# 产物输出至 .\Releases\

param(
    [string]$Version,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$repoRoot = $PSScriptRoot
$targetDir = Join-Path $repoRoot "target\release"
$stagingDir = Join-Path $repoRoot "target\velopack-staging"
$releasesDir = Join-Path $repoRoot "Releases"

# ── 1. 解析版本号 ───────────────────────────────────────────
if (-not $Version) {
    $cargoToml = Get-Content (Join-Path $repoRoot "Cargo.toml") -Raw
    if ($cargoToml -match '(?ms)^\[workspace\.package\].*?^version\s*=\s*"(?<v>[^"]+)"') {
        $Version = $matches['v']
    } else {
        throw "无法从 Cargo.toml 解析版本号，请通过 -Version 显式指定。"
    }
}
Write-Host "=== MyWubi Velopack 打包 (v$Version) ===" -ForegroundColor Cyan

# ── 2. 校验 vpk 工具链 ─────────────────────────────────────
$vpk = Get-Command vpk -ErrorAction SilentlyContinue
if (-not $vpk) {
    Write-Host "未检测到 vpk，正在安装 Velopack CLI…" -ForegroundColor Yellow
    dotnet tool update -g vpk
    if ($LASTEXITCODE -ne 0) { throw "vpk 安装失败，请先安装 .NET SDK 8。" }
}

# ── 3. Release 编译 ────────────────────────────────────────
if (-not $SkipBuild) {
    Write-Host "编译 Release 产物…" -ForegroundColor Green
    Push-Location $repoRoot
    cargo build --release
    $buildExit = $LASTEXITCODE
    Pop-Location
    if ($buildExit -ne 0) { throw "cargo build --release 失败 ($buildExit)" }
}

# 校验关键产物存在
$settingsExe = Join-Path $targetDir "settings.exe"
$imEngineDll = Join-Path $targetDir "im_engine.dll"
if (-not (Test-Path $settingsExe)) { throw "找不到 $settingsExe，请先执行 cargo build --release。" }
if (-not (Test-Path $imEngineDll)) { throw "找不到 $imEngineDll，请先执行 cargo build --release。" }

# ── 4. 暂存产物到 packDir ──────────────────────────────────
Write-Host "暂存打包产物到 $stagingDir …" -ForegroundColor Green
if (Test-Path $stagingDir) { Remove-Item $stagingDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null

# 主二进制
Copy-Item $settingsExe $stagingDir -Force
Copy-Item $imEngineDll $stagingDir -Force

# 码表（项目根 tables/ 目录下的自带码表）
$tablesSrc = Join-Path $repoRoot "tables"
$tablesDst = Join-Path $stagingDir "tables"
New-Item -ItemType Directory -Force -Path $tablesDst | Out-Null
Copy-Item (Join-Path $tablesSrc "*.dict") $tablesDst -Force -ErrorAction SilentlyContinue

# 注：不打包默认 config.toml。settings.exe 首次启动时会通过
# config_path::resolve_config_path() 自动写入内置默认配置（便携模式优先，
# 回退 %APPDATA%\MyWubi\），避免覆盖用户已有配置。

# ── 5. 解析 CHANGELOG 作为 release notes ───────────────────
$releaseNotesFile = Join-Path $env:TEMP "mywubi-release-notes-$Version.md"
$changelogPath = Join-Path $repoRoot "CHANGELOG.md"
$notesWritten = $false
if (Test-Path $changelogPath) {
    $lines = Get-Content $changelogPath
    $startIndex = -1
    for ($i = 0; $i -lt $lines.Length; $i++) {
        if ($lines[$i].TrimStart() -match "^##\s+[Vv]$Version\s*$") { $startIndex = $i; break }
    }
    if ($startIndex -ge 0) {
        $notes = [System.Collections.ArrayList]@()
        for ($j = $startIndex + 1; $j -lt $lines.Length; $j++) {
            if ($lines[$j].TrimStart() -match "^##\s+") { break }
            $null = $notes.Add($lines[$j])
        }
        if ($notes.Count -gt 0) {
            ($notes -join "`n").Trim() | Out-File -FilePath $releaseNotesFile -Encoding utf8
            $notesWritten = $true
            Write-Host "已从 CHANGELOG 提取 V$Version 更新说明。" -ForegroundColor Green
        }
    }
}

# ── 6. vpk pack ────────────────────────────────────────────
Write-Host "执行 vpk pack …" -ForegroundColor Green
if (-not (Test-Path $releasesDir)) { New-Item -ItemType Directory -Force -Path $releasesDir | Out-Null }

# 安装完成提示文案：引导用户手动运行 settings.exe 安装输入法。
# 不由安装器自动调用 settings.exe（其嵌入 asInvoker 清单，安装器在非提升
# 上下文启动它虽不再报错，但 TIP 注册需要管理员权限会失败）。
$conclusionFile = Join-Path $env:TEMP "mywubi-inst-conclusion.txt"
@"
安装已完成！

请手动运行「MyWubi 设置」程序（开始菜单或桌面快捷方式），在「输入法管理」面板点击「安装输入法」完成输入法注册。

注意：「MyWubi 设置」需要以管理员身份运行才能安装输入法。程序会在你点击安装时自动请求管理员权限。
"@ | Out-File -FilePath $conclusionFile -Encoding utf8

$vpkArgs = @(
    "pack",
    "--packId", "MyWubi",
    "--packVersion", $Version,
    "--packDir", $stagingDir,
    "--mainExe", "settings.exe",
    "--packTitle", "MyWubi",
    "--packAuthors", "silevilence",
    "--outputDir", $releasesDir,
    "--instConclusion", $conclusionFile
)
if ($notesWritten) { $vpkArgs += @("--releaseNotes", $releaseNotesFile) }

& vpk @vpkArgs
if ($LASTEXITCODE -ne 0) { throw "vpk pack 失败 ($LASTEXITCODE)" }

# ── 7. 输出结果 ────────────────────────────────────────────
Write-Host ""
Write-Host "=== 打包完成 ===" -ForegroundColor Cyan
Get-ChildItem $releasesDir -File | Where-Object { $_.LastWriteTime -gt (Get-Date).AddMinutes(-5) } | ForEach-Object {
    $size = [math]::Round($_.Length / 1MB, 2)
    Write-Host ("  {0}  ({1} MB)" -f $_.Name, $size)
}
Write-Host ""
Write-Host "输出目录: $releasesDir" -ForegroundColor Green
Write-Host "Setup.exe 用于标准安装；*-Portable.zip 为绿色便携版；*.nupkg 为增量更新包。" -ForegroundColor Green