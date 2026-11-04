# Run after placing the complete bundle in its permanent location. Per-user only.
param([Parameter(Mandatory=$true)][string]$BundlePath)
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path (Join-Path $BundlePath 'mobile.exe')).Path
$key = 'HKCU:\Software\Classes\omochi'
New-Item -Path $key -Force | Out-Null
Set-Item -Path $key -Value 'URL:Omochi Invitation'
New-ItemProperty -Path $key -Name 'URL Protocol' -Value '' -Force | Out-Null
New-Item -Path "$key\DefaultIcon" -Force | Out-Null
Set-Item -Path "$key\DefaultIcon" -Value ('"' + $exe + '",0')
New-Item -Path "$key\shell\open\command" -Force | Out-Null
Set-Item -Path "$key\shell\open\command" -Value ('"' + $exe + '" "%1"')
Write-Output 'Omochi invitation links registered for the current user.'
