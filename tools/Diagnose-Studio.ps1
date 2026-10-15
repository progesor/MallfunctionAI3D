# Diagnoses source checkout + stale Tauri/Blender UV worker, offline.
# NO git fetch/checkout, no CI, and no modification of library/Unity/ComfyUI.
[CmdletBinding()]
param(
    [string]$RepairDir = '',
    [switch]$SkipBranchCheck
)
$ErrorActionPreference='Stop'
$root=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$expectedBranch='feat/desktop-studio-foundation'
$requiredRevision='M2.8-XATLAS-TRADEOFF'
$failed=$false

function Say([string]$label,[string]$value){
    Write-Host ("{0,-26}: {1}" -f $label,$value)
}
function Fail([string]$message){
    Write-Host ("STALE / ERROR: "+$message) -ForegroundColor Red
    $script:failed=$true
}
function Has([string]$path,[string]$marker){
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $false }
    $raw=Get-Content -LiteralPath $path -Raw -Encoding UTF8
    return $raw.Contains($marker)
}
Write-Host '=== Mallfunction AI3D Studio source/runtime diagnostic ===' -ForegroundColor Cyan
Say 'Repo root' $root
if (Get-Command git -ErrorAction SilentlyContinue) {
    $branch=(& git -C $root branch --show-current 2>$null)
    $head=(& git -C $root rev-parse --short=12 HEAD 2>$null)
    Say 'Git branch' ($branch -join '')
    Say 'Git commit' ($head -join '')
    if (($branch -join '') -ne $expectedBranch -and -not $SkipBranchCheck){
        Fail "Wrong branch; expected '$expectedBranch'. Do not assume git pull updates the feature branch."
    }
    $tracked=(& git -C $root rev-parse --verify 'refs/remotes/origin/feat/desktop-studio-foundation' 2>$null)
    if($LASTEXITCODE -eq 0) {
        $localHead=(& git -C $root rev-parse HEAD 2>$null)
        if (($tracked -join '') -ne ($localHead -join '')) {
            Write-Warning 'Local HEAD differs from locally cached origin feature branch. Fetch and pull explicitly when ready.'
        }
    }
} else {
    Write-Warning 'Git command unavailable; local branch/HEAD could not be inspected'
}

$worker=Join-Path $root 'blender\repair_uv_pbr.py'
$adapter=Join-Path $root 'blender\uv_topology_blender.py'
$panel=Join-Path $root 'studio\src\components\UVRepairPanel.tsx'
$engines=Join-Path $root 'studio\src-tauri\src\uv_engines.rs'
$backend=Join-Path $root 'studio\src-tauri\src\uv_repair.rs'
$sources=@(
    @('UV worker proxy stage',$worker,'stage("build_uv_topology_proxy")'),
    @('Render normals preserved',$adapter,'transfer_clean_uv_from_proxy('),
    @('xatlas UI',$panel,'UV Repair motor seçimi'),
    @('UV UI revision',$panel,$requiredRevision),
    @('UV backend revision',$engines,$requiredRevision),
    @('Tauri UV engine command',$backend,'.arg("--uv-engine").arg(&uv_engine)')
)
foreach($check in $sources) {
    $name=$check[0];$path=$check[1];$marker=$check[2]
    $ok=Has $path $marker
    Say $name $(if($ok){'PRESENT'}else{'MISSING'})
    if(-not $ok){Fail ("Missing M2.8 feature: "+$name)}
}
if(Test-Path -LiteralPath $worker -PathType Leaf){
    $raw=Get-Content -LiteralPath $worker -Raw -Encoding UTF8
    if($raw.Contains('rebuild_exact_topology_work_copy(')){
        Fail 'LOCAL Blender worker still contains obsolete normal-reconstruction call'
    }
    $hash=(Get-FileHash -LiteralPath $worker -Algorithm SHA256).Hash.ToLowerInvariant()
    Say 'Current worker SHA256' $hash
}

# Optional comparison to the actual copied worker in a FAILED archive run.
# This diagnoses whether the user's log came from stale Rust/Tauri binary.
if(-not [string]::IsNullOrWhiteSpace($RepairDir)){
    $archived=Join-Path $RepairDir 'repair_uv_pbr.py'
    if(Test-Path -LiteralPath $archived -PathType Leaf){
        $archiveHash=(Get-FileHash -LiteralPath $archived -Algorithm SHA256).Hash.ToLowerInvariant()
        Say 'Repair-run worker SHA' $archiveHash
        if($hash -and $hash -ne $archiveHash){
            Write-Warning 'The failed run used a DIFFERENT worker from the current checkout. This is an old/stale run or binary.'
        }
        $stage=Get-Content -LiteralPath (Join-Path $RepairDir 'Repair_Run.log') -ErrorAction SilentlyContinue |
            Where-Object {$_ -like 'AI3D_UV_STAGE:*'} | Select-Object -Last 1
        if($stage){Say 'Last Blender stage' $stage}
    } else {
        Write-Warning ("Archived worker not found in: "+$RepairDir)
    }
}

# Never terminate the user's processes automatically. Only report listeners.
if(Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue) {
    $listeners=@(Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue)
    foreach($listener in $listeners){
        Say 'Vite :1420 listener PID' ([string]$listener.OwningProcess)
        Write-Warning 'Existing Vite/Tauri may still be running. Close the old Studio window/process before starting a new version.'
    }
}

if($failed){
    Write-Host 'The current checkout must be updated BEFORE attempting another expensive UV bake.' -ForegroundColor Red
    Write-Host 'Close Studio, review local changes, then run:' -ForegroundColor Yellow
    Write-Host '  git fetch origin feat/desktop-studio-foundation'
    Write-Host '  git switch feat/desktop-studio-foundation'
    Write-Host '  git pull --ff-only origin feat/desktop-studio-foundation'
    Write-Host '  .\tools\Diagnose-Studio.ps1'
    exit 2
}
Write-Host 'SOURCE PASS — UV M2.8 code is present on disk.' -ForegroundColor Green
Write-Host 'Runtime still needs a restarted, newly built Tauri process and matching UI backend version.' -ForegroundColor Yellow
exit 0
