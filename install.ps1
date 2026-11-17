# Windows AI one-line installer.
#   irm https://raw.githubusercontent.com/jinqo/windows-ai/main/install.ps1 | iex
#
# Downloads the latest release installer from GitHub Releases, verifies its
# SHA-256 checksum against the published SHA256SUMS.txt, and installs
# per-user (no administrator rights). Nothing else is downloaded or changed.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Repo = 'jinqo/windows-ai'
$Headers = @{ 'User-Agent' = 'windows-ai-installer'; 'Accept' = 'application/vnd.github+json' }

Write-Host "Windows AI installer" -ForegroundColor Magenta
if ([Environment]::OSVersion.Version.Build -lt 19044) { throw "Windows AI requires Windows 10 21H2 or Windows 11." }

$release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest" -Headers $Headers
$installer = $release.assets | Where-Object { $_.name -like '*x64-setup.exe' } | Select-Object -First 1
$sums = $release.assets | Where-Object { $_.name -eq 'SHA256SUMS.txt' } | Select-Object -First 1
if (-not $installer) { throw "No installer found in release $($release.tag_name)." }

$tmp = Join-Path $env:TEMP ("windows-ai-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
$exe = Join-Path $tmp $installer.name
Write-Host "Downloading $($installer.name) ($($release.tag_name))..."
Invoke-WebRequest $installer.browser_download_url -OutFile $exe -Headers $Headers

if ($sums) {
    $expected = ((Invoke-WebRequest $sums.browser_download_url -Headers $Headers).Content -split "`n" |
        Where-Object { $_ -match [regex]::Escape($installer.name) } | Select-Object -First 1) -split '\s+' | Select-Object -First 1
    $actual = (Get-FileHash $exe -Algorithm SHA256).Hash
    if (-not $expected -or $actual -ne $expected.ToUpper()) { Remove-Item $tmp -Recurse -Force; throw "Checksum mismatch - aborting." }
    Write-Host "Checksum verified."
} else {
    Write-Warning "Release has no SHA256SUMS.txt; skipping checksum verification."
}

Write-Host "Installing (per-user)..."
Start-Process -FilePath $exe -ArgumentList '/S' -Wait
Remove-Item $tmp -Recurse -Force

$app = @("$env:LOCALAPPDATA\Windows AI\windows-ai.exe", "$env:LOCALAPPDATA\Programs\Windows AI\windows-ai.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($app) { Start-Process $app }

Write-Host "Installed. Press Alt+Space to open Windows AI." -ForegroundColor Green
if (-not (Get-Command claude -ErrorAction SilentlyContinue)) {
    Write-Host "To use your Claude subscription, install Claude Code:  irm https://claude.ai/install.ps1 | iex" -ForegroundColor Yellow
    Write-Host "Then sign in from Windows AI > Settings > AI providers."
}
