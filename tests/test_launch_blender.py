import contextlib
import io
import os
from pathlib import Path
import tempfile
import unittest
import sys
from unittest import mock
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'tools'))
from launch_blender import blender_candidates, find_blender, newest_fox_input, main


class TestBlenderLauncher(unittest.TestCase):
    def test_installed_blender_autodetection(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)/'Blender Foundation'
            older=root/'Blender 4.1'/'blender.exe'
            newer=root/'Blender 4.5'/'blender.exe'
            older.parent.mkdir(parents=True)
            newer.parent.mkdir(parents=True)
            older.write_bytes(b'old')
            newer.write_bytes(b'new')
            with mock.patch('launch_blender.shutil.which', return_value=None), mock.patch.dict(os.environ, {'BLENDER_EXE':''}):
                found=blender_candidates([root])
            self.assertEqual(found[0], newer)

    def test_explicit_path_with_spaces(self):
        with tempfile.TemporaryDirectory(prefix='Program Files ') as d:
            p=Path(d)/'blender.exe'
            p.write_bytes(b'exe')
            self.assertEqual(find_blender(str(p)),p.resolve())

    def test_latest_fox_glb_selected(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            first=root/'PLAYBOX_FoxPlush_pixal3d_pilot_v01_00001.glb'
            second=root/'PLAYBOX_FoxPlush_pixal3d_pilot_v01_00002.glb'
            first.write_bytes(b'a'); second.write_bytes(b'b')
            os.utime(first,(100,100)); os.utime(second,(200,200))
            self.assertEqual(newest_fox_input(root), second.resolve())

    def test_dry_run_does_not_create_output(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            model=root/'model.glb'; model.write_bytes(b'glTF')
            app=root/'blender.exe'; app.write_bytes(b'test')
            out=root/'outputs'
            with contextlib.redirect_stdout(io.StringIO()) as stdout:
                result=main(['--blender',str(app),'--input',str(model),'--outdir',str(out),'--dry-run'])
            self.assertEqual(result,0)
            self.assertIn('DRY RUN',stdout.getvalue())
            self.assertFalse(out.exists())

    @unittest.skipIf(os.name == 'nt', 'fake executable uses a Unix shebang')
    def test_background_runner_writes_expected_report(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            model=root/'model.glb'; model.write_bytes(b'glTF')
            app=root/'blender.exe'
            app.write_text('#!/usr/bin/env python3\nimport pathlib,sys\nargs=sys.argv\noutput=pathlib.Path(args[args.index("--outdir")+1])\nfor i in range(3): (output/f"PLAYBOX_FoxPlush_LOD{i}.glb").write_bytes(b"glTF")\n(output/"Blender_LOD_Report.json").write_text("{}")\nprint("OK faux Blender")\n',encoding='utf-8')
            app.chmod(0o755)
            out=root/'outputs'
            with contextlib.redirect_stdout(io.StringIO()) as stdout:
                result=main(['--blender',str(app),'--input',str(model),'--outdir',str(out)])
            self.assertEqual(result,0)
            self.assertTrue((out/'Blender_LOD_Report.json').is_file())
            self.assertTrue((out/'Blender_Run.log').is_file())
            self.assertIn('DONE:',stdout.getvalue())
            self.assertEqual(len(list(out.glob('*.glb'))),3)

    def test_refuse_overwrite(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            model=root/'model.glb'; model.write_bytes(b'glTF')
            app=root/'blender.exe'; app.write_bytes(b'test')
            out=root/'outputs'; out.mkdir(); (out/'keep.txt').write_text('keep')
            with contextlib.redirect_stderr(io.StringIO()) as stderr:
                code=main(['--blender',str(app),'--input',str(model),'--outdir',str(out),'--dry-run'])
            self.assertEqual(code,2)
            self.assertEqual((out/'keep.txt').read_text(),'keep')
            self.assertIn('bos degil',stderr.getvalue())


if __name__=='__main__': unittest.main()
