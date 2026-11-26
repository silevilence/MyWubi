param(
    [string]$Dll = (Join-Path $PSScriptRoot '..\target\release\im_engine.dll'),
    [string]$Output = (Join-Path $PSScriptRoot ('..\target\expression-browser\sandbox-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))),
    [switch]$PrepareOnly
)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$Output = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $Output) {throw "Use a new output directory to preserve prior evidence: $Output"}
if (-not (Test-Path -LiteralPath $Dll)) {throw 'Build im_engine locally before running Sandbox validation.'}
$app = Join-Path $Output 'app'
$scripts = Join-Path $Output 'scripts'
$results = Join-Path $Output 'results'
New-Item -ItemType Directory -Path $app,$scripts,$results,(Join-Path $app 'tables') | Out-Null
Copy-Item -LiteralPath $Dll -Destination (Join-Path $app 'im_engine.dll')
Copy-Item -Path (Join-Path $PSScriptRoot 'windows-expression-browser\*') -Destination $scripts
& (Join-Path $PSScriptRoot 'copy-expression-bundle.ps1') -Destination (Join-Path $app 'expressions')
# Reuse the complete local library, moving its longest entry first to exercise wrapping
# without hundreds of UI page turns. This is a selected imported-resource fixture.
$document = (([IO.File]::ReadAllText((Join-Path $app 'expressions\kaomoji.mwe')) -split "`n",2)[1] | ConvertFrom-Json)
$longest = $document.entries | Sort-Object {$_.text.Length} -Descending | Select-Object -First 1
$document.entries = @($longest) + @($document.entries | Where-Object text -CNE $longest.text)
for ($index=0; $index -lt $document.entries.Count; $index++) {$document.entries[$index].order=$index}
$utf8 = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText((Join-Path $app 'expressions\imported-kaomoji.mwe'),"# MyWubi Expressions 1`n"+($document | ConvertTo-Json -Depth 20 -Compress),$utf8)
$table = @'
---
display_name: Browser automation fixture
charset: abcdefghijklmnopqrstuvwxyz
max_code_len: 4
temporary:
  key: '`'
---
a	笑脸
b	普通
jjad	2026-11-12	100
jjad	甲	90
jjad	乙	80
'@
[IO.File]::WriteAllText((Join-Path $app 'tables\main.dict'),$table.Replace('\t',"`t"),$utf8)
$config = @'
[basic]
candidate_count = 2
auto_commit_unique = false
[dictionary]
system_table = "tables/main.dict"
user_table = "tables/user.dict"
[appearance]
font_size = 14
background_color = 0xFFFFFFFF
highlight_color = 0xFFFFD54F
[[shortcuts.rules]]
code = "jjad"
position = 3
kind = "date_time"
format = "'2026-11-12'"
enabled = true
'@
foreach ($kind in 'emoji','kaomoji','kana','math','punctuation','symbols','greek','cyrillic','latin','phonetic','enclosed','numbers','arrows','currency','units','shapes','box_drawing','braille','music','diacritics') {
    $config += "`n[expressions.$kind]`nsearch = true`n"
    if ($kind -eq 'kaomoji') {$config += 'resource = "expressions/imported-kaomoji.mwe"' + "`n"}
}
[IO.File]::WriteAllText((Join-Path $app 'config.toml'),$config.Replace("`r`n","`n"),$utf8)
Get-FileHash (Join-Path $app 'im_engine.dll') | ConvertTo-Json | Set-Content (Join-Path $Output 'dll.json')
$sourceXml = [Security.SecurityElement]::Escape($Output)
$resultsXml = [Security.SecurityElement]::Escape($results)
$wsb = @"
<Configuration><Networking>Disable</Networking><MappedFolders><MappedFolder><HostFolder>$sourceXml</HostFolder><SandboxFolder>C:\Browser-Source</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder><MappedFolder><HostFolder>$resultsXml</HostFolder><SandboxFolder>C:\Browser-Results</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder></MappedFolders><LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File C:\Browser-Source\scripts\start.ps1</Command></LogonCommand></Configuration>
"@
# Scripts contain Chinese control names; Windows PowerShell 5 needs a UTF-8 BOM.
foreach ($path in Get-ChildItem $scripts -Filter '*.ps1') {
    [IO.File]::WriteAllText($path.FullName,[IO.File]::ReadAllText($path.FullName),[Text.UTF8Encoding]::new($true))
}
[IO.File]::WriteAllText((Join-Path $Output 'browser.wsb'),$wsb,$utf8)
Write-Output $Output
if (-not $PrepareOnly) {
    Start-Process WindowsSandbox.exe -ArgumentList ('"'+(Join-Path $Output 'browser.wsb')+'"') -WindowStyle Hidden
}
