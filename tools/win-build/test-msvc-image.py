"""Architecture selection regressions for the ImageMagick MSVC entry point."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    'image_builder', Path(__file__).with_name('build-msvc-image.py'))
assert spec is not None and spec.loader is not None
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class ArchitectureTests(unittest.TestCase):
    def test_default_and_arm64_work_directories(self):
        for arguments, directory in (([], 'image'), (['--arch', 'arm64'], 'image-arm64')):
            with self.subTest(arguments=arguments), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                resolve = Path.resolve
                with patch.object(sys, 'argv', ['build-msvc-image.py', '--prepare-only', *arguments]), \
                     patch.object(Path, 'resolve', lambda path: resolve(root / path)), \
                     patch.object(builder, 'SOURCES', {}), \
                     patch.object(builder, 'PATCHED', {}), \
                     patch.object(builder, 'patches'), \
                     patch.object(builder, 'sha', return_value='test'), \
                     contextlib.redirect_stdout(io.StringIO()):
                    builder.main()
                self.assertTrue((root / 'build/msvc-native' / directory / 'source.json').is_file())

    def test_rejects_other_architecture_source_and_output(self):
        for arch, other in (('arm64', 'x64'), ('x64', 'arm64')):
            for conflict in ('source', 'output'):
                with self.subTest(arch=arch, conflict=conflict), tempfile.TemporaryDirectory() as temporary:
                    work = Path(temporary)
                    if conflict == 'source':
                        (work / 'source/ProjectFiles' / other).mkdir(parents=True)
                    else:
                        (work / 'image-build.json').write_text(
                            json.dumps({'target': builder.TARGETS[other]}), encoding='utf-8')
                    with patch.object(sys, 'argv', ['build-msvc-image.py', '--arch', arch,
                                                   '--work-directory', str(work)]), \
                         patch.object(builder, 'source_archive') as download, \
                         contextlib.redirect_stderr(io.StringIO()), \
                         self.assertRaises(SystemExit) as error:
                        builder.main()
                    self.assertEqual(error.exception.code, 2)
                    download.assert_not_called()


if __name__ == '__main__':
    unittest.main()
