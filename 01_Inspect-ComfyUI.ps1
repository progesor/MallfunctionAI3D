# Read-only inspection of ComfyUI Desktop (also supports Docker/WSL), Windows GPU and installed models.
# Does not download models, update packages, restart containers or change user files.
[CmdletBinding()]
param(
    [string]$ComfyUrl = 'http://127.0.0.1:8188',
    [string]$DockerContainer = '',
    [string]$ReportRoot = ''
)
$ErrorActionPreference = 'Stop'
# PowerShell 5.1 does not reliably initialize $PSScriptRoot inside param() defaults.
# Resolve it only after the parameter binder has finished. Never touch the
# ComfyUI installation; reports are always written to this pilot directory
# (unless explicitly overridden with -ReportRoot).
$bundleRoot = $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($bundleRoot)) {
    $scriptPath = $MyInvocation.MyCommand.Path
    if (-not [string]::IsNullOrWhiteSpace($scriptPath)) {
        $bundleRoot = Split-Path -Parent $scriptPath
    }
}
if ([string]::IsNullOrWhiteSpace($bundleRoot)) {
    $bundleRoot = (Get-Location).Path
}
if ([string]::IsNullOrWhiteSpace($ReportRoot)) {
    $ReportRoot = Join-Path -Path $bundleRoot -ChildPath 'reports'
}
$ReportRoot = [System.IO.Path]::GetFullPath($ReportRoot)
$ComfyUrl = $ComfyUrl.TrimEnd('/')
$stamp = Get-Date -Format 'yyyyMMdd_HHmmss'
$folder = Join-Path $ReportRoot ("Comfy_Audit_" + $stamp)
New-Item -ItemType Directory -Path $folder -Force | Out-Null
$warnings = New-Object 'System.Collections.Generic.List[string]'
function LogWarn([string]$msg) { $warnings.Add($msg); Write-Host "WARNING: $msg" -ForegroundColor Yellow }
function SaveJson([string]$filename, $value) {
    $json = ConvertTo-Json -InputObject $value -Depth 50
    [System.IO.File]::WriteAllText((Join-Path $folder $filename), $json, (New-Object System.Text.UTF8Encoding $false))
}
function TryGet([string]$path, [int]$timeout = 15) {
    try { return Invoke-RestMethod -Uri ($ComfyUrl + $path) -Method Get -TimeoutSec $timeout }
    catch { LogWarn ("GET " + $path + " failed: " + $_.Exception.Message); return $null }
}
function TryCommand([string]$name, [string[]]$arguments, [string]$filename) {
    if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
        LogWarn "$name is not available in PATH"; return
    }
    try {
        $lines = & $name @arguments 2>&1 | Out-String
        [System.IO.File]::WriteAllText((Join-Path $folder $filename), $lines)
    } catch { LogWarn ("Command failed " + $name + ": " + $_.Exception.Message) }
}

Write-Host "Inspecting local ComfyUI $ComfyUrl (read-only)"
$stats = TryGet '/system_stats'
if ($null -ne $stats) { SaveJson 'system_stats.json' $stats }
$models_categories = TryGet '/models'
$fullModelInventory = @{}
if ($null -ne $models_categories) {
    foreach ($category in @($models_categories)) {
        # Valid directory categories only; never interpolate arbitrary URLs.
        if ([string]$category -notmatch '^[a-zA-Z0-9_-]+$') { continue }
        try {
            $arr = Invoke-RestMethod -Uri ($ComfyUrl + '/models/' + [string]$category) -Method Get -TimeoutSec 20
            $fullModelInventory[[string]$category] = @($arr)
        } catch {
            LogWarn ("Model inventory unavailable for category " + $category)
        }
    }
} else {
    # Some forks don't expose /models; try the mandatory categories.
    foreach ($cat in @('diffusion_models','vae','clip_vision','geometry_estimation','background_removal')) {
        try {
            $arr = Invoke-RestMethod -Uri ($ComfyUrl + '/models/' + $cat) -Method Get -TimeoutSec 20
            $fullModelInventory[$cat] = @($arr)
        } catch { LogWarn "Model category not exposed: $cat" }
    }
}
SaveJson 'all_model_names_by_category.json' $fullModelInventory

