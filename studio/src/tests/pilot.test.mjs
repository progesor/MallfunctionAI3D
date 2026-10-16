import assert from 'node:assert/strict';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { validatePilot, preparePilot, toApiPrompt, DEFAULT_OPTIONS } from '../lib/workflow.ts';

// Structural smoke using the REAL pinned workflow JSON tracked in the repository.
// Mock object_info built from UI metadata is NOT a substitute for the installed
// ComfyUI /object_info schema; the first Windows end-to-end smoke checks that.
const fileDir = path.dirname(fileURLToPath(import.meta.url));
for (const name of ['pixal3d','trellis2']) {
  const file = path.resolve(fileDir,`../../../workflows/Mallfunction_PLAYBOX_FoxPlush_${name}_pilot_v01.json`);
  const graph = JSON.parse(fs.readFileSync(file,'utf8'));
  validatePilot(graph);
  assert.equal(graph.nodes.length,66);
  assert.equal(graph.links.length,100);
  const patch=preparePilot(graph, DEFAULT_OPTIONS, 'fox_test.png');
  assert.equal(patch.nodes.find(n=>n.id===122).widgets_values_named.image,'fox_test.png');
  assert.equal(graph.nodes.find(n=>n.id===122).widgets_values_named.image,'PLAYBOX_FoxPlush_reference.png');
  const info={};
  for (const node of graph.nodes) {
    if (['Note','MarkdownNote'].includes(node.type)) continue;
    if (info[node.type]) continue;
    const required={};
    const optional={};
    for(const input of (node.inputs??[])) {
      if (input.link != null) required[input.name]=['ANY',{}];
      else optional[input.name]=['ANY',{}];
    }
    // Pseudo metadata: named widget inputs; unrelated UI-only widgets are removed below.
    for(const key of Object.keys(node.widgets_values_named??{})) {
      if (!key.includes('.')) required[key]??=['ANY',{}];
    }
    info[node.type]={input:{required,optional}};
  }
  // A synthetic schema may contain UI-only 'upload'/'fixed' fields without
  // actual backend bindings; this test only validates topo and links.
  for(const def of Object.values(info)) {
    delete def.input.required.upload;
    delete def.input.required.fixed;
    delete def.input.required.viewport_state;
    delete def.input.required['control_after_generate'];
    delete def.input.optional['viewport_state'];
  }
  const result=toApiPrompt(patch,info);
  assert.ok(result.prompt['322']);
  assert.ok(result.reachable.length>25,`expected full 3D graph, got ${result.reachable.length}`);
  assert.ok(result.prompt['122']);
  assert.ok(result.prompt['186']);
  console.log(`PASS: ${name} original=${graph.nodes.length}/${graph.links.length} reachable=${result.reachable.length}`);
}
