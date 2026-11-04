param(
    [Parameter(Mandatory)][string]$Destination,
    [Parameter(Mandatory)][string]$OnnxRuntime,
    [string]$VcRedist
)
$ErrorActionPreference='Stop'
if (-not $VcRedist) {
    $vswhere=Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio\Installer\vswhere.exe'
    $vs=& $vswhere -latest -products '*' -property installationPath
    $root=Join-Path $vs 'VC\Redist\MSVC'
    $versions=Get-ChildItem -LiteralPath $root -Directory | Where-Object Name -Match '^\d+\.\d+\.\d+$' | Sort-Object {[version]$_.Name} -Descending
    foreach($version in $versions) {
        $crt=Get-ChildItem -LiteralPath (Join-Path $version.FullName 'x64') -Directory | Where-Object Name -Like 'Microsoft.VC*.CRT' | Select-Object -First 1
        if($crt){$VcRedist=$crt.FullName;break}
    }
}
if(-not $VcRedist){throw 'Provide the licensed Visual Studio x64 redistributable CRT directory'}
foreach($name in @('msvcp140.dll','msvcp140_1.dll','vcruntime140.dll','vcruntime140_1.dll')) {
    if(-not(Test-Path -LiteralPath (Join-Path $VcRedist $name))){throw "Missing redistributable $name"}
}
$destinationPath=[IO.Path]::GetFullPath($Destination)
New-Item -ItemType Directory -Path $destinationPath -Force | Out-Null
$files=@((Get-Item -LiteralPath $OnnxRuntime))+@(Get-ChildItem -LiteralPath $VcRedist -Filter '*.dll' -File)
$records=@()
foreach($file in $files) {
    $target=Join-Path $destinationPath $file.Name
    if($file.FullName -ne $target){Copy-Item -LiteralPath $file.FullName -Destination $target -ErrorAction Stop}
    $records+=@{name=$file.Name;version=$file.VersionInfo.FileVersion;sha256=(Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()}
}
@{vc_redist_directory=(Split-Path $VcRedist -Leaf);vc_license='Microsoft Visual Studio Distributable Code';vc_source='https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files';files=$records} |
    ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $destinationPath 'model-native-runtime.json') -Encoding utf8
