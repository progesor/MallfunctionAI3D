#!/usr/bin/env python3
"""Run the Mallfunction Blender LOD pilot without requiring blender.exe on PATH.

Standard-library only. Locates a regular Blender installation or accepts
--blender; never modifies Blender, ComfyUI, the original GLB, or user PATH.
"""
from __future__ import annotations
import argparse
import datetime as dt
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


def blender_candidates(roots: list[Path] | None = None) -> list[Path]:
    """Return candidates in priority order (newest conventional install first)."""
    found: list[Path] = []
    env_exe = os.environ.get("BLENDER_EXE")
    if env_exe:
        found.append(Path(os.path.expandvars(os.path.expanduser(env_exe.strip('"')))))
    for cmd in ("blender", "blender.exe"):
        value = shutil.which(cmd)
        if value:
            found.append(Path(value))
    if roots is None:
        roots = []
        for variable in ("ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"):
            base = os.environ.get(variable)
            if not base:
                continue
            folder = Path(base)
            roots.append(folder / "Blender Foundation")
            if variable == "LOCALAPPDATA":
                roots.append(folder / "Programs" / "Blender Foundation")
            if variable in ("ProgramFiles", "ProgramFiles(x86)"):
                roots.append(folder / "Steam" / "steamapps" / "common" / "Blender")
    for base in roots:
        if not base.is_dir():
            continue
        found.append(base / "blender.exe")
        found.extend(sorted(base.glob("Blender*/blender.exe"), reverse=True))
    distinct: list[Path] = []
    seen: set[str] = set()
    for item in found:
        try:
            resolved = item.resolve()
        except OSError:
            resolved = item
        key = str(resolved).casefold()
        if key not in seen and item.is_file():
            seen.add(key)
            distinct.append(resolved)
    return distinct


def find_blender(explicit: str | None, roots: list[Path] | None = None) -> Path:
    if explicit:
        candidate = Path(os.path.expandvars(os.path.expanduser(explicit.strip('"'))))
        if candidate.is_file():
            return candidate.resolve()
        via_path = shutil.which(explicit)
        if via_path:
            return Path(via_path).resolve()
        raise FileNotFoundError(f"Belirtilen Blender uygulamasi bulunamadi: {explicit}")
    matches = blender_candidates(roots)
    if matches:
        return matches[0]
    raise FileNotFoundError(
        "Blender.exe bulunamadi. Blender'in kurulu oldugu klasordeki blender.exe yolunu "
        "--blender ile ver: --blender \"C:\\Program Files\\Blender Foundation\\Blender 4.5\\blender.exe\". "
        "Blender arayuzunu acabiliyorsan blender.exe dosyasini Dosya Gezgini'nde bulabilirsin."
    )


def newest_fox_input(base: Path | None = None) -> Path:
    if base is None:
        loc = os.environ.get("LOCALAPPDATA")
        if not loc:
            raise FileNotFoundError("LOCALAPPDATA yok. --input ile GLB dosyasinin tam yolunu belirt.")
        base = Path(loc) / "Comfy-Desktop" / "ComfyUI-Shared" / "output" / "3d" / "Mallfunction"
    matches = sorted(base.glob("PLAYBOX_FoxPlush_pixal3d_pilot_v01_*.glb"),
                     key=lambda p: (p.stat().st_mtime, p.name), reverse=True)
    if not matches:
        raise FileNotFoundError(
            f"PLAYBOX Pixal3D GLB bulunamadi: {base}. "
            "--input \"DOSYA.glb\" parametresiyle yolunu belirt."
        )
    return matches[0].resolve()


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--blender", default=None, help="blender.exe tam yolu (otomatik bulunamazsa)")
    p.add_argument("--input", type=Path, default=None, help="Kaynak GLB (varsayilan son PLAYBOX Pixal3D ciktisi)")
    p.add_argument("--outdir", type=Path, default=None, help="LOD ciktisi klasoru")
    p.add_argument("--height-m", type=float, default=0.0, help="Gercek fiziksel yukseklik kesin biliniyorsa; normalde 0")
    p.add_argument("--triangles", type=int, nargs=3, default=(8000, 5000, 3000), metavar=("LOD0", "LOD1", "LOD2"))
    p.add_argument("--dry-run", action="store_true", help="Yalnizca yollar ve komutu goster; islem yapma")
    args = p.parse_args(argv)
    root = Path(__file__).resolve().parent.parent
    program = root / "blender" / "prepare_lods.py"
    if not program.is_file():
        p.error(f"LOD scripti bulunamadi: {program}")
    if any(n < 100 for n in args.triangles) or sorted(args.triangles, reverse=True) != list(args.triangles):
        p.error("--triangles uc adet azalan (>=100) sayi olmali")
    if args.height_m < 0 or args.height_m > 10:
        p.error("--height-m 0 (varsayilan) veya 0-10 metre arasi olmali")
    try:
        source = (args.input.resolve() if args.input else newest_fox_input())
        if not source.is_file() or source.suffix.lower() != ".glb":
            raise FileNotFoundError(f"Gecerli kaynak GLB bulunamadi: {source}")
        blender = find_blender(args.blender)
    except (FileNotFoundError, OSError) as exc:
        print("ERROR:", exc, file=sys.stderr)
        return 2

    outdir = (args.outdir.resolve() if args.outdir else
              (root / "reports" / ("FoxBlenderTest_" + dt.datetime.now().strftime("%Y%m%d_%H%M%S"))).resolve())
    if outdir == source.parent or outdir == root:
        print("ERROR: Kaynak dosya klasoru veya repo kokune cikti yazma.", file=sys.stderr)
        return 2
    if outdir.is_dir() and any(outdir.iterdir()):
        print(f"ERROR: Cikti klasoru bos degil; uzerine yazilmayacak: {outdir}", file=sys.stderr)
        return 2
    cmd = [str(blender), "-b", "--python", str(program), "--", "--input", str(source),
           "--outdir", str(outdir), "--triangles", *map(str, args.triangles)]
    if args.height_m > 0:
        cmd += ["--height-m", str(args.height_m)]
    print("Blender:", blender, flush=True)
    print("Source :", source, flush=True)
    print("Output :", outdir, flush=True)
    print("Triangles:", "/".join(map(str, args.triangles)), flush=True)
    if args.dry_run:
        print("DRY RUN: Blender calistirilmadi; herhangi bir dosya degistirilmedi.")
        return 0
    outdir.mkdir(parents=True, exist_ok=True)
    logfile = outdir / "Blender_Run.log"
    print("Calistiriliyor. Log:", logfile, flush=True)
    with logfile.open("w", encoding="utf-8") as out:
        with subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                              text=True, encoding="utf-8", errors="replace") as process:
            assert process.stdout is not None
            for line in process.stdout:
                out.write(line)
                out.flush()
                print(line, end="", flush=True)
            code = process.wait()
    if code != 0:
        print(f"ERROR: Blender return code {code}. Log: {logfile}", file=sys.stderr)
        return code if code > 0 else 1
    expected = [outdir / f"PLAYBOX_FoxPlush_LOD{i}.glb" for i in range(3)]
    expected.append(outdir / "Blender_LOD_Report.json")
    missing = [p.name for p in expected if not p.is_file()]
    if missing:
        print("ERROR: Blender completed but LOD files are missing:", ", ".join(missing), file=sys.stderr)
        return 3
    print("DONE: 3 LOD GLB + Blender_LOD_Report.json")
    print("IMPORTANT: Decimation is not new normal/AO baking; inspect texture appearance in Blender/Unity.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
