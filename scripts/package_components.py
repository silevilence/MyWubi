"""Create deterministic native component archives and the application trust catalog."""
import argparse
import hashlib
import json
import re
import struct
import subprocess
import zipfile
from pathlib import Path


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def package(directory, output, version, backend='cpu', tag=None):
    directory, output = Path(directory).resolve(), Path(output).resolve()
    if not re.fullmatch(r'\d+\.\d+\.\d+', version):
        raise ValueError('Expected a release version')
    tag = tag or 'v' + version
    if tag not in ('v' + version, 'V' + version):
        raise ValueError('Tag/version mismatch')
    if output == directory or output.is_relative_to(directory):
        raise ValueError('Output must be outside payload directory')
    required = {'model-worker.exe', 'onnxruntime.dll', 'msvcp140.dll', 'msvcp140_1.dll',
                'vcruntime140.dll', 'vcruntime140_1.dll', 'model-native-runtime.json',
                'model-licenses/onnxruntime-LICENSE.txt', 'model-licenses/llama-cpp-LICENSE.txt'}
    files = sorted(p for p in directory.rglob('*') if p.is_file())
    names = {p.relative_to(directory).as_posix() for p in files}
    if not required <= names:
        raise ValueError(f'Missing native dependencies/notices: {required - names}')
    for path in directory.rglob('*'):
        if path.is_symlink() or path.is_junction():
            raise ValueError('Links are not allowed in a component')
    for path in files:
        if path.suffix.lower() in ('.gguf', '.onnx'):
            raise ValueError('Model weights must be delivered separately')
        if path.suffix.lower() in ('.dll', '.exe'):
            data = path.read_bytes()
            pe = struct.unpack_from('<I', data, 0x3c)[0]
            if data[:2] != b'MZ' or data[pe:pe+4] != b'PE\0\0' or struct.unpack_from('<H', data, pe+4)[0] != 0x8664:
                raise ValueError(f'Not PE x64: {path.name}')
    result = subprocess.run([str(directory / 'model-worker.exe'), '--features'], capture_output=True,
                            check=True, timeout=30, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
    subprocess.run([str(directory / 'model-worker.exe'), '--check-backends'], check=True,
                   capture_output=True, timeout=30, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
    features = json.loads(result.stdout)
    capabilities = ['llama', 'onnx']
    if not features.get('llama') or not features.get('onnx'):
        raise ValueError('CPU component must provide both backends')
    if backend != 'cpu':
        if not features.get(backend):
            raise ValueError('Missing requested GPU capability')
        capabilities.append(backend)
    output.mkdir(parents=True, exist_ok=True)
    name = f'MyWubi-components-{backend}-{version}-windows-x86_64.zip'
    archive = output / name
    records = []
    with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as writer:
        for path in files:
            relative = path.relative_to(directory).as_posix()
            records.append({'path': relative, 'bytes': path.stat().st_size, 'sha256': digest(path)})
            entry = zipfile.ZipInfo(relative, date_time=(2026, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            writer.writestr(entry, path.read_bytes(), compresslevel=9)
    component = {
        'id': backend, 'version': version, 'application': '=' + version,
        'os': 'windows', 'architecture': 'x86_64', 'capabilities': capabilities, 'dependencies': [],
        'source': f'https://github.com/silevilence/MyWubi/releases/download/{tag}/{name}',
        'license': 'MIT; Apache-2.0; Microsoft Visual Studio Distributable Code (see model-licenses and model-native-runtime.json)',
        'archive': {'path': name, 'bytes': archive.stat().st_size, 'sha256': digest(archive)},
        'installed_bytes': sum(f['bytes'] for f in records), 'files': records,
    }
    (output / 'components.json').write_text(json.dumps([component], ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    return component


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--version', required=True)
    parser.add_argument('--tag')
    parser.add_argument('--backend', choices=['cpu', 'vulkan', 'cuda'], default='cpu')
    args = parser.parse_args()
    print(json.dumps(package(args.directory, args.output, args.version, args.backend, args.tag), indent=2))
