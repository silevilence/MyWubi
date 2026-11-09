param([Parameter(Mandatory = $true)][string]$Destination)
$ErrorActionPreference = 'Stop'
$expressionSource = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\expressions'))
$expressionDestination = [IO.Path]::GetFullPath($Destination)
foreach ($line in Get-Content -LiteralPath (Join-Path $expressionSource 'bundle-files.tsv') -Encoding utf8) {
    if (-not $line -or $line.StartsWith('#')) { continue }
    $parts = $line.Split("`t")
    if ($parts.Count -ne 2) { throw 'Invalid expression bundle manifest line' }
    $source = [IO.Path]::GetFullPath((Join-Path $expressionSource $parts[0]))
    $target = [IO.Path]::GetFullPath((Join-Path $expressionDestination $parts[1]))
    if (-not $source.StartsWith($expressionSource + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        -not $target.StartsWith($expressionDestination + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Expression bundle path escapes its directory'
    }
    $directory = Split-Path -Parent $target
    if (-not (Test-Path -LiteralPath $directory)) { New-Item -ItemType Directory -Path $directory | Out-Null }
    Copy-Item -LiteralPath $source -Destination $target -ErrorAction Stop
}
