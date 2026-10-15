# M2.5.2 — QA-Rejected UV Bake Recovery (Review-Only)

Status: code implementation, acceptance pending on Windows Blender 5.1.2.
Branch: feat/desktop-studio-foundation, Draft PR #1.
GitHub Actions stays manual-only; no CI triggered by this change.

## Incident and actual data

On Fox, the latest completed Blender 2048px Preserve UV repair returned
qualityStatus=needs-review, because:
- Original 254 UV islands -> 269 (not an easier atlas).
- UV anisotropy P95 1.24353 -> 2.87535; allowed <= 2.23835.
- Worst anisotropy 8.62372 -> 27.67654; allowed <= 25.87116.
- Occlusion P99 error 0.300251 > maximum allowed 0.280.
All five texture maps, Repair_Report.json, Texture_Quality.json,
UV_Stress.svg, geometry.glb, and working .blend were actually produced.

The QA gate is correct to refuse a successful game asset. However the
previous list_uv_repair_attempts endpoint silently filtered all results
with passed=false, AND the front end filtered by its current new-bake
resolution/profile: defaults to 1024px, last failed run was 2048px.
Thus a fully completed failed bake vanished from the recovery UI.

## Fixed user workflow

1. Choose original Fox, open Auto UV + Texture Repair.
2. All completed QA-v2 attempts are now listed, including QA FAIL and
   2048px attempts even when new-bake UI still shows default 1024px.
3. Select failed 2048px Preserve run; Studio shows quality issues:
   Occlusion, excessive UV stretch and island-count regression.
4. Acknowledge that the output failed quality and is ONLY for review.
   If the original source GLB declares 7994 triangles and Blender
   imported 7993 with no documented redundancy, separately approve
   the one import-time face loss as before.
5. Select 'QA başarısız sürümü taslak olarak kurtar (Blender çalışmaz)'.
   No Blender, ComfyUI, Unity or rebake operation is launched. The
   existing files are re-read and checked against the selected source
   asset and exact verified five texture input snapshots.
6. The catalog receives a new asset with title suffix
   'UV REVIEW (QA FAIL)'. Source and existing Unity assets remain intact.
   Quality metrics and the explicit review-only permission are written
   into immutable provenance/manifest records.
7. The Inspector visibly labels that asset QA FAIL. The Unity Native
   and URP game export UI is hidden. Both native and legacy Unity export
   backends independently reject a review-only asset by its archive
   manifest provenance flag, even if invoked directly without the UI.

The regular 'Önceki bake'i tamamla' path still requires strict QA PASS.
Failed results cannot silently be marked good by toggling the previous
one-triangle exception; review-only is a separate explicit action.

## Non-bypass validation

A review override does not bypass source SHA, ownership/UUID containment,
exact SourceMaps vs source Texture Export, report/quality consistency,
schema/version, complete Blender stage markers, PNG dimensions, material
roles, triangle preservation DURING UV repair, strict one-face import
loss permission, geometry.glb size/count, or GLB PBR reconstruction.

Only the quality acceptance (measured and recorded as failed) and the
new atlas's island-count improvement are overridden for review.
Existing failed version folders are never replaced, and a version with
Repair_Manifest.json is not re-imported a second time.

## Underlying UV repair limitation is not yet fixed

The five seam-angle plans recorded from the real Fox run were:
62 deg: 117 islands, P95 5.45367
50 deg: 169 islands, P95 3.67474
38 deg: 269 islands, P95 2.87535
26 deg: 521 islands, P95 3.01041
20 deg: 841 islands, P95 1.72633

None simultaneously meets the user's goal (fewer than 254 islands)
AND the calibrated distortion limit (P95 <= 2.23835). A stricter
quality gate cannot magically generate an acceptable atlas. A later
task must develop geometry/texture-boundary-guided seam optimization
or support a manually edited, versioned .blend UV roundtrip.

## Offline checks, no GitHub CI

  python -m unittest discover -s tests -p test_uv_repair_contract.py
  cargo test --manifest-path studio/src-tauri/Cargo.toml --lib uv_repair
  cd studio; npm run build
  npx playwright test e2e/uv-repair.spec.ts --workers=1

Real Blender/Unity visual acceptance and Cargo/TypeScript build on
the user's Windows station are pending; the assistant has no local
Rust/Blender toolchain in this environment.
