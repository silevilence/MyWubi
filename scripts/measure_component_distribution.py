"""Compare same-binary Windows layouts; this is not a historical installer benchmark."""
import argparse
import hashlib
import json
import zipfile
from pathlib import Path


def measure(application, release, output, model_bytes=0):
    application, release, output = map(Path, (application, release, output))
    output.mkdir(parents=True, exist_ok=True)
    catalog = json.loads((release / 'components.json').read_text(encoding='utf-8'))
    component = catalog[0]
    app = {p.relative_to(application).as_posix(): p.read_bytes() for p in application.rglob('*') if p.is_file()}
    if any(n.endswith(('model-worker.exe', 'onnxruntime.dll', '.gguf', '.onnx')) for n in app):
        raise ValueError('Light application contains optional inference payload')
    with zipfile.ZipFile(release / component['archive']['path']) as archive:
        native = {n: archive.read(n) for n in archive.namelist()}
    layouts = {'base_layout_reference': {k: v for k, v in app.items() if k != 'components.json'},
               'legacy_model_layout_reconstructed': dict(app, **native), 'unified_application': app}
    rows = {}
    for name, files in layouts.items():
        path = output / (name + '.zip')
        with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for relative, data in sorted(files.items()):
                info = zipfile.ZipInfo(relative, (2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, data, compresslevel=9)
        rows[name] = {'download_bytes': path.stat().st_size, 'installed_bytes': sum(map(len, files.values()))}
    cpu = component['archive']['bytes']
    rows['independent_cpu_component'] = {'download_bytes': cpu, 'installed_bytes': component['installed_bytes']}
    report = {
        'application': component['application'], 'architecture': component['architecture'],
        'method': 'Identical current app binaries, same ZIP deflate level; legacy layout reconstructed, not an old published build.',
        'layouts': rows,
        'first_enable_bytes': {'legacy_layout_with_weights': rows['legacy_model_layout_reconstructed']['download_bytes'] + model_bytes,
                               'unified_with_component_and_weights': rows['unified_application']['download_bytes'] + cpu + model_bytes,
                               'model_weight_download_bytes': model_bytes},
        'full_update_budget_not_measured_delta': {'app_only': rows['unified_application']['download_bytes'], 'component_only': cpu, 'unchanged_models': 0},
        'reuse_rule': 'Only compatible exact hashes in the new application catalog can be reused. Incompatible components require a new download.',
        'actual_velopack_assets': {p.name: p.stat().st_size for p in release.iterdir() if p.suffix in ('.exe', '.zip', '.nupkg')},
        'application_hashes': {n: hashlib.sha256(app[n]).hexdigest() for n in ('settings.exe', 'im_engine.dll')},
        'component_archive_sha256': component['archive']['sha256'],
    }
    (output / 'sizes.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--application', required=True, type=Path)
    parser.add_argument('--release', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--model-bytes', type=int, default=0)
    args = parser.parse_args()
    print(json.dumps(measure(args.application, args.release, args.output, args.model_bytes), indent=2))
