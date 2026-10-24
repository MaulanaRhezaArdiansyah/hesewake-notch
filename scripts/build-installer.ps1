# Bangun exe rilis lalu installer Inno Setup.
# Pakai: powershell -ExecutionPolicy Bypass -File scripts\build-installer.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

Write-Host '==> cargo build --release'
cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build gagal' }

# Cari ISCC.exe (Inno Setup 6): PATH, Program Files, atau instalasi per-pengguna.
$candidates = @(
  (Get-Command iscc -ErrorAction SilentlyContinue).Source,
  "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
  "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
  "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) }
if (-not $candidates) {
  throw 'ISCC.exe tidak ditemukan. Pasang Inno Setup 6 (winget install JRSoftware.InnoSetup) lalu jalankan ulang.'
}
$iscc = $candidates | Select-Object -First 1

Write-Host "==> $iscc hesewake-notch.iss"
& $iscc 'hesewake-notch.iss'
if ($LASTEXITCODE -ne 0) { throw 'ISCC gagal' }

Get-ChildItem installer_output -Filter '*.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1 Name, Length, LastWriteTime
