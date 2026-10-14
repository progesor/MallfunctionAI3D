# Unity Editor acceptance gate. Run locally on an explicitly selected test project;
# does not alter Mallfunction or other Unity projects unless their path is supplied.
param(
    [Parameter(Mandatory=$true)][string]$ProjectPath,
    [string]$UnityEditor = "",
    [string]$AssetId = "",
    [ValidateSet("URP","HDRP","")][string]$Pipeline = ""
)
$ErrorActionPreference = "Stop"
$project = (Resolve-Path -LiteralPath $ProjectPath).Path
if (!(Test-Path -LiteralPath (Join-Path $project "ProjectSettings/ProjectVersion.txt")) -or
    !(Test-Path -LiteralPath (Join-Path $project "Packages/manifest.json"))) {
    throw "ProjectPath is not a Unity project: $project"
}
if (!$UnityEditor) {
    $hub = "C:\Program Files\Unity\Hub\Editor"
    if (Test-Path $hub) {
        $UnityEditor = Get-ChildItem -Path $hub -Filter Unity.exe -File -Recurse |
          Where-Object { $_.FullName -match '[\\/]Editor[\\/]Unity\.exe$' } |
          Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
    }
}
if (!$UnityEditor -or !(Test-Path -LiteralPath $UnityEditor)) {
    throw "Unity.exe not found. Use -UnityEditor path to the installed version matching this project."
}
$imported = Join-Path $project "Assets/MallfunctionAI3D/Imported"
if (!(Test-Path $imported)) { throw "No imported AI3D assets in this Unity project." }
$marker = Get-ChildItem -LiteralPath $imported -Filter "AI3D_READY.txt" -Recurse -File
if ($AssetId) {
    $marker = @($marker | Where-Object { $_.Directory.Parent.Name -eq $AssetId })
}
if (!@($marker).Count) { throw "No AI3D native export marker found for selected asset." }
$log = Join-Path $env:TEMP ("MallfunctionAI3D_UnityAcceptance_" + [guid]::NewGuid().ToString("N") + ".log")
Write-Host "Unity Editor: $UnityEditor"
Write-Host "Project: $project"
Write-Host "Batch log: $log"
& $UnityEditor -batchmode -quit -projectPath $project -executeMethod MallfunctionAI3DImporter.ImportAll -logFile $log
if ($LASTEXITCODE -ne 0) {
    throw "Unity Editor exit code $LASTEXITCODE. Inspect $log"
}
$ok = 0
foreach ($file in $marker) {
    $reportFile = Join-Path $file.DirectoryName "unity_import_report.json"
    if (!(Test-Path -LiteralPath $reportFile)) {
        throw "Unity importer did not create report: $reportFile. Inspect $log"
    }
    $report = Get-Content -LiteralPath $reportFile -Raw | ConvertFrom-Json
    if ($Pipeline -and $report.pipeline -ne $Pipeline) { continue }
    if ($report.status -ne "UnityPrefabReady" -or $report.rendererCount -ne 3 -or
        !$report.sharedMaterial -or !$report.colliderExists -or
        @($report.lodTriangles).Count -ne 3 -or
        !(Test-Path -LiteralPath (Join-Path $project $report.prefabPath))) {
        throw "Unity acceptance FAILED: $reportFile. Inspect report and $log"
    }
    $ok++
    Write-Host ("PASS {0} {1} prefab={2}, LOD={3}" -f $report.assetId,$report.pipeline,
      $report.prefabPath,($report.lodTriangles -join "/"))
}
if (!$ok) { throw "No matching '$Pipeline' pipeline export report was validated." }
Write-Host "Unity native acceptance PASS ($ok assets). Log: $log"
