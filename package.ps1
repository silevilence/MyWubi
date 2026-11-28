# MyWubi 发布打包脚本
# 将 release 产物 + 码表复制到 deploy/ 目录
#
# 用法: .\package.ps1              # debug 构建（默认）
#       .\package.ps1 -Release      # release 构建
#       .\package.ps1 -CopyConfig   # 同时复制默认配置文件 config.toml

param(
    [switch]$Release,
    [switch]$CopyConfig
)

$ErrorActionPreference = "Stop"

# 选择构建配置
$buildProfile = if ($Release) { "release" } else { "debug" }
$targetDir = "target\$buildProfile"
$imEngineX86 = "target\i686-pc-windows-msvc\$buildProfile\im_engine.dll"
$deployDir = "deploy"

Write-Host "=== MyWubi 打包 ($buildProfile) ===" -ForegroundColor Cyan

# 1. 确保已构建
$profileArgs = if ($Release) { @('--release') } else { @() }
cargo build --locked -p settings -p im_engine @profileArgs
if ($LASTEXITCODE -ne 0) { throw 'Application build failed' }
cargo build --locked --target i686-pc-windows-msvc -p im_engine @profileArgs
if ($LASTEXITCODE -ne 0) { throw '32-bit input service build failed' }

# 2. 创建 deploy 目录
New-Item -ItemType Directory -Force -Path $deployDir | Out-Null

# 3. 复制二进制文件
Write-Host "复制二进制文件..." -ForegroundColor Green
Copy-Item "$targetDir\settings.exe" "$deployDir\" -Force
Copy-Item "$targetDir\skin_editor.exe" "$deployDir\" -Force
Copy-Item "$targetDir\im_engine.dll" "$deployDir\" -Force
Copy-Item -LiteralPath $imEngineX86 -Destination (Join-Path $deployDir 'im_engine_x86.dll') -Force

# 4. 复制配置文件（默认不复制，避免覆盖已有配置）
if ($CopyConfig) {
    Write-Host "复制配置文件..." -ForegroundColor Green
    Copy-Item "config.toml" "$deployDir\" -Force
}

# 5. 复制码表（排除用户词库）
Write-Host "复制码表..." -ForegroundColor Green
$tableDest = "$deployDir\tables"
New-Item -ItemType Directory -Force -Path $tableDest | Out-Null
foreach ($table in @('wubi86.dict', 'pinyin.dict', 'english.dict', 'README.md', 'sources.json')) {
    Copy-Item -LiteralPath (Join-Path 'tables' $table) -Destination $tableDest -ErrorAction Stop
}
Copy-Item -LiteralPath 'tables\licenses' -Destination $tableDest -Recurse -ErrorAction Stop
& (Join-Path $PSScriptRoot 'scripts\copy-expression-bundle.ps1') -Destination (Join-Path $deployDir 'expressions')

# 6. 显示结果
Write-Host ""
Write-Host "=== 打包完成 ===" -ForegroundColor Cyan
Get-ChildItem $deployDir -Recurse -File | ForEach-Object {
    $size = [math]::Round($_.Length / 1KB, 1)
    $name = $_.FullName.Replace((Resolve-Path $deployDir).Path + "\", "")
    Write-Host "  $name  ($size KB)"
}
Write-Host ""
Write-Host "输出目录: $(Resolve-Path $deployDir)" -ForegroundColor Green
