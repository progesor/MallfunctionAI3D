"""Offline secure xatlas wheel installer contract, no network/Blender/GitHub CI."""
import hashlib
from io import BytesIO
import importlib.util
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile

SRC=Path(__file__).resolve().parents[1]/"blender"/"install_xatlas_wheel.py"
spec=importlib.util.spec_from_file_location("xatlas_pinned_installer",SRC)
installer=importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


def fake_wheel(filename="xatlas/xatlas.cp313-win_amd64.pyd"):
    buffer=BytesIO()
    with ZipFile(buffer,"w") as z:
        z.writestr(filename,b"fake pyd test fixture; not executable")
        z.writestr("xatlas-0.0.11.dist-info/METADATA",b"Name: xatlas\nVersion: 0.0.11\n")
    return buffer.getvalue()


class XatlasInstallerSafetyTests(unittest.TestCase):
    def test_installer_uses_pinned_wheel_and_pypi_https(self):
        wheel=fake_wheel()
        digest=hashlib.sha256(wheel).hexdigest()
        result=installer.resolve_release({"urls":[{
            "filename":installer.EXPECTED_FILENAME,
            "url":"https://files.pythonhosted.org/packages/example/"+installer.EXPECTED_FILENAME,
            "digests":{"sha256":digest},"size":len(wheel)
        }]})
        self.assertEqual(result["sha256"],digest)
        self.assertIn("cp313-cp313-win_amd64",installer.EXPECTED_FILENAME)
        with tempfile.TemporaryDirectory() as folder:
            target=Path(folder)/"xatlas"
            installer.unpack_verified_wheel(wheel,digest,target)
            self.assertTrue((target/"xatlas/xatlas.cp313-win_amd64.pyd").is_file())

    def test_modified_wheel_digest_is_rejected(self):
        wheel=fake_wheel()
        with tempfile.TemporaryDirectory() as folder:
            target=Path(folder)/"xatlas"
            with self.assertRaisesRegex(ValueError,"SHA256"):
                installer.unpack_verified_wheel(wheel,"f"*64,target)
            self.assertFalse(target.exists())

    def test_wheel_zip_slip_and_unknown_host_are_rejected(self):
        wheel=fake_wheel("../evil.pyd")
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaisesRegex(ValueError,"Unsafe wheel path"):
                installer.unpack_verified_wheel(wheel,hashlib.sha256(wheel).hexdigest(),
                                                 Path(folder)/"xatlas")
        with self.assertRaisesRegex(ValueError,"host"):
            installer.resolve_release({"urls":[{
                "filename":installer.EXPECTED_FILENAME,
                "url":"https://example.net/unknown.whl",
                "digests":{"sha256":"f"*64},"size":len(wheel),
            }]})

    def test_no_automatic_install_in_uv_workflow(self):
        worker=(SRC.parent/"repair_uv_pbr.py").read_text(encoding="utf-8")
        rust=(SRC.parents[1]/"studio/src-tauri/src/uv_engines.rs").read_text(encoding="utf-8")
        self.assertNotIn("install_xatlas_wheel",worker)
        self.assertIn("install_uv_xatlas",rust)
        self.assertIn("INSTALLER_SCRIPT",rust)
        self.assertIn('"--uv-engine"',worker)
        self.assertNotIn("pip install",SRC.read_text(encoding="utf-8"))


if __name__=="__main__":
    unittest.main()
