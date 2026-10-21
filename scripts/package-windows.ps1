# Packs the Windows build of SketchCAD as a .zip in dist/.
#
# Usage: scripts/package-windows.ps1 -Version 0.4.0 -Binary target/x86_64-pc-windows-msvc/release/sketchcad.exe
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Binary
)
$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$Dist = Join-Path $Root "dist"
$Name = "SketchCAD-v$Version-windows-x64"
$Stage = Join-Path $Dist $Name

if (Test-Path $Stage) { Remove-Item $Stage -Recurse -Force }
New-Item -ItemType Directory -Path $Stage | Out-Null

Copy-Item $Binary (Join-Path $Stage "SketchCAD.exe")
# Sample models next to the executable (the app opens samples/ relative to its folder)
Copy-Item (Join-Path $Root "samples") (Join-Path $Stage "samples") -Recurse
foreach ($doc in @("README.md", "README.en.md", "LICENSE")) {
    Copy-Item (Join-Path $Root $doc) $Stage
}

$Zip = Join-Path $Dist "$Name.zip"
if (Test-Path $Zip) { Remove-Item $Zip -Force }
Compress-Archive -Path $Stage -DestinationPath $Zip
Remove-Item $Stage -Recurse -Force

Write-Host "Created: $Zip"
