"""Offline regressions for the actual Blender-independent pre-bake policy.

Run without Blender, Cargo or GitHub Actions:
  python -m unittest discover -s tests -p test_uv_planner.py
"""
import sys
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1] / 'blender'
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))
from uv_planner import evaluate_plan, select_bake_candidate, select_diagnostic_candidate

SOURCE = {'p95Anisotropy': 1.24353, 'maximumAnisotropy': 8.62372,
          'trianglesSampled': 7993, 'degenerateUvTriangles': 0}


def plan(deg, islands, p95, peak, old_count=254):
    return evaluate_plan(old_count, SOURCE, {
        'seamAngleDeg': deg, 'newUVIslands': islands, 'validBounds': True,
        'rebuiltStretch': {'p95Anisotropy': p95,
                           'maximumAnisotropy': peak, 'trianglesSampled': 7993,
                           'degenerateUvTriangles': 0},
    })


class UVPreflightRegression(unittest.TestCase):
    def test_original_fox_seam_candidates_are_not_bake_eligible(self):
        trials = [plan(62, 117, 5.45367, 65.),
                  plan(50, 169, 3.67474, 88.76794),
                  plan(38, 269, 2.87535, 27.67654),
                  plan(26, 521, 3.01041, 35.),
                  plan(20, 841, 1.72633, 18.)]
        self.assertIsNone(select_bake_candidate(trials))
        # Even the least-bad diagnosis is NEVER promoted to an accepted atlas.
        self.assertFalse(select_diagnostic_candidate(trials, 254)['passesPreflight'])

    def test_new_station_fox_smart_uv_262_to_1733_is_rejected(self):
        result = plan(0, 1733, 1.5, 9., old_count=262)
        self.assertFalse(result['passesPreflight'])
        self.assertIn('island count did not decrease', result['reasons'][0])
        self.assertIsNone(select_bake_candidate([result]))

    def test_qualified_atlas_selected_only_with_both_constraints(self):
        good = plan(55, 180, 1.6, 14.)
        bad = plan(62, 110, 4.5, 80.)
        self.assertIs(select_bake_candidate([bad, good]), good)

    def test_bounds_and_invalid_metrics_fail_closed(self):
        bad = evaluate_plan(254, SOURCE, {
            'newUVIslands': 130, 'validBounds': False,
            'rebuiltStretch': None,
        })
        self.assertIsNone(select_bake_candidate([bad]))
        rejected = evaluate_plan(254, SOURCE, {
            'newUVIslands': 140, 'validBounds': True,
            'rebuiltStretch': {'p95Anisotropy': 2.0,
                               'maximumAnisotropy': 2.,
                               'trianglesSampled': 7993,
                               'degenerateUvTriangles': 2},
        })
        self.assertFalse(rejected['passesPreflight'])


if __name__ == '__main__':
    unittest.main()
