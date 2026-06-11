# Build ssh4 in release mode and install it to %LOCALAPPDATA%\ssh4.
$ErrorActionPreference = "Stop"

Write-Host "Building ssh4 (release)..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$dest = Join-Path $env:LOCALAPPDATA "ssh4"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item "target\release\ssh4.exe" $dest -Force
Write-Host "Installed to $dest\ssh4.exe" -ForegroundColor Green

# Add the install directory to the user PATH if missing.
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$dest*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$dest", "User")
    Write-Host "Added $dest to user PATH (restart your shell to pick it up)."
}

Write-Host ""
Write-Host "Usage:"
Write-Host "  ssh4                      GUI mode"
Write-Host "  ssh4 user@host[:port]     CLI mode"
Write-Host "  ssh4 -P <profile>         CLI mode from saved profile"
Write-Host "  ssh4 user@host --once     upload clipboard image and exit"
