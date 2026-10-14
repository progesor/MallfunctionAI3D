# Read-only inspection of local ComfyUI, Windows GPU, and Docker/WSL.
# Does not download models, update packages, restart containers or change user files.
[CmdletBinding()]
param(
    [string]$ComfyUrl = 'http://127.0.0.1:8188',
    [string]$DockerContainer = '',
    [string]$ReportRoot = (Join-Path $PSScriptRoot 'reports')
)
$ErrorActionPreference = 'Stop'
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
$info = TryGet '/object_info' 45
$requiredNodes = @()
if ($null -ne $info) {
    $names = @($info.PSObject.Properties.Name)
    foreach ($type in $requiredNodeNames) {
        $requiredNodes += [pscustomobject]@{type=$type;present=($names -contains $type)}
    }
    SaveJson 'required_nodes.json' $requiredNodes
    [System.IO.File]::WriteAllLines((Join-Path $folder 'all_available_node_types.txt'),$names)
} else {
    LogWarn 'Cannot inspect node types; API unreachable or unauthorized'
}

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
$missingNodes=@($requiredNodes | Where-Object { -not $_.present })
$summary = @(
    'MALLFUNCTION COMFYUI READ-ONLY AUDIT',
    'Timestamp: ' + (Get-Date -Format o),
    'ComfyUI URL: ' + $ComfyUrl,
    'API /system_stats: ' + [bool]($null -ne $stats),
    'Models found: ' + ($requiredModels.Count - $missingModels.Count) + '/' + $requiredModels.Count,
    'Nodes found: ' + ($requiredNodes.Count - $missingNodes.Count) + '/' + $requiredNodes.Count,
    'Missing models: ' + (($missingModels | ForEach-Object {$_.filename}) -join ', '),
    'Missing nodes: ' + (($missingNodes | ForEach-Object {$_.type}) -join ', '),
    'Warnings: ' + $warnings.Count,
    'No local installation or model weights have been changed.'
)
$summary | Set-Content -Path (Join-Path $folder 'SUMMARY.txt') -Encoding UTF8
$zip = "$folder.zip"
try {
    Compress-Archive -Path (Join-Path $folder '*') -DestinationPath $zip -CompressionLevel Optimal
    Write-Host "\nAUDIT READY: $zip" -ForegroundColor Green
} catch { LogWarn "Zip failed; report folder still available: $folder" }
$summary | ForEach-Object {Write-Host $_}
