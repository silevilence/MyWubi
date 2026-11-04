"""Build a data-only package from trusted developer-selected model files."""
import argparse
import hashlib
import json
import shutil
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('backend', choices=['llama', 'onnx'])
    parser.add_argument('model', type=Path)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--vocabulary', type=Path)
    parser.add_argument('--prediction-policy', type=Path,
                        help='model-bound calibration JSON; absent means evaluation-only for prediction')
    parser.add_argument('--id', required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--license', required=True)
    args = parser.parse_args()
    if (args.backend == 'onnx') != (args.vocabulary is not None):
        parser.error('ONNX requires --vocabulary; GGUF must not supply it')
    for value in [args.id, args.version]:
        if not value or len(value) > 64 or '--' in value or any(not (c.isascii() and (c.isalnum() or c in '-_')) for c in value):
            parser.error('id/version must contain 1-64 ASCII letters, digits, hyphens or underscores, without --')
    args.destination.mkdir(parents=True, exist_ok=False)
    model = 'model.gguf' if args.backend == 'llama' else 'model.onnx'
    files = []
    for source, name in [(args.model, model), *([(args.vocabulary, 'vocabulary.json')] if args.vocabulary else []), *([(args.prediction_policy, 'prediction-policy.json')] if args.prediction_policy else [])]:
        out = args.destination / name
        shutil.copyfile(source, out)
        with out.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        files.append(dict(path=name, bytes=out.stat().st_size, sha256=digest))
    manifest = dict(protocol=1, id=args.id, version=args.version,
                    backend='LlamaCpp' if args.backend == 'llama' else 'OnnxRuntime',
                    adapter='gguf-continuation-v1' if args.backend == 'llama' else 'character-step-v1',
                    model=model, vocabulary='vocabulary.json' if args.vocabulary else None,
                    prediction_policy='prediction-policy.json' if args.prediction_policy else None,
                    files=files, source=args.source, license=args.license,
                    capabilities=dict(protocol=1, tasks=['Rank', 'Predict'], languages=['zh'],
                                      max_context_chars=256, max_candidates=64, max_candidate_chars=16,
                                      max_code_chars=64, proposes_words=False, incremental=True,
                                      cooperative_cancel=True))
    (args.destination / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
