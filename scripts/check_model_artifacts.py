"""Check optional native payloads and their notices without loading any model."""
import argparse
import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def check(apk=None, windows_dir=None, gpu_dir=None):
    source = ROOT / 'model_input/licenses'
    manifest = json.loads((source / 'sources.json').read_text(encoding='utf-8'))
    for name, expected in manifest['files'].items():
        assert hashlib.sha256((source / name).read_bytes()).hexdigest() == expected, name
    if apk:
        with zipfile.ZipFile(apk) as archive:
            names = set(archive.namelist())
            for abi in ('arm64-v8a', 'x86_64'):
                for library in ('libmywubi_android.so', 'libonnxruntime.so'):
                    assert f'lib/{abi}/{library}' in names, (abi, library)
            assert not any(n.endswith(('.gguf', '.onnx')) for n in names), 'weights must be optional'
            for name, expected in manifest['files'].items():
                assert hashlib.sha256(archive.read('assets/licenses/models/' + name)).hexdigest() == expected, name
    if windows_dir:
        directory = Path(windows_dir).resolve()
        for name in ('settings.exe', 'im_engine.dll', 'model-worker.exe', 'onnxruntime.dll'):
            assert (directory / name).is_file(), name
        native = json.loads((directory / 'model-native-runtime.json').read_text(encoding='utf-8-sig'))
        required = {'msvcp140.dll', 'msvcp140_1.dll', 'vcruntime140.dll', 'vcruntime140_1.dll'}
        assert required <= {f['name'] for f in native['files']}, 'missing VC runtime dependencies'
        for item in native['files']:
            assert hashlib.sha256((directory / item['name']).read_bytes()).hexdigest() == item['sha256'], item['name']
        subprocess.run([str(directory / 'model-worker.exe'), '--check-backends'], check=True, timeout=15,
                       creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        for name, expected in manifest['files'].items():
            assert hashlib.sha256((directory / 'model-licenses' / name).read_bytes()).hexdigest() == expected, name
        assert not any(p.suffix in ('.gguf', '.onnx') for p in directory.rglob('*')), 'weights must be optional'
    if gpu_dir:
        directory = Path(gpu_dir).resolve()
        component = json.loads((directory / 'gpu-component.json').read_text(encoding='utf-8-sig'))
        assert component['schema'] == 1 and component['architecture'] == 'x86_64'
        listed = set()
        for item in component['files']:
            path = (directory / item['path']).resolve()
            assert path.is_relative_to(directory) and path not in listed
            listed.add(path)
            assert path.stat().st_size == item['bytes']
            assert hashlib.sha256(path.read_bytes()).hexdigest() == item['sha256'], item['path']
        actual = {p.resolve() for p in directory.rglob('*') if p.is_file() and p.name != 'gpu-component.json'}
        assert listed == actual, 'unlisted component files'
        assert not any(p.suffix in ('.gguf', '.onnx') for p in actual), 'weights must be optional'
        for name, expected in manifest['files'].items():
            assert hashlib.sha256((directory / 'model-licenses' / name).read_bytes()).hexdigest() == expected
        output = subprocess.run([str(directory / 'model-worker.exe'), '--features'], check=True,
                                capture_output=True, timeout=30, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        features = json.loads(output.stdout)
        assert features[component['backend']] and features['llama'] and features['onnx']
    return {'notices': len(manifest['files']), 'apk': str(apk) if apk else None,
            'windows_dir': str(windows_dir) if windows_dir else None,
            'gpu_dir': str(gpu_dir) if gpu_dir else None}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apk', type=Path)
    parser.add_argument('--windows-dir', type=Path)
    parser.add_argument('--gpu-dir', type=Path)
    args = parser.parse_args()
    print(json.dumps(check(args.apk, args.windows_dir, args.gpu_dir), indent=2))
