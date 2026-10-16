"""Managed xatlas 0.0.11 CPython 3.13 Windows wheel installer.

Invoked ONLY on explicit Studio user click, not during UV bake.
Requires ONLY Blender's bundled Python standard library; pip is not needed.
Downloads from PyPI, selects the exact cp313-win_amd64 wheel, validates
its upstream SHA256, extracts safely into a temporary app-owned folder,
then atomically publishes to <library>/plugins/xatlas.
"""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import sys
import tempfile
from urllib.parse import urlparse
from urllib.request import urlopen
from zipfile import ZipFile

VERSION = "0.0.11"
EXPECTED_FILENAME = f"xatlas-{VERSION}-cp313-cp313-win_amd64.whl"
JSON_URL = f"https://pypi.org/pypi/xatlas/{VERSION}/json"
MAX_WHEEL_BYTES = 5_000_000
MAX_EXTRACT_BYTES = 20_000_000


def resolve_release(download_metadata):
    files = download_metadata.get("urls", [])
    matching = [item for item in files if item.get("filename") == EXPECTED_FILENAME]
    if len(matching) != 1:
        raise ValueError(f"Exact Python 3.13 Windows xatlas wheel missing: {EXPECTED_FILENAME}")
    info = matching[0]
    url = info.get("url", "")
    parsed = urlparse(url)
    if parsed.scheme != "https" or parsed.hostname != "files.pythonhosted.org":
        raise ValueError("Unexpected xatlas wheel download host")
    sha = info.get("digests", {}).get("sha256", "")
    if len(sha) != 64 or any(c not in "0123456789abcdef" for c in sha):
        raise ValueError("PyPI wheel SHA256 is missing")
    size = int(info.get("size", -1))
    if not 1 <= size <= MAX_WHEEL_BYTES:
        raise ValueError("Wheel is larger than allowed size")
    return {"url": url, "sha256": sha, "size": size}


def unpack_verified_wheel(wheel_bytes, expected_sha256, target_dir):
    actual = hashlib.sha256(wheel_bytes).hexdigest()
    if actual != expected_sha256:
        raise ValueError("xatlas wheel SHA256 does not match PyPI digest")
    from io import BytesIO
    dest = Path(target_dir)
    dest.mkdir(exist_ok=False)
    total = 0
    with ZipFile(BytesIO(wheel_bytes)) as wheel:
        for info in wheel.infolist():
            key = PurePosixPath(info.filename)
            if (key.is_absolute() or not key.parts or
                any(part in (".", "..") for part in key.parts) or
                "\\" in info.filename or ":" in info.filename):
                raise ValueError("Unsafe wheel path rejected")
            # No filesystem symlinks or special devices in vendored native wheel.
            mode = (info.external_attr >> 16) & 0o170000
            if mode not in (0, 0o100000, 0o040000):
                raise ValueError("Wheel contains unsafe file types")
            total += info.file_size
            if total > MAX_EXTRACT_BYTES:
                raise ValueError("Wheel extraction exceeds size limit")
            target = dest.joinpath(*key.parts)
            if info.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with wheel.open(info) as source, target.open("xb") as out:
                    shutil.copyfileobj(source, out)
    if not any(dest.rglob("*.pyd")):
        raise ValueError("xatlas wheel has no Windows native extension")


def install(library):
    if sys.version_info[:2] != (3, 13) or sys.platform != "win32":
        raise RuntimeError("xatlas 0.0.11 requires Blender Python 3.13 on Windows")
    root = Path(library).resolve()
    plugin_root = root / "plugins"
    plugin_root.mkdir(parents=True, exist_ok=True)
    target = plugin_root / "xatlas"
    if target.exists():
        raise FileExistsError("xatlas app-owned plugin already exists, refusing overwrite")
    print("UV_XATLAS_STAGE:fetch_pypi_metadata", flush=True)
    with urlopen(JSON_URL, timeout=25) as stream:
        metadata = json.load(stream)
    wheel = resolve_release(metadata)
    print("UV_XATLAS_STAGE:download_pinned_wheel", flush=True)
    with urlopen(wheel["url"], timeout=45) as stream:
        raw = stream.read(MAX_WHEEL_BYTES + 1)
    if len(raw) != wheel["size"]:
        raise RuntimeError("xatlas wheel download size mismatch")
    with tempfile.TemporaryDirectory(prefix="uv_xatlas_", dir=plugin_root) as tmp:
        stage = Path(tmp) / "installed"
        print("UV_XATLAS_STAGE:verify_and_extract", flush=True)
        unpack_verified_wheel(raw, wheel["sha256"], stage)
        # Never import the .pyd BEFORE moving it on Windows: a loaded
        # native library stays file-locked, causing EBUSY/PermissionError
        # when renaming the staging folder. The Rust installer probes the
        # final plugin path from a fresh Python process AFTER publication.
        print("UV_XATLAS_STAGE:atomic_publish", flush=True)
        if target.exists():
            raise FileExistsError("Plugin directory created by another process")
        os.rename(stage, target)
    print(f"UV_XATLAS_INSTALL_OK:{target}", flush=True)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise RuntimeError("Usage: install_xatlas_wheel.py <Studio library>")
    install(sys.argv[1])
