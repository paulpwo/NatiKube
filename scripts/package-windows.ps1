<#
.SYNOPSIS
  Builds the Windows x64 release artifacts:
    target\dist\NatiKube-<version>-windows-x64.zip   (exe, LICENSE, THIRD-PARTY-NOTICES.md)
    target\dist\NatiKube-<version>-windows-x64.zip.sha256
.PARAMETER SkipBuild
  Reuse an existing target\release\natikube.exe.
#>
param([switch]$SkipBuild)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Manifest = Join-Path $Root 'Cargo.toml'

$meta = cargo metadata --manifest-path $Manifest --no-deps --format-version 1 | ConvertFrom-Json
$Version = ($meta.packages | Where-Object { $_.name -eq 'natikube-app' }).version
if (-not $Version) { throw 'could not read the natikube-app version' }

if (-not $SkipBuild) {
  cargo build --manifest-path $Manifest --release
  if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
}

$Exe = Join-Path $Root 'target\release\natikube.exe'
if (-not (Test-Path $Exe)) { throw "missing $Exe" }

$Dist = Join-Path $Root 'target\dist'
$Name = "NatiKube-$Version-windows-x64.zip"
$Zip = Join-Path $Dist $Name
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
Remove-Item -Force -ErrorAction SilentlyContinue $Zip, "$Zip.sha256"

$Stage = Join-Path ([System.IO.Path]::GetTempPath()) ("natikube-" + [guid]::NewGuid())
$Folder = Join-Path $Stage 'NatiKube'
New-Item -ItemType Directory -Force -Path $Folder | Out-Null
try {
  Copy-Item $Exe (Join-Path $Folder 'natikube.exe')
  Copy-Item (Join-Path $Root 'LICENSE') $Folder
  Copy-Item (Join-Path $Root 'packaging\THIRD-PARTY-NOTICES.md') $Folder
  Compress-Archive -Path $Folder -DestinationPath $Zip -CompressionLevel Optimal
} finally {
  Remove-Item -Recurse -Force $Stage
}

# Lower-case hex plus the bare file name, the same layout `shasum -c` expects.
$hash = (Get-FileHash -Algorithm SHA256 $Zip).Hash.ToLower()
"$hash  $Name" | Out-File -Encoding ascii -NoNewline "$Zip.sha256"
Add-Content -Encoding ascii "$Zip.sha256" ''

Write-Output "zip=$Zip"
Write-Output "sha256=$hash"
