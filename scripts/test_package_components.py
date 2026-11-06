import json
import struct
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from types import SimpleNamespace

from package_components import package, digest


class PackageComponentsTests(unittest.TestCase):
    def test_reproducible_archive_and_separate_weight_delivery(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            stage = root / 'stage'
            stage.mkdir()
            pe = bytearray(256)
            pe[:2] = b'MZ'
            struct.pack_into('<I', pe, 0x3c, 128)
            pe[128:132] = b'PE\0\0'
            struct.pack_into('<H', pe, 132, 0x8664)
            for name in ('model-worker.exe', 'onnxruntime.dll', 'msvcp140.dll', 'msvcp140_1.dll', 'vcruntime140.dll', 'vcruntime140_1.dll'):
                (stage / name).write_bytes(pe)
            (stage / 'model-native-runtime.json').write_text('{}')
            licenses = stage / 'model-licenses'
            licenses.mkdir()
            for name in ('onnxruntime-LICENSE.txt', 'llama-cpp-LICENSE.txt'):
                (licenses / name).write_text('license fixture')
            with patch('package_components.subprocess.run', return_value=SimpleNamespace(stdout=b'{"llama":true,"onnx":true}')):
                first = package(stage, root / 'one', '0.7.0')
                second = package(stage, root / 'two', '0.7.0')
                self.assertEqual(first, second)
                self.assertEqual(first['archive']['sha256'], digest(root / 'two' / second['archive']['path']))
                self.assertEqual(json.loads((root / 'one/components.json').read_text(encoding='utf-8'))[0], first)
                (stage / 'model.gguf').write_bytes(b'weights')
                with self.assertRaisesRegex(ValueError, 'separately'):
                    package(stage, root / 'three', '0.7.0')

    def test_version_and_tag_cannot_redirect_release_source(self):
        with self.assertRaises(ValueError):
            package('missing', 'output', '../untrusted')
        with self.assertRaises(ValueError):
            package('missing', 'output', '0.7.0', tag='v0.6.0')


if __name__ == '__main__':
    unittest.main()
