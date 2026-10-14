import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { ObjectInfo, ApiPrompt } from './workflow';
import type { WorkflowDocument } from './api-workflow';
import type { TextureExportFile, TexturePreset } from './texture-export';

export interface Settings {
  comfyUrl: string;
  blenderPath: string;
  station: string;
  libraryRoot: string;
}
export interface Environment {
  settings: Settings;
  appDataPath: string;
  detectedBlender: string | null;
  source: string;
}
export interface Asset {
  id: string;
  title: string;
  engine: string;
  station: string;
  createdAt: string;
  fileSize: number;
  triangleCount: number;
  vertexCount: number;
  materialCount: number;
  textureCount: number;
  sha256: string;
  filePath: string;
}
export interface JobStatus {
  promptId: string;
  status: 'queued' | 'running' | 'pending_result' | 'done' | 'failed' | 'no_output';
  message: string;
  asset: Asset | null;
}
export interface WorkflowDescriptor {
  id: string;
  title: string;
  description: string;
  family: string;
  version: string;
}
export interface GpuStats {
  name: string;
  index: number;
  vramTotalMiB: number | null;
  vramUsedMiB: number | null;
  utilPercent: number | null;
  temperatureC: number | null;
}
export type LodResult = { assetId: string; lodDirectory: string; report: { lods: Array<{ triangles: number; file_bytes: number }> }; generatedAt: string };
export interface TextureExportSummary { outputDir: string; preset: string; generatedAt: string; mapCount: number }
export interface UnityExportResult {
  outputDir: string; manifestPath: string; lodTriangles: [number, number, number];
  sourceLodBytes: number; geometryLodBytes: number; sharedTextureBytes: number;
  sourceUntouched: boolean;
}
export interface UnityExportSummary { outputDir: string; generatedAt: string; triangles: [number, number, number] }
export type UnityPipeline = 'Auto' | 'URP' | 'HDRP';
export interface UnityProjectInfo {
  projectPath: string; editorVersion: string; pipeline: string;
  urpInstalled: boolean; hdrpInstalled: boolean; activePipelineVerified: boolean;
}
export interface NativeUnityResult {
  outputDir: string; unityFolder: string; manifestPath: string;
  pipeline: 'URP' | 'HDRP'; lodTriangles: [number, number, number];
  status: 'STAGED_PENDING_UNITY_EDITOR'; sourceUntouched: boolean;
}
export interface UnityImportStatus {
  status: 'STAGED_PENDING_UNITY_EDITOR' | 'UnityPrefabReady' | 'ImportFailed';
  prefabPath: string | null; message: string | null;
}
export interface TextureExportResult { outputDir: string; manifestPath: string; files: Array<{ filename: string; role: string; width: number; height: number; fileBytes: number; sha256: string }>; sourceUntouched: boolean };

export const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const bridge = {
  environment: () => invoke<Environment>('get_environment'),
  saveSettings: (settings: Settings) => invoke<Settings>('save_settings', { settings }),
  health: () => invoke<Record<string, unknown>>('comfy_health'),
  nodeInfo: () => invoke<ObjectInfo>('comfy_node_info'),
  workflows: () => invoke<WorkflowDescriptor[]>('get_workflows'),
  workflow: (id: string) => invoke<WorkflowDocument>('get_workflow_json', { id }),
  importApiWorkflow: (path: string) => invoke<WorkflowDescriptor>('comfy_import_api_workflow', { path }),
  jobDiagnostics: (promptId: string) => invoke<Record<string, unknown>>('comfy_job_diagnostics', { promptId }),
  uploadImage: (path: string) => invoke<string>('comfy_upload_image', { path }),
  submit: (request: { workflowId: string; imagePath: string; apiPrompt: ApiPrompt; uiWorkflow: unknown; config: unknown }) =>
    invoke<JobStatus>('comfy_submit_job', { request }),
  pollJob: (promptId: string) => invoke<JobStatus>('comfy_poll_job', { promptId }),
  gpu: () => invoke<GpuStats>('gpu_snapshot'),
  assets: () => invoke<Asset[]>('list_assets'),
  importGlb: (path: string) => invoke<Asset>('import_model_file', { path }),
  assetBinary: (assetId: string) => invoke<string>('read_asset_glb', { assetId }),
  assetManifest: (assetId: string) => invoke<Record<string, unknown>>('get_asset_manifest', { assetId }),
  exportTextures: (assetId: string, preset: TexturePreset, files: TextureExportFile[]) => invoke<TextureExportResult>('export_asset_textures', { assetId, preset, files }),
  textureExports: (assetId: string) => invoke<TextureExportSummary[]>('list_asset_texture_exports', { assetId }),
  exportUnityUrp: (assetId: string, textureExportDir: string) => invoke<UnityExportResult>('export_unity_urp', { assetId, textureExportDir }),
  unityExports: (assetId: string) => invoke<UnityExportSummary[]>('list_unity_exports', { assetId }),
  inspectUnityProject: (unityProjectPath: string) => invoke<UnityProjectInfo>('inspect_unity_project', { projectPath: unityProjectPath }),
  exportUnityNative: (assetId: string, textureExportDir: string, unityProjectPath: string, profile: UnityPipeline) =>
    invoke<NativeUnityResult>('export_unity_native', { assetId, textureExportDir, unityProjectPath, profile }),
  checkUnityImport: (assetId: string, outputDir: string) =>
    invoke<UnityImportStatus>('check_unity_import', { assetId, outputDir }),
  referencePreview: (path: string) => invoke<string>('read_reference_preview', { path }),
  trashAsset: (assetId: string) => invoke<string>('move_asset_to_trash', { assetId }),
  generateLods: (assetId: string) => invoke<LodResult>('blender_generate_lods', { assetId }),
  lodReport: (assetId: string) => invoke<LodResult | null>('blender_lod_report', { assetId }),
  lodBinary: (assetId: string, index: number) => invoke<string>('blender_read_lod', { assetId, index }),
  blenderDetect: () => invoke<string | null>('blender_detect'),
  pickImage: () => open({ multiple: false, directory: false, filters: [{ name: 'Reference image', extensions: ['png', 'jpg', 'jpeg', 'webp'] }] }),
  pickGlb: () => open({ multiple: false, directory: false, filters: [{ name: 'glTF Binary', extensions: ['glb'] }] }),
  pickApiWorkflow: () => open({ multiple: false, directory: false, filters: [{ name: 'ComfyUI API Workflow', extensions: ['json'] }] }),
  pickBlender: () => open({ multiple: false, directory: false, filters: [{ name: 'Blender executable', extensions: ['exe'] }] }),
  pickArchive: () => open({ multiple: false, directory: true }),
  pickUnityProject: () => open({ multiple: false, directory: true }),
};

export function formatBytes(bytes: number): string {
  const mb = bytes / 1048576;
  return mb < 1 ? `${Math.round(bytes / 1024)} KiB` : `${mb.toFixed(2)} MiB`;
}
export function shortDate(value: string): string {
  try { return new Date(value).toLocaleString('tr-TR', { dateStyle: 'short', timeStyle: 'short' }); }
  catch { return value; }
}