$wanted = @(
    @{category='diffusion_models'; name='pixal3d_int8_convrot.safetensors'},
    @{category='diffusion_models'; name='trellis_2_int8_convrot.safetensors'},
    @{category='vae'; name='trellis_2_shape_vae_bf16.safetensors'},
    @{category='vae'; name='trellis_2_texture_vae_bf16.safetensors'},
    @{category='clip_vision'; name='dino_v3_L_naf_fp32.safetensors'},
    @{category='geometry_estimation'; name='moge_2_vitl_normal_fp16.safetensors'},
    @{category='background_removal'; name='birefnet.safetensors'}
)
$requiredModels = @()
foreach ($target in $wanted) {
    $hits = @()
    if ($fullModelInventory.ContainsKey($target.category)) {
        foreach ($file in @($fullModelInventory[$target.category])) {
            $name = ([string]$file).Replace('\\','/')
            if ($name -eq $target.name -or $name.EndsWith('/' + $target.name, [System.StringComparison]::OrdinalIgnoreCase)) {
                $hits += [string]$file
            }
        }
    }
    $requiredModels += [pscustomobject]@{category=$target.category; filename=$target.name; present=($hits.Count -gt 0); entries=$hits}
}
SaveJson 'required_models.json' $requiredModels

$requiredNodeNames = @('PrimitiveBoolean','PrimitiveInt','LoadImage','UNETLoader','VAELoader','CLIPVisionLoader',
    'Trellis2ShapeStage','Trellis2UpsampleStage','Trellis2TextureStage','VaeDecodeStructureTrellis2',
    'VaeDecodeShapeTrellis','VaeDecodeTextureTrellis','Pixal3DConditioning','Trellis2Conditioning',
    'LoadMoGeModel','MoGeInference','LoadBackgroundRemovalModel','RemoveBackground','ComfySwitchNode',
    'RemeshMesh','DecimateMesh','UnwrapMesh','BakeTextureFromVoxel','BakeNormalMapFromMesh',
    'BakeAmbientOcclusion','ApplyTextureToMesh','MeshToFile3D','Save3DAdvanced','Preview3DAdvanced')
# Explicitly parse the HTTP response body. On some ComfyUI Desktop / Windows
# PowerShell combinations, Invoke-RestMethod may return a String rather than
# a PSCustomObject; in v0.1.1 that caused "Length" to be reported as the only
# node type and a false "0/29 nodes" result.
function Get-NodeNamesFromApi([string]$path, [int]$timeoutSeconds = 45) {
    $response = Invoke-WebRequest -Uri ($ComfyUrl + $path) -Method Get -UseBasicParsing -TimeoutSec $timeoutSeconds -Headers @{Accept='application/json'}
    if ($response.StatusCode -ne 200) { throw "HTTP $($response.StatusCode) for $path" }
    $raw = [string]$response.Content
    if ([string]::IsNullOrWhiteSpace($raw)) { throw "Empty JSON for $path" }
    $obj = ConvertFrom-Json -InputObject $raw -ErrorAction Stop
    # Handles an unusual double-serialized response, without guessing from
    # object/string Length properties or treating an API failure as missing.
    for ($i = 0; ($i -lt 2) -and ($obj -is [string]); $i++) {
        $obj = ConvertFrom-Json -InputObject $obj -ErrorAction Stop
    }
    if ($obj -is [System.Collections.IDictionary]) {
        return @($obj.Keys | ForEach-Object { [string]$_ })
    }
    if ($obj -is [pscustomobject]) {
        return @($obj.PSObject.Properties | ForEach-Object { [string]$_.Name })
    }
    throw "Unexpected object_info response type: $($obj.GetType().FullName)"
}
$requiredNodes = @()
$allNodeNames = @()
$nodeCheckMethod = 'bulk'
try {
    $allNodeNames = @(Get-NodeNamesFromApi '/object_info' 90)
    if ($allNodeNames.Count -lt 20 -or ($allNodeNames.Count -eq 1 -and $allNodeNames[0] -eq 'Length')) {
        throw "Invalid bulk node list: $($allNodeNames.Count) keys"
    }
    [System.IO.File]::WriteAllLines((Join-Path $folder 'all_available_node_types.txt'), [string[]]$allNodeNames)
    foreach ($type in $requiredNodeNames) {
        $present = $allNodeNames -ccontains $type
        $requiredNodes += [pscustomobject]@{type=$type;present=$present;status=$(if($present){'present'}else{'missing'});method='bulk'}
    }
} catch {
    LogWarn "Bulk /object_info could not be parsed reliably: $($_.Exception.Message); checking individual node endpoints instead"
    $nodeCheckMethod = 'per-node'
    foreach ($type in $requiredNodeNames) {
        try {
            $path = '/object_info/' + [System.Uri]::EscapeDataString($type)
            $returned = @(Get-NodeNamesFromApi $path 20)
            $present = $returned -ccontains $type
            $requiredNodes += [pscustomobject]@{type=$type;present=$present;status=$(if($present){'present'}else{'missing'});method='per-node'}
        } catch {
            # Important: unverified is NOT missing.
            LogWarn "Node $type could not be verified: $($_.Exception.Message)"
            $requiredNodes += [pscustomobject]@{type=$type;present=$null;status='unknown';method='per-node'}
        }
    }
}
SaveJson 'required_nodes.json' $requiredNodes
SaveJson 'node_check_info.json' ([pscustomobject]@{
    method=$nodeCheckMethod
    total_required=$requiredNodeNames.Count
    full_node_count=$(if($nodeCheckMethod -eq 'bulk'){$allNodeNames.Count}else{$null})
})

