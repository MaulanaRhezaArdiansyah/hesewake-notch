# Unduh kamus IPADIC Lindera (untuk fitur "Romaji lyrics") ke dict\ipadic.
# Versi harus sama dengan crate `lindera` di Cargo.toml (format kamus ikut versi crate).
# Pakai: powershell -ExecutionPolicy Bypass -File scripts\fetch-dict.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

$ver = '6.2.0'
$dest = 'dict\ipadic'
if (Test-Path "$dest\metadata.json") {
  Write-Host "==> $dest sudah ada"
  exit 0
}

$url = "https://github.com/lindera/lindera/releases/download/v$ver/lindera-ipadic-$ver.zip"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) "lindera-ipadic-$ver"
if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
New-Item -ItemType Directory -Force $tmp | Out-Null
$zip = Join-Path $tmp 'ipadic.zip'

Write-Host "==> unduh $url"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
Expand-Archive -Path $zip -DestinationPath $tmp -Force

# Arsip berisi satu folder (lindera-ipadic\) dengan metadata.json, dict.*, matrix.mtx, NOTICE.txt, ...
$src = Get-ChildItem $tmp -Recurse -Filter 'metadata.json' | Select-Object -First 1
if (-not $src) { throw 'metadata.json tidak ditemukan di arsip kamus' }
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item -Path (Join-Path $src.DirectoryName '*') -Destination $dest -Recurse -Force
Remove-Item -Recurse -Force $tmp

Get-ChildItem $dest | Select-Object Name, Length
