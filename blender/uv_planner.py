"""Pure pre-bake UV acceptance policy. No Blender imports or GPU work.

The worker must never silently fall back to Smart UV when all curvature seam
plans fail. A plan is bake-eligible only if it reduces island count AND
passes source-relative stretch and degenerate-UV geometry checks.
"""
import math
from uv_texture_fidelity import evaluate_uv_stretch


def evaluate_plan(original_islands, source_stretch, candidate):
    """Annotate one diagnostic candidate; never mutate input or geometry."""
    if not isinstance(original_islands, int) or original_islands < 1:
        raise ValueError('Invalid source UV island count')
    count = candidate.get('newUVIslands')
    valid_bounds = candidate.get('validBounds') is True
    new_stretch = candidate.get('rebuiltStretch')
    gate = None
    reasons = []
    if not isinstance(count, int) or count < 1:
        reasons.append('invalid UV island count')
    elif count >= original_islands:
        reasons.append('UV atlas island count did not decrease')
    if not valid_bounds:
        reasons.append('UV out of bounds or degenerate bounds')
    if not isinstance(new_stretch, dict):
        reasons.append('UV stretch unavailable')
    else:
        try:
            gate = evaluate_uv_stretch(source_stretch, new_stretch)
        except (ValueError, TypeError, KeyError, ZeroDivisionError) as exc:
            reasons.append('UV stretch invalid: ' + str(exc))
        else:
            if not gate['passed']:
                reasons.append('UV distortion exceeds source-relative limits')
    eligible = not reasons
    p95 = new_stretch.get('p95Anisotropy') if isinstance(new_stretch, dict) else None
    score = (float(p95) * (1. + .22 * count / original_islands)
             if isinstance(p95, (int, float)) and math.isfinite(p95)
             and isinstance(count, int) and count > 0 else None)
    return {**candidate, 'stretchGate': gate, 'reasons': reasons,
            'passesPreflight': eligible, 'valid': eligible,
            'score': round(score, 6) if score is not None else None}


def select_bake_candidate(candidates):
    """Return best *qualified* candidate or None; never pick rejected atlas."""
    qualified = [c for c in candidates if c.get('passesPreflight')
                 and isinstance(c.get('score'), (int, float))]
    return min(qualified, key=lambda c: c['score']) if qualified else None


def select_diagnostic_candidate(candidates, source_islands):
    """Prefer a visually inspectable island-reduced candidate, never approve it."""
    valid = [c for c in candidates if isinstance(c.get('score'), (int, float))
             and math.isfinite(c['score']) and c.get('validBounds') is True]
    if not valid:
        return None
    reduced = [c for c in valid if c.get('newUVIslands', source_islands) < source_islands]
    return min(reduced or valid, key=lambda c: c['score'])
