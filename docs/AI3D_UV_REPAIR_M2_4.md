# M2.4 · Auto UV Atlas + PBR Repair (Fox pilot)
Date: 2026-10-15
Branch: feat/desktop-studio-foundation (Draft PR #1).
Status: code-integrated pilot; real Blender and Unity HDRP visual tests pending.
GitHub Actions: manually triggered only. Do not run CI for this patch.

## Product scope

This workflow keeps the source GLB and existing Blender LOD/Texture Export
history untouched. Rather than the user's prior Smart UV / image-by-image
scripts, it combines geometry-aware UV seam planning with explicit semantic
BaseColor, Normal, Metallic, Roughness and AO source maps from M2.1.

The default does NOT remesh or generate a new AI object. Safe UV-split vertex
welding is attempted on a disposable mesh and discarded if triangle count or
bounds change. Curvature seams + a hidden meridian cut on closed components,
Blender angle-based unwrap and island packing are attempted first. Smart UV
is a reported fallback, not silently advertised as semantic unwrapping.

No automatic anatomical segmentation is claimed; recognizing "tail", "head"
and "body" is a later optional/manual refinement.

## Studio workflow

1. Choose PLAYBOX Fox from Asset Library, SOURCE preview. Ensure 5-map PBR
   Texture Export exists; select the 1024px version (or export once).
2. Select Auto UV + Texture Repair and its latest verified source map version.
3. Choose Natural / Preserve for faithful conversion, or Peluş / kumaş to
   globally set Metallic=0, effective Roughness>=0.82 (no AI material masking
   of nose/eyes yet).
4. Choose 512/1024/2048 atlas, 1024 default. Advanced seam-angle options
   45/62/78 degrees and reversible vertex-weld checkbox.
5. Click Auto UV + PBR Repair once. Blender bakes five separate semantic maps
   and saves report, UV atlas SVG and editable .blend. Existing ComfyUI does
   not run. Source images sample explicitly from old UV, bake targets new UV.
6. Rust verifies triangles, UV island count, map PNG size, SHA of immutable
   source, and reconstructed glTF ORM channels: R=AO, G=Roughness, B=Metallic.
   The final self-contained PBR GLB has three images, five semantic roles and
   correct glTF material factors, and is stored as an independent new library
   asset. Source asset/version is not replaced.
7. Click "Temiz modeli 3D Inspector’da aç" to compare new and original PBR.
   The new asset can use the existing LOD -> Texture Export -> Unity native
   URP/HDRP export sequence. Downstream export is not yet auto-chained.

## Versioned files

Under assets/<sourceId>/uv_repairs/<newUniqueId>/:
- SourceMaps/m0_baseColor.png, m0_normal.png, m0_metallic.png,
  m0_roughness.png, m0_occlusion.png (verified immutable snapshots)
- repair_uv_pbr.py (pinned worker), Repair_Run.log
- Textures/{baseColor,normal,metallic,roughness,occlusion,ORM}.png
- UV_Layout.svg, Repair_Working.blend (both UV layers retained)
- geometry.glb and Repaired.glb (new UV, source triangles preserved)
- Repair_Report.json, Repair_Manifest.json

A successful repaired GLB is also stored as a *new* managed asset in SQLite
with parent asset ID, parent SHA and repair version provenance.

## PBR baking and limitations

EMIT bake (on source old UV) separately transfers BaseColor, Metallic,
Roughness and AO. Cycles tangent-space NORMAL bake is performed on the new
atlas; simply copying an old normal map after changing UV would be wrong.
Bake margin = 24px. BaseColor stays sRGB, scalar/normal images Non-Color.
The glTF-packed ORM is built in Rust; Photoshop edits can use separated maps.
Source baseColorFactor, AO strength and normal scale are retained.
Preserve mode keeps original metal/rough factors; Plush mode bakes effective
values into the atlas and sets both material factors to 1 to avoid multiplying
twice. Height Map is not created since Fox GLB provides no such source.

Automatic UV count and triangle gates cannot alone guarantee visually clean
normal seams or anatomically meaningful islands. Actual visual acceptance on
the Fox tail/body region is required. Glossy patches that exist in BaseColor
will not be repaired by a Metallic/Roughness preset.

Manual .blend editing is *available* as a working file, but Studio does not
yet automatically rebake edited Blender UVs. That is a specific next stage
after this auto mode is tested.

## Offline validation (no GitHub CI)

From repository root on the Windows machine:
  python -m unittest discover -s tests -p "test_uv_repair_contract.py"
  python -m py_compile blender/repair_uv_pbr.py
  cd studio
  npm run build
  npx playwright test e2e/uv-repair.spec.ts --workers=1
  cd ..
  cargo test --manifest-path studio/src-tauri/Cargo.toml --lib uv_repair

The Python static test does NOT replace real Blender Cycles baking. For real
acceptance run the UI on Fox with its existing Texture Export and ensure:
1. Source SHA unchanged; triangles unchanged; new islands <= old islands.
2. Clean UV SVG with larger coherent regions; verify no noticeable stretches.
3. Every baked PNG exists and glTF Inspector finds 5 correct semantic maps.
4. Tail-body seam in PBR/Metallic/Roughness/Normal visual inspection.
5. Fox repaired version separately imported into Unity HDRP; normal texture
   direction, transparency, material response and LOD behavior unchanged.

Do not call the pipeline "production accepted" without a real Blender and
Unity test. Never automatically run multiple models for this first pilot.

## Next iteration once Fox acceptance passes

Interactive island/seam overrides and manually edited .blend -> fresh
versioned rebake; material-region masks for plush vs plastic details;
before/after atlas and map comparison; optional downstream Unity export
chaining only after PBR validation.

## 2026-10-15 hotfix · Windows os error 2 and Blender failure visibility

Problem reported on first actual Fox test: "UV/PBR onarımı: The system cannot find
the file specified. (os error 2)". The previous Rust worker read
Repair_Report.json immediately after Blender exited; Blender can exit 0 even
when an embedded Python script failed, leaving no report. The underlying
failure cannot be determined from that generic error alone.

Fix:
- Blender invoked using `--python-exit-code 23` and script catches Python
  exceptions, prints stack traces and returns code 23 on error.
- Worker logs stage markers: input validation, GLB import, working mesh
  duplication, UV unwrap, each of the five bakes, Blender save, geometry
  export, completion.
- Backend verifies both process status AND presence of Repair_Report.json.
  Missing artifacts now report exact path, Blender stage, exit code, traceback
  tail and absolute Repair_Run.log path.
- Missing PBR input texture and output files show their exact absolute names;
  no bare os error 2 without file context.
- Studio displays a compact error with expandable detail + copyable text.
- Source GLB, existing PBR, LOD, Unity prefab unchanged. GitHub CI remains
  manual-only and was not run by this hotfix.

If it fails again, copy the error details and/or last 40 lines from the
Repair_Run.log path shown in Studio. This will identify the exact Blender
operator or PBR input that needs the next correction. No need to repeat any
ComfyUI generation, LOD or Unity export.

## 2026-10-15 · Blender 5.1.2 UnicodeDecodeError in UV unwrap (Fox)

Real user test confirmed original GLB import, working mesh duplication, and entry
to `AI3D_UV_STAGE:auto_unwrap`. The Python traceback then failed at
`uv_island_count(mesh, layer.name)` with `UnicodeDecodeError` while
reading Blender's UV layer RNA `.name` property. It is NOT evidence of an
invalid image encoding, missing source GLB, or a texture bake failure:
none of the bake stages had started.

Root cause: Blender mode switches / BMesh.to_mesh invalidate previously
retained `MeshUVLoopLayer` RNA handles. Accessing `layer.name` or
`layer.data` on a stale handle is unsafe.

Fix: capture original and clean UV names as plain Python strings before mesh
operators, and freshly look up the UV layer by its saved name after every
BMesh write-back or Edit/Object mode switch. Also apply the same rule to Smart
UV fallback, and keep the unwrap diagnostic/error stage intact.

Regression added: `tests/test_uv_layer_lifecycle.py` extracts the real
`new_uv` function and runs it against fake Blender UV layer handles that
raise `UnicodeDecodeError` if read after mode invalidation. Covers primary
angle unwrap, Smart UV fallback, and duplicate clean-UV layer names.

Run locally, **not through GitHub CI**:
```powershell
python -m unittest discover -s tests -p "test_uv_layer_lifecycle.py"
python -m unittest discover -s tests -p "test_uv_repair_contract.py"
python -m py_compile blender/repair_uv_pbr.py
```
These offline checks validate the layer lifetime behavior and Python syntax;
the user's actual Blender 5.1.2 Fox bake is still the required acceptance.
If the next run advances to `AI3D_UV_STAGE:bake_baseColor`, this specific
blocker has been passed; follow the next exact log error if another stage
fails. Original Fox, old texture exports, and prior Unity prefabs are preserved.

## 2026-10-15 · Blender 5.1 glTF WinError 123 (after successful UV & 5 maps)

Confirmed from the real workstation log: all five PBR bakes and
Repair_Working.blend save passed, but glTF exporter raised WinError 123 at
\`AI3D_UV_STAGE:export_uv_geometry\`.

**Cause:** Studio's Rust PathBuf canonicalization supplies Windows paths with
the \`\\?\C:\...\` extended-length prefix. Python reads/writes and Blender
blend saving already supported these paths. The Blender 5.1 glTF addon
normalizes the target directory with a trailing '/' and then calls
\`os.makedirs\` on the mixed extended-prefix path, which Windows rejects.

**Fix:** Only geometry.glb generation now uses a private short
\`tempfile.TemporaryDirectory(prefix="ai3d_uv_glb_")\` outside the archive,
passing the native local-drive filename to the Blender glTF add-on. The
temporary output is checked for glTF magic, version and byte length, then
copied with exclusive creation to the **newly versioned archive** (Windows
long paths are supported by ordinary Python file IO). The temp folder is
cleaned automatically. No source GLB, 5-map input, working .blend, existing
LOD, or Unity prefab is edited. The temporary file is not a public asset.

Regression: \`tests/test_blender_gltf_staging.py\` loads the REAL two helper
functions via AST without importing Blender; it checks a representative
\`\\?\C:\Users\proge\...\` path, UNC/overlong input refusal, short-path
staging with a synthetic GLB, immutable destination, and invalid/cancelled
GLB export rejection.

Offline (without GitHub Actions):
  python -m unittest discover -s tests -p "test_blender_gltf_staging.py"
  python -m unittest discover -s tests -p "test_uv_repair_contract.py"
  python -m py_compile blender/repair_uv_pbr.py

The real Blender 5.1.2 Fox export is still the acceptance gate. Retry the
automatic UV/PBR button on the same source asset: each attempt generates a
fresh version. No ComfyUI work is needed. \`AI3D_GLTF_STAGING:\` and
\`AI3D_GLTF_ARCHIVED:\` in the new Repair_Run.log confirm staging and copy.


## 2026-10-15 · Blender report acceptance rejected after GLB archive succeeded

The user's third real run reached the Rust stage after Blender wrote
Repair_Report.json and staged geometry.glb. The Studio displayed the generic
"Blender bake raporu geometri/UV kabulünü geçemedi" error, hiding WHICH
report field differed from the source.

The report validator has been extracted into verify_repair_report:
- Prints source GLB triangle count, Blender input/output triangle values,
  sourceUntouched and old/new UV island counts.
- Gives an explicit list of failing conditions and the exact existing
  Repair_Report.json path.
- Retains strict topology and UV regression rejection. It does **not**
  automatically accept a mismatch or overwrite any source/version.
- Includes synthetic unit cases for valid reports, mismatched triangles,
  missing UV data and increased islands.

The current failed UV repair folder is preserved. **Before re-running the
full five-map bake, open/upload Repair_Report.json from that folder**. Its
JSON fields allow targeted diagnosis without another long Blender run.

Read only, on user's Windows workstation:
  Get-Content -LiteralPath "<existing UV repair folder>\Repair_Report.json" -Raw

This is a diagnostic hotfix, not a declaration that the Fox repair passed.
No Windows CI was triggered; the branch's workflow remains manual-only.
