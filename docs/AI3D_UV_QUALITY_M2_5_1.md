# M2.5.1 — Fox UV Distortion and Local PBR Fidelity
Status: code prepared; real Blender 5.1 and Unity acceptance pending.
Branch: feat/desktop-studio-foundation, Draft PR #1.
GitHub Actions manual-only; do NOT trigger CI on push.

## Source evidence from the 2026-10-15 user upload

The uploaded screenshots visibly show tail banding in the repaired Fox.
The Texture_Quality.json marked all four PBR roles passed from 31,972
surface samples, yet the physical UV anisotropy degraded severely:

| Value | Source | Rebuilt |
| --- | ---: | ---: |
| Triangles (Studio/Blender) | 7994 / 7993 | 7993 |
| UV islands | 254 | 169 |
| P95 UV anisotropy | 1.24353 | 3.67474 |
| Worst UV anisotropy | 8.62372 | 88.76794 |
| P95 ratio | 1x | 2.955x |
| Maximum ratio | 1x | 10.293x |

User's prior BaseColor mean error 0.012797, P95 0.066861 and maximum
0.474459. Roughness maximum is 0.810679, AO maximum 1.0. Both source
and new face provenance matched exactly. The observed tail artifact is
consistent with severe UV distortion but world-space normal transfer
remains unverified. Do not attribute the result solely to roughness.

## M2.5.1 strict and versioned acceptance contract

- Texture_Quality.json schemaVersion=2. Older schema=1 entries cannot
  be silently re-approved as quality-passed assets.
- Four PBR roles (BaseColor, Metallic, Roughness, AO) now require global
  mean, P95, P99, and high-error sample fraction to pass independently.
- Physical UV stretch must pass *both* relative and absolute limits:
  - rebuilt P95 <= max(1.9, 1.8 times source P95)
  - rebuilt worst <= max(24, 3.0 times source maximum)
  - zero degenerate UV-edge triangles on the rebuilt atlas
- For the uploaded Fox, allowed P95 <= 2.23835, allowed maximum <=
  25.87116. Real measured 3.67474 and 88.76794 must FAIL.
- UV island count must decrease for an auto-accepted cleaner atlas.
- The above constraints are mandatory regardless of any explicit
  user acknowledgement of one source/import triangle mismatch.
- Keep original GLB, previous reports and existing Unity prefabs
  immutable. Reject only the *new candidate* for automated approval.

Initial role-specific local artifact limits:
| Map | P99 max | Extreme-error cutoff | Extreme fraction max |
| --- | ---: | ---: | ---: |
| BaseColor | 0.250 | 0.30 | 1% |
| Metallic | 0.270 | 0.35 | 1.5% |
| Roughness | 0.270 | 0.35 | 1.5% |
| AO | 0.280 | 0.35 | 1.5% |

Limits are conservative pilot heuristics; calibrate with real assets.
Numerically passed texture data never proves normal/tangent correctness
or game readiness. Rendered tail appearance still requires visual review.

## Improved search and diagnostics

Blender now evaluates up to five seam-angle plans: requested, -12,
-24, -36 and -48 degrees (minimum 20), instead of three. A limited
island budget allows investigating cleaner lower-stretch atlases.
An atlas that does not improve island count can be inspected, but
cannot pass the automatic acceptance gate.

New UV_Stress.svg visualizes per-triangle stretch on the UV atlas:
teal low, yellow >=2, orange >=3, red >=6. Its precise path is included
in failed quality messages; output is preserved for investigation.

## Local Fox verification (without GitHub CI)

1. Pull branch, restart Tauri Studio; keep original Fox as source.
2. Use Preserve + 1024/2048 PBR preset and run ONE new UV repair.
3. If QA fails, DO NOT override it. Inspect the versioned immutable
   Texture_Quality.json and UV_Stress.svg output.
4. Share these two outputs and ideally source original.glb plus
   geometry.glb to perform exact component/tail seam analysis.
5. Do not export the rejected UV Clean model as final Unity content.

Offline commands:
  python -m unittest discover -s tests -p test_uv_fidelity.py
  python -m unittest discover -s tests -p test_uv_layer_lifecycle.py
  python -m unittest discover -s tests -p test_uv_repair_contract.py
  python -m py_compile blender/repair_uv_pbr.py blender/uv_texture_fidelity.py
  cargo test --manifest-path studio/src-tauri/Cargo.toml --lib uv_repair
  cd studio; npm run build

Real Blender/Unity/Windows pipeline testing is pending. No Cargo
or Blender runtime is installed in the assistant's execution container.

## Next planned

Use original GLB and repaired GLB/geometry to implement component-local
seam/atlas repair based on measured tail distortion. Also evaluate
world-space normal map similarity (UV tangent frames), and reversible
small hole/overlap repair with explicit before/after visual inspection.
