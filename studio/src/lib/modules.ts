/** Module boundaries are deliberate: later image generation / texture processing
 * will register independent adapters. No fake 'available' actions are exposed. */
export type ModuleKind = 'generation' | 'inspection' | 'library' | 'optimization' | 'texture' | 'image' | 'export';
export interface StudioModule {
  id: ModuleKind;
  label: string;
  description: string;
  enabled: boolean;
  pipelineInput?: 'image' | 'mesh' | 'texture';
  pipelineOutput?: 'mesh' | 'texture' | 'image' | 'package';
}
export const STUDIO_MODULES: StudioModule[] = [
  { id: 'generation', label: 'AI Generate', description: 'ComfyUI 3D workflow queue', enabled: true, pipelineInput: 'image', pipelineOutput: 'mesh' },
  { id: 'inspection', label: '3D Inspector', description: 'PBR / mesh / wireframe', enabled: true, pipelineInput: 'mesh', pipelineOutput: 'mesh' },
  { id: 'library', label: 'Asset Library', description: 'Immutable originals + variants', enabled: true },
  { id: 'optimization', label: 'Blender Optimize', description: '8K, 5K, 3K LOD', enabled: true, pipelineInput: 'mesh', pipelineOutput: 'mesh' },
  { id: 'texture', label: 'Texture Lab', description: 'Channel packing, map resize, KTX2 (planned)', enabled: false, pipelineInput: 'texture', pipelineOutput: 'texture' },
  { id: 'image', label: 'AI Image', description: 'ComfyUI image generation (planned)', enabled: false, pipelineInput: 'image', pipelineOutput: 'image' },
  { id: 'export', label: 'Unity Export', description: 'Prefab / material package (planned)', enabled: false, pipelineInput: 'mesh', pipelineOutput: 'package' },
];
