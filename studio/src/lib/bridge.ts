import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { ObjectInfo, ApiPrompt } from './workflow';
import type { WorkflowDocument } from './api-workflow';

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
  generateLods: (assetId: string) => invoke<LodResult>('blender_generate_lods', { assetId }),
  lodReport: (assetId: string) => invoke<LodResult | null>('blender_lod_report', { assetId }),
  lodBinary: (assetId: string, index: number) => invoke<string>('blender_read_lod', { assetId, index }),
  blenderDetect: () => invoke<string | null>('blender_detect'),
  pickImage: () => open({ multiple: false, directory: false, filters: [{ name: 'Reference image', extensions: ['png', 'jpg', 'jpeg', 'webp'] }] }),
  pickGlb: () => open({ multiple: false, directory: false, filters: [{ name: 'glTF Binary', extensions: ['glb'] }] }),
  pickApiWorkflow: () => open({ multiple: false, directory: false, filters: [{ name: 'ComfyUI API Workflow', extensions: ['json'] }] }),
  pickBlender: () => open({ multiple: false, directory: false, filters: [{ name: 'Blender executable', extensions: ['exe'] }] }),
  pickArchive: () => open({ multiple: false, directory: true }),
};

export function formatBytes(bytes: number): string {
  const mb = bytes / 1048576;
  return mb < 1 ? `${Math.round(bytes / 1024)} KiB` : `${mb.toFixed(2)} MiB`;
}
export function shortDate(value: string): string {
  try { return new Date(value).toLocaleString('tr-TR', { dateStyle: 'short', timeStyle: 'short' }); }
  catch { return value; }
}