# Host-only diagnostics; no Docker environment variables, secrets or model file modifications.
TryCommand 'nvidia-smi' @('--query-gpu=index,name,driver_version,memory.total,memory.used,utilization.gpu,temperature.gpu','--format=csv') 'nvidia_smi.txt'
TryCommand 'docker' @('ps','--format','{{.Names}} | {{.Image}} | {{.Status}} | {{.Ports}}') 'docker_ps.txt'
TryCommand 'docker' @('stats','--no-stream','--format','{{.Name}} | {{.MemUsage}} | {{.CPUPerc}}') 'docker_stats.txt'
TryCommand 'wsl.exe' @('--list','--verbose') 'wsl_distributions.txt'
# Optional targeted container inspection is limited to read-only version checks.
if ($DockerContainer -ne '') {
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        LogWarn 'Docker not found; cannot inspect selected container'
    } else {
        TryCommand 'docker' @('inspect','--format','{{json .Mounts}}',$DockerContainer) 'selected_container_mounts.json'
        $pyProbe='import json,sys; d={"python":sys.version}; import torch; d.update(torch_version=torch.__version__, cuda_build=torch.version.cuda, cuda_available=torch.cuda.is_available(), device=torch.cuda.get_device_name(0) if torch.cuda.is_available() else None); print(json.dumps(d))'
        TryCommand 'docker' @('exec',$DockerContainer,'python','-c',$pyProbe) 'selected_container_torch.json'
    }
}
try {
    $cs=Get-CimInstance Win32_ComputerSystem
    $os=Get-CimInstance Win32_OperatingSystem
    SaveJson 'host_memory.json' ([pscustomobject]@{total_physical_bytes=$cs.TotalPhysicalMemory;free_physical_kb=$os.FreePhysicalMemory;os=$os.Caption;ps_version=$PSVersionTable.PSVersion.ToString()})
} catch { LogWarn "Cannot read host RAM: $($_.Exception.Message)" }
SaveJson 'warnings.json' @($warnings)
$missingModels=@($requiredModels | Where-Object { -not $_.present })
$missingNodes=@($requiredNodes | Where-Object { $_.status -eq 'missing' })
$presentNodes=@($requiredNodes | Where-Object { $_.status -eq 'present' })
$unknownNodes=@($requiredNodes | Where-Object { $_.status -eq 'unknown' })
$summary = @(
    'MALLFUNCTION COMFYUI READ-ONLY AUDIT',
    'Timestamp: ' + (Get-Date -Format o),
    'ComfyUI URL: ' + $ComfyUrl,
    'API /system_stats: ' + [bool]($null -ne $stats),
    'Models found: ' + ($requiredModels.Count - $missingModels.Count) + '/' + $requiredModels.Count,
    'Nodes found: ' + $presentNodes.Count + '/' + $requiredNodes.Count,
    'Node status unknown: ' + $unknownNodes.Count,
    'Node check method: ' + $nodeCheckMethod,
    'Missing models: ' + (($missingModels | ForEach-Object {$_.filename}) -join ', '),
    'Missing nodes (confirmed): ' + (($missingNodes | ForEach-Object {$_.type}) -join ', '),
    'Unknown nodes (unverified): ' + (($unknownNodes | ForEach-Object {$_.type}) -join ', '),
    'Warnings: ' + $warnings.Count,
    'No local installation or model weights have been changed.'
)
$summary | Set-Content -Path (Join-Path $folder 'SUMMARY.txt') -Encoding UTF8
$zip = "$folder.zip"
try {
    Compress-Archive -Path (Join-Path $folder '*') -DestinationPath $zip -CompressionLevel Optimal
    Write-Host "`nAUDIT READY: $zip" -ForegroundColor Green
} catch { LogWarn "Zip failed; report folder still available: $folder" }
$summary | ForEach-Object {Write-Host $_}
