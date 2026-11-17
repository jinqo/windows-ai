<#
.SYNOPSIS
  DEVELOPMENT ONLY. Trusts the Windows AI dev certificate on THIS machine so
  dev-signed MSIX packages can be installed. Requires an elevated PowerShell.

  Security: this adds a certificate to LocalMachine\TrustedPeople. Only run it
  on your own development machines, never ask end users to run it. Undo with
  -Remove. Public releases must use a trusted code-signing certificate instead.
#>
param([string]$CerPath = (Join-Path $PSScriptRoot "out\WindowsAI-dev.cer"), [switch]$Remove)
$ErrorActionPreference = "Stop"
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw "Run this script in an elevated (Administrator) PowerShell." }
$cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($CerPath)
if ($Remove) {
  Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object Thumbprint -eq $cert.Thumbprint | Remove-Item
  Write-Host "Removed $($cert.Subject) from TrustedPeople."
} else {
  Import-Certificate -FilePath $CerPath -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null
  Write-Host "Trusted $($cert.Subject) ($($cert.Thumbprint)) for MSIX installs on this machine."
}
