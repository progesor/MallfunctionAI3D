"""UV atlas pre-bake acceptance (pure Python; no Blender/native dependency).

Two different useful outputs must not be conflated:
- COMPACT: strictly fewer UV islands with safe source-relative distortion.
- FIDELITY TRADEOFF: at most 1.8x source islands but demonstrably BETTER
  geometric UV fidelity (p95 <=0.90x AND worst <=0.50x source).
Default 'auto' prefers compact, then permits only the strong fidelity fallback.
All options still require 5-map PBR quality, normals/provenance and triangle
checks AFTER the bake; this is never a game-ready claim by itself.
"""
import math
from uv_texture_fidelity import evaluate_uv_stretch

ISLAND_TRADEOFF_LIMITS = {
    "maxIslandRatio": 1.8,
    "maxP95Ratio": 0.90,
    "maxMaximumRatio": 0.50,
}
ATLAS_OBJECTIVES = ("auto", "compact", "fidelity")


def evaluate_island_policy(old_count, new_count, source_stretch, result_stretch,
                           objective="auto"):
    if objective not in ATLAS_OBJECTIVES:
        raise ValueError("Invalid atlas objective")
    if not isinstance(old_count,int) or not isinstance(new_count,int) or old_count<1 or new_count<1:
        raise ValueError("Missing UV island count")
    ratio = new_count / old_count
    compact = new_count < old_count
    source_p95=float(source_stretch["p95Anisotropy"])
    source_max=float(source_stretch["maximumAnisotropy"])
    target_p95=float(result_stretch["p95Anisotropy"])
    target_max=float(result_stretch["maximumAnisotropy"])
    if not all(math.isfinite(x) and x>=1 for x in
               (source_p95,source_max,target_p95,target_max)):
        raise ValueError("Invalid source/rebuilt UV stretch")
    p95_ratio=target_p95/source_p95
    peak_ratio=target_max/source_max
    fidelity_tradeoff=(not compact and ratio<=ISLAND_TRADEOFF_LIMITS["maxIslandRatio"]
                       and p95_ratio<=ISLAND_TRADEOFF_LIMITS["maxP95Ratio"]
                       and peak_ratio<=ISLAND_TRADEOFF_LIMITS["maxMaximumRatio"])
    tier=("compact" if compact else
          "fidelity-tradeoff" if fidelity_tradeoff and objective!="compact"
          else "rejected")
    return {
        "atlasObjective":objective,
        "selectionTier":tier,
        "passed":tier!="rejected",
        "uvAtlasSimplified":compact,
        "islandRatio":round(ratio,6),
        "p95Ratio":round(p95_ratio,6),
        "maximumRatio":round(peak_ratio,6),
        "limits":dict(ISLAND_TRADEOFF_LIMITS),
        "message":(
            "UV adasi azaldi" if tier=="compact" else
            "UV ada sayisi artti ancak P95 ve en kotu gerilme ciddi iyilesti; "
            "atlas elle duzenleme icin daha parcali" if tier=="fidelity-tradeoff" else
            "UV ada sayisi / yuzüey gerilmesi hedefine ulasilamadi"
        ),
    }


def evaluate_plan(original_islands, source_stretch, candidate,
                  objective="auto"):
    """Annotate one plan, fail closed on invalid or degenerate UV metrics."""
    if objective not in ATLAS_OBJECTIVES:
        raise ValueError("Invalid atlas objective")
    count=candidate.get("newUVIslands")
    valid_bounds=candidate.get("validBounds") is True
    rebuilt=candidate.get("rebuiltStretch")
    reasons=[]
    stretch=None
    policy=None
    if not isinstance(count,int) or count<1:
        reasons.append("invalid UV island count")
    elif count>=original_islands and objective=="compact":
        reasons.append("UV atlas island count did not decrease")
    if not valid_bounds:
        reasons.append("UV out of bounds or degenerate bounds")
    if not isinstance(rebuilt,dict):
        reasons.append("UV stretch unavailable")
    else:
        try:
            stretch=evaluate_uv_stretch(source_stretch,rebuilt)
        except (ValueError,TypeError,KeyError,ZeroDivisionError) as exc:
            reasons.append("UV stretch invalid: "+str(exc))
        else:
            if not stretch["passed"]:
                reasons.append("UV distortion exceeds source-relative limits")
            else:
                try:
                    policy=evaluate_island_policy(
                        original_islands,count,source_stretch,rebuilt,objective)
                except (ValueError,TypeError,KeyError,ZeroDivisionError) as exc:
                    reasons.append("UV island policy invalid: "+str(exc))
                else:
                    if not policy["passed"]:
                        reasons.append(
                            "UV island count did not decrease AND strong fidelity "
                            "tradeoff requirements were not met"
                        )
    eligible=not reasons and policy is not None and policy["passed"]
    p95=rebuilt.get("p95Anisotropy") if isinstance(rebuilt,dict) else None
    score=(float(p95)*(1.+.22*count/original_islands)
           if isinstance(p95,(int,float)) and math.isfinite(p95)
           and isinstance(count,int) and count>0 and original_islands>0
           else None)
    return {
        **candidate,
        "stretchGate":stretch,
        "islandPolicy":policy,
        "atlasObjective":objective,
        "selectionTier":policy["selectionTier"] if policy else "rejected",
        "reasons":reasons,
        "passesPreflight":bool(eligible),
        "valid":bool(eligible),
        "score":round(score,6) if score is not None else None,
    }


def select_bake_candidate(candidates, objective="auto"):
    if objective not in ATLAS_OBJECTIVES:
        raise ValueError("Invalid atlas objective")
    eligible=[c for c in candidates if c.get("passesPreflight")
              and isinstance(c.get("score"),(int,float))
              and math.isfinite(c["score"])]
    if objective=="compact":
        eligible=[c for c in eligible if c.get("selectionTier")=="compact"]
    if not eligible:return None
    if objective=="auto":
        return min(eligible,key=lambda c:(
            0 if c.get("selectionTier")=="compact" else 1,c["score"]
        ))
    return min(eligible,key=lambda c:c["score"])


def select_diagnostic_candidate(candidates, source_islands):
    valid=[c for c in candidates if isinstance(c.get("score"),(int,float))
           and math.isfinite(c["score"]) and c.get("validBounds") is True]
    if not valid:return None
    reduced=[c for c in valid if c.get("newUVIslands",source_islands)<source_islands]
    return min(reduced or valid,key=lambda c:c["score"])
