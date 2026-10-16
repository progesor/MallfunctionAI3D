"""M3 texture seam source contract, no Blender/GitHub CI required.

These invariants must never be weakened to make an auto-repair 'pass':
UV0 unchanged, original normal never blended, original GLB SHA verified,
3D geometric seam matching, conditional review-only new asset, manual
post-preview approval, and no visible face deletion.
"""
from pathlib import Path
import re
import unittest

ROOT=Path(__file__).resolve().parents[1]
S=ROOT/"studio/src-tauri/src"
C=ROOT/"studio/src/components"


class TextureSurfaceContract(unittest.TestCase):
    def read(self,name):
        return (S/name).read_text(encoding="utf-8")

    def test_source_mesh_uv_and_normal_are_never_mutated(self):
        geo=self.read("texture_seam_geometry.rs")
        pix=self.read("texture_seam_pixels.rs")
        app=self.read("texture_surface.rs")
        self.assertIn("candidate_uv_seams",geo)
        self.assertIn("intentional_sharp_edges_skipped",geo)
        self.assertIn("non_manifold_edges_skipped",geo)
        self.assertIn("source_untouched:true,uv_unchanged:true,source_normal_unchanged:true",app)
        self.assertIn('if !matches!(role,"baseColor"|"metallic"|"roughness"|"occlusion")',pix)
        self.assertIn('let normal_png=fs::read(map_dir.join("m0_normal.png"))',app)
        self.assertIn('source_geometry!=result_geometry',app)
        self.assertIn('sourceUVUnchanged":true',app)
        self.assertNotIn("bpy.ops.uv.unwrap(",geo)
        self.assertNotIn("bmesh.ops.remove_doubles(",geo)
        self.assertNotIn("texture_seam_pixels::blend_seams",self.read("uv_repair.rs"))

    def test_seam_repair_never_paints_outside_owning_uv_face(self):
        pix=self.read("texture_seam_pixels.rs")
        # Triangle center containment, not merely an inward direction
        # followed by rounded texture pixel coordinates.
        self.assertIn("fn pixel_inside_face(",pix)
        self.assertIn("if !pixel_inside_face(side,x,y,w,h)",pix)
        self.assertIn("if !pixel_inside_face(&pair.left,ax,ay,w,h)",pix)
        self.assertIn("!pixel_inside_face(&pair.right,bx,by,w,h)",pix)
        self.assertIn("thin_uv_charts_never_overpaint_neighbor_pixels",pix)

    def test_pbr_composition_is_not_reimplemented_differently(self):
        rust=self.read("texture_surface.rs")
        self.assertIn("make_orm(",rust)
        self.assertIn("build_repaired_glb(",rust)
        self.assertIn("gltf_factors(source)?",rust)
        self.assertIn("existing_texture_dir(",rust)
        self.assertIn("verify_sha256(",rust)
        self.assertIn('format!("m0_{role}.png")',rust)
        self.assertIn('role=="baseColor"',self.read("texture_seam_pixels.rs"))
        self.assertIn("max_delta>0.220001",self.read("texture_seam_pixels.rs"))
        self.assertIn("fraction>0.080001",self.read("texture_seam_pixels.rs"))

    def test_preview_review_and_explicit_approval(self):
        rust=self.read("texture_surface.rs")
        app=(ROOT/"studio/src/App.tsx").read_text(encoding="utf-8")
        panel=(C/"TextureRepairPanel.tsx").read_text(encoding="utf-8")
        lib=self.read("lib.rs")
        bridge=(ROOT/"studio/src/lib/bridge.ts").read_text(encoding="utf-8")
        self.assertIn("pub fn approve_texture_seam_review(",rust)
        self.assertIn('confirmed_visual_review:bool',rust)
        self.assertIn('"reviewOnly":true',rust)
        self.assertIn('"productionExportBlocked":true',rust)
        self.assertIn('"reviewOnly":false',rust)
        self.assertIn('texture_surface::repair_texture_seams',lib)
        self.assertIn('texture_surface::approve_texture_seam_review',lib)
        self.assertIn("surfaceRepair: (assetId:string",bridge)
        self.assertIn("surfaceApprove: (reviewAssetId:string",bridge)
        self.assertIn('data-testid="texture-seam-analyze"',panel)
        self.assertIn('data-testid="texture-seam-result"',panel)
        self.assertIn('data-testid="texture-approve-button"',panel)
        self.assertIn('reviewKind===\'texture\'',app)
        self.assertIn("const [reviewOnlyAsset,",app)

    def test_rust_omitted_leading_zero_never_breaks_tauri_build(self):
        # Rust parses cfg(test) tokens even for cargo run. A literal such
        # as [.10,.20] is invalid and prevents the desktop app launching.
        # Valid tuple fields like foo?.0 / image.get_pixel(0,0).0 and
        # format precision values {:.4} must NOT be flagged.
        bad=re.compile(r"(?<![\w.:\]\)\?])\.\d+(?![\w])")
        for correct in ("foo?.0", "rgba.get_pixel(0,0).0", "processed[3].1",
                        "{value:.4}", "0.15", "tuple.0"):
            self.assertIsNone(bad.search(correct),correct)
        for invalid in ("[.10,.20]", "let x = .45;", "end:[.80,.95]"):
            self.assertIsNotNone(bad.search(invalid),invalid)
        for path in S.glob("*.rs"):
            lines=path.read_text(encoding="utf-8").splitlines()
            for number,line in enumerate(lines,1):
                with self.subTest(file=path.name,line=number):
                    self.assertIsNone(bad.search(line),
                        f"{path.name}:{number}: invalid leading-dot float; "
                        "write 0.10 instead of .10")

    def test_dev_server_runs_rust_fractional_literal_guard_first(self):
        npm=(ROOT/"studio/package.json").read_text(encoding="utf-8")
        guard=(ROOT/"studio/scripts/check-rust-literals.mjs").read_text(
            encoding="utf-8")
        self.assertIn(
            "node scripts/check-rust-literals.mjs && node scripts/check-jsx-syntax.mjs",
            npm)
        self.assertIn("const invalidLiteral =",guard)
        self.assertIn("Rust fractional-literal guard: PASS",guard)

    def test_user_edit_did_not_get_replaced(self):
        # Explicitly preserve the user's Oct 15 one-brace UV panel fix.
        uv=(C/"UVRepairPanel.tsx").read_text(encoding="utf-8")
        self.assertIn("UV_RUNTIME_REVISION = 'M2.8-XATLAS-TRADEOFF'",uv)
        self.assertIn('aria-label="UV atlas hedefi"',uv)


if __name__=="__main__":
    unittest.main()
