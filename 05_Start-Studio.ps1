# Windows developer bootstrap for the Tauri 2 desktop studio. Existing ComfyUI
# Desktop, Blender installations and AI model folders are NOT modified.
[CmdletBinding()]
param([switch]$CheckOnly)
$ErrorActionPreference='Stop'
$base = $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($base)) { $base = Split-Path -Parent $MyInvocation.MyCommand.Path }
$root=Join-Path $base 'studio'
if (-not (Test-Path (Join-Path $root 'package.json'))) {
    throw "Studio klasoru eksik: $root. Feature branch'i cektiginden emin ol."
}
$needed=@('node','npm','rustc','cargo')
$missing=@()
foreach ($name in $needed) {
    $cmd=Get-Command $name -ErrorAction SilentlyContinue
    if ($null -eq $cmd) { $missing += $name; Write-Host "MISSING: $name" -ForegroundColor Yellow }
    else { Write-Host "OK: $name -> $($cmd.Source)" -ForegroundColor Green }
}
if ($missing.Count -gt 0) {
    Write-Host 'On kosul: Node.js LTS, Rust stable (rustup) ve Windows MSVC C++ Build Tools / WebView2.' -ForegroundColor Yellow
    Write-Host 'Rust: https://rustup.rs/  |  Tauri prerequisites: https://v2.tauri.app/start/prerequisites/'
    exit 2
}
# Validate checkout/worker source before Cargo or Vite starts.
# This does not fetch, checkout, or modify any asset.
$diag=Join-Path $base 'tools\Diagnose-Studio.ps1'
if(-not (Test-Path -LiteralPath $diag -PathType Leaf)){
    throw "UV sürüm kontrol betiği bulunamadı: $diag. Güncel feature dalını çek."
}
& $diag
if($LASTEXITCODE -ne 0){
    throw "Yanlış/eski Studio sürümü tespit edildi; yukarıdaki Git talimatlarını uygula."
}
if ($CheckOnly) { Write-Host 'Masaustu gereksinimleri ve UV kaynagi mevcut.'; exit 0 }
if(Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue){
    $inUse=@(Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue)
    if($inUse.Count -gt 0){
        throw ("Port 1420 zaten dinleniyor (PID: "+(($inUse | ForEach-Object OwningProcess) -join ',')+
            "). Eski Vite/Tauri Studio sürecini kapat, ardından yeniden başlat.")
    }
}
Push-Location $root
try {
    if (-not (Test-Path (Join-Path $root 'node_modules'))) {
        Write-Host 'Installing UI dependencies into studio/node_modules only...' -ForegroundColor Cyan
        & npm install --no-audit --no-fund
        if ($LASTEXITCODE -ne 0) { throw 'npm install failed' }
    }
    Write-Host 'Starting Mallfunction AI3D Studio (Tauri + Vite)...' -ForegroundColor Cyan
    & npm run desktop:dev
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
