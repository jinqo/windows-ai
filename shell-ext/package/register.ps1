<#
.SYNOPSIS
  Registers (or removes) the signed sparse package for the current user so
  "Windows AI" appears in the Windows 11 top-level context menu.
  The app folder must contain windows-ai.exe and windows_ai_shell.dll.
#>
param(
  [string]$Msix = (Join-Path $PSScriptRoot "out\WindowsAI.msix"),
  [string]$AppDir = "$env:LOCALAPPDATA\Windows AI",
  [switch]$Uninstall
)
$ErrorActionPreference = "Stop"
if ($Uninstall) {
  Get-AppxPackage -Name "WindowsAI" | Remove-AppxPackage
  Write-Host "Removed the Windows 11 context menu package."
  return
}
foreach ($f in "windows-ai.exe", "windows_ai_shell.dll") {
  if (-not (Test-Path (Join-Path $AppDir $f))) { throw "$f not found in $AppDir" }
}
Add-AppxPackage -Path $Msix -ExternalLocation $AppDir
Write-Host "Registered. Right-click a file in Explorer to see Windows AI (restart Explorer if it does not appear)." -ForegroundColor Green
