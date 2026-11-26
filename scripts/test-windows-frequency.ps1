param(
    [string]$Dll = "$PSScriptRoot\..\target\debug\im_engine.dll",
    [string]$Core = "$PSScriptRoot\..\target\debug\core_engine.dll",
    [string]$Output = "$PSScriptRoot\..\target\frequency-acceptance\windows-$(Get-Date -Format yyyyMMdd-HHmmss)",
    [switch]$PrepareOnly
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $root) { throw "Choose a fresh evidence directory: $root" }
$app = New-Item -ItemType Directory -Path "$root\app\tables"
$scripts = New-Item -ItemType Directory -Path "$root\scripts"
$results = New-Item -ItemType Directory -Path "$root\results"
Copy-Item -LiteralPath $Dll -Destination "$root\app\im_engine.dll"
Copy-Item -LiteralPath $Core -Destination "$root\app\core_engine.dll"
$utf8 = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText("$app\main.dict","ab|以及|10!ab|已经|9!".Replace('|',[char]9).Replace('!',[char]10),$utf8)
$config = @"
[basic]
auto_commit_unique = false
[dictionary]
system_table = "tables/main.dict"
user_table = "tables/user.dict"
enable_user_dict = true
[frequency]
record = true
rank = true
"@
[IO.File]::WriteAllText("$root\app\config.toml",$config,$utf8)
foreach ($name in 'start.ps1','host.ps1','interop.cs') {
    Copy-Item -LiteralPath "$PSScriptRoot\windows-expression-browser\$name" -Destination $scripts
}
Copy-Item -LiteralPath "$PSScriptRoot\windows-frequency\verify.ps1" -Destination $scripts
foreach ($path in Get-ChildItem $scripts -Filter '*.ps1') {
    [IO.File]::WriteAllText($path.FullName,[IO.File]::ReadAllText($path.FullName),[Text.UTF8Encoding]::new($true))
}
$sourceXml = [Security.SecurityElement]::Escape($root)
$resultsXml = [Security.SecurityElement]::Escape($results.FullName)
$wsb = @"
<Configuration><Networking>Disable</Networking><MappedFolders><MappedFolder><HostFolder>$sourceXml</HostFolder><SandboxFolder>C:\Browser-Source</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder><MappedFolder><HostFolder>$resultsXml</HostFolder><SandboxFolder>C:\Browser-Results</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder></MappedFolders><LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -STA -File C:\Browser-Source\scripts\start.ps1</Command></LogonCommand></Configuration>
"@
[IO.File]::WriteAllText("$root\frequency.wsb",$wsb,$utf8)
Get-FileHash $Dll,$Core | ConvertTo-Json | Set-Content "$root\build.json"
if (!$PrepareOnly) { Start-Process WindowsSandbox.exe -ArgumentList ('"'+$root+'\frequency.wsb"') -WindowStyle Hidden }
$root
