# MyWubi 发布打包脚本
# 将 release 产物 + 配置文件 + 码表复制到 deploy/ 目录
#
# 用法: .\package.ps1          # debug 构建（默认）
#       .\package.ps1 -Release  # release 构建

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"

$profile = if ($Release) { "release" } else { "debug" }
$targetDir = "target\$profile"
$deployDir = "deploy"

Write-Host "=== MyWubi 打包 ($profile) ===" -ForegroundColor Cyan

# 1. 确保已构建
if (-not (Test-Path "$targetDir\settings.exe")) {
    Write-Host "尚未构建，执行 cargo build --$profile ..." -ForegroundColor Yellow
    cargo build --$profile
    if ($LASTEXITCODE -ne 0) { throw "构建失败" }
}

# 2. 创建 deploy 目录
New-Item -ItemType Directory -Force -Path $deployDir | Out-Null

# 3. 复制二进制文件
Write-Host "复制二进制文件..." -ForegroundColor Green
Copy-Item "$targetDir\settings.exe" "$deployDir\" -Force
Copy-Item "$targetDir\im_engine.dll" "$deployDir\" -Force

# 4. 复制配置文件
Write-Host "复制配置文件..." -ForegroundColor Green
Copy-Item "config.toml" "$deployDir\" -Force

# 5. 复制码表（排除用户词库）
Write-Host "复制码表..." -ForegroundColor Green
$tableDest = "$deployDir\tables"
New-Item -ItemType Directory -Force -Path $tableDest | Out-Null
Copy-Item "tables\*.dict" $tableDest -Force -ErrorAction SilentlyContinue
# 不复制 user.dict（用户词库）
Remove-Item "$tableDest\user.dict" -Force -ErrorAction SilentlyContinue

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
