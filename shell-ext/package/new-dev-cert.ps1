<#
.SYNOPSIS
  Creates a self-signed code-signing certificate for DEVELOPMENT signing of
  the Windows AI MSIX. The subject is taken from signing.json (dev profile)
  so it always matches the manifest Publisher.

  The private key stays in Cert:\CurrentUser\My (non-exportable by default).
  Only the public .cer is written (to out\), for trust-dev-cert.ps1.
  Never commit .pfx/.cer files.
#>
param([int]$Years = 2)
$ErrorActionPreference = "Stop"
$cfg = Get-Content (Join-Path $PSScriptRoot "signing.json") -Raw | ConvertFrom-Json
$subject = $cfg.profiles.dev.publisher

$existing = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $subject -and $_.NotAfter -gt (Get-Date) -and $_.HasPrivateKey } | Select-Object -First 1
if ($existing) {
  $cert = $existing
  Write-Host "Using existing dev certificate $($cert.Thumbprint)"
} else {
  $cert = New-SelfSignedCertificate -Type Custom -Subject $subject -KeyUsage DigitalSignature -FriendlyName "Windows AI MSIX dev signing" `
    -CertStoreLocation "Cert:\CurrentUser\My" -NotAfter (Get-Date).AddYears($Years) `
    -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
  Write-Host "Created dev certificate $($cert.Thumbprint)"
}
$out = Join-Path $PSScriptRoot "out"
New-Item -ItemType Directory -Force -Path $out | Out-Null
Export-Certificate -Cert $cert -FilePath (Join-Path $out "WindowsAI-dev.cer") | Out-Null
Write-Host "Subject:    $($cert.Subject)"
Write-Host "Thumbprint: $($cert.Thumbprint)"
Write-Host "Public cert: $out\WindowsAI-dev.cer"
Write-Host "Next: run trust-dev-cert.ps1 as Administrator on machines that should accept dev-signed packages."
