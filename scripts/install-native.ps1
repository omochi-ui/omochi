param([string]$Prefix = (Join-Path $env:LOCALAPPDATA 'Programs\Omochi\bin'))
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Prefix | Out-Null
foreach ($Name in @('omochi-ui.exe', 'omochi-utils.exe')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot "bin\$Name") -Destination (Join-Path $Prefix $Name) -Force
}
Write-Host "Installed Omochi in $Prefix. Add this directory to your user PATH."
