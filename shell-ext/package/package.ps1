<#
.SYNOPSIS
  Builds and signs the Windows AI sparse MSIX (Windows 11 top-level File
  Explorer menu).

.DESCRIPTION
  Publisher comes from signing.json (selected profile) and MUST equal the
  certificate Subject exactly; the script stops if they differ.

  Certificate sources (no secrets are ever stored in the repo):
    dev profile     : the self-signed cert from new-dev-cert.ps1 (CurrentUser\My, found by subject)
    -Thumbprint     : any cert in Cert:\CurrentUser\My
    -PfxPath        : a .pfx file; password via -PfxPassword or env WINDOWS_AI_PFX_PASSWORD (CI secret)

  Requires makeappx.exe and signtool.exe (Windows SDK or the
  Microsoft.Windows.SDK.BuildTools NuGet package; set WINDOWS_SDK_BIN to its folder).

.EXAMPLE
  .\package.ps1                                  # dev profile, dev cert
  .\package.ps1 -Profile release -PfxPath C:\secure\codesign.pfx
  .\package.ps1 -AlsoSign windows-ai.exe,"Windows AI_0.1.0_x64-setup.exe"   # sign app + installer too
#>
param(
  [ValidateSet("dev", "release")] [string]$Profile,
  [string]$PfxPath,
  [securestring]$PfxPassword,
  [string]$Thumbprint,
  [string]$Version = "0.1.0.0",
  [string]$OutDir = (Join-Path $PSScriptRoot "out"),
  [string]$TimestampUrl = "http://timestamp.digicert.com",
  # Extra files (app exe, installer) signed with the same certificate.
  [string[]]$AlsoSign = @()
)
$ErrorActionPreference = "Stop"

$cfg = Get-Content (Join-Path $PSScriptRoot "signing.json") -Raw | ConvertFrom-Json
if (-not $Profile) { $Profile = $cfg.profile }
$publisher = $cfg.profiles.$Profile.publisher
if (-not $publisher -or $publisher -match "TO BE FINALIZED") { throw "Set profiles.$Profile.publisher in signing.json to the certificate's exact Subject." }

function Find-SdkTool($name) {
  if ($env:WINDOWS_SDK_BIN -and (Test-Path (Join-Path $env:WINDOWS_SDK_BIN $name))) { return (Join-Path $env:WINDOWS_SDK_BIN $name) }
  $cmd = Get-Command $name -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  $hit = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter $name -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\x64\\" } | Sort-Object FullName -Descending | Select-Object -First 1
  if (-not $hit) { throw "$name not found. Install the Windows SDK or set WINDOWS_SDK_BIN." }
  return $hit.FullName
}
$makeappx = Find-SdkTool "makeappx.exe"
$signtool = Find-SdkTool "signtool.exe"

# Resolve the certificate.
$pfxPlain = $null
if ($PfxPath) {
  if (-not $PfxPassword -and $env:WINDOWS_AI_PFX_PASSWORD) { $PfxPassword = ConvertTo-SecureString $env:WINDOWS_AI_PFX_PASSWORD -AsPlainText -Force }
  $cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($PfxPath, $PfxPassword)
  if ($PfxPassword) { $pfxPlain = [Runtime.InteropServices.Marshal]::PtrToStringBSTR([Runtime.InteropServices.Marshal]::SecureStringToBSTR($PfxPassword)) }
} elseif ($Thumbprint) {
  $cert = Get-Item "Cert:\CurrentUser\My\$Thumbprint"
} else {
  $cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $publisher -and $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date) } |
    Sort-Object NotAfter -Descending | Select-Object -First 1
  if (-not $cert) { throw "No certificate with subject '$publisher' in CurrentUser\My. Run new-dev-cert.ps1 (dev) or pass -PfxPath/-Thumbprint." }
}

# Publisher must match the certificate subject exactly.
if ($cert.Subject -ne $publisher) {
  throw "Certificate subject '$($cert.Subject)' does not match signing.json publisher '$publisher'."
}
Write-Host "Profile: $Profile | Publisher: $publisher | Cert: $($cert.Thumbprint)"

$stage = Join-Path $OutDir "stage"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path "$stage\Assets" -Force | Out-Null
$icons = Join-Path $PSScriptRoot "..\..\src-tauri\icons"
foreach ($f in "StoreLogo.png", "Square150x150Logo.png", "Square44x44Logo.png") { Copy-Item "$icons\$f" "$stage\Assets\$f" }

$manifest = (Get-Content (Join-Path $PSScriptRoot "AppxManifest.xml") -Raw).
  Replace("__PUBLISHER__", [System.Security.SecurityElement]::Escape($publisher)).
  Replace("__VERSION__", $Version).
  Replace("__PACKAGE_NAME__", $cfg.packageName)
Set-Content -Path "$stage\AppxManifest.xml" -Value $manifest -Encoding UTF8

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$msix = Join-Path $OutDir "WindowsAI.msix"
& $makeappx pack /d $stage /p $msix /nv /o
if ($LASTEXITCODE) { throw "makeappx failed" }

function Sign-File($file) {
  if ($PfxPath) {
    $a = @("sign", "/fd", "SHA256", "/tr", $TimestampUrl, "/td", "SHA256", "/f", $PfxPath)
    if ($pfxPlain) { $a += @("/p", $pfxPlain) }
    & $signtool @a $file
  } else {
    & $signtool sign /fd SHA256 /tr $TimestampUrl /td SHA256 /sha1 $cert.Thumbprint /s My $file
  }
  if ($LASTEXITCODE) { throw "signtool failed for $file" }
}
Sign-File $msix
Write-Host "Signed package: $msix" -ForegroundColor Green
foreach ($f in $AlsoSign) {
  Sign-File (Resolve-Path $f).Path
  Write-Host "Signed: $f" -ForegroundColor Green
}
