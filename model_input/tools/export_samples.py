"""Export deterministic interface fixtures; requires numpy, onnx and onnxruntime.

These are compatibility fixtures, not models validated for product quality.
The count model learns only the authored train split. LSTM weights are synthetic.
No downloaded model or model-provided Python is executed.
"""
import argparse
import json
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
from onnx import TensorProto as T, helper as H, numpy_helper as N


def export(destination: Path):
    samples = json.loads((Path(__file__).parents[1] / 'evaluation/samples.json').read_text(encoding='utf-8'))
    texts = [s['input']['prefix'] + s['target'] for s in samples if s['split'] == 'train']
    tokens = ['<unk>', '<bos>'] + sorted(set(''.join(texts) + '下周去西安仙'))
    ids = {token: i for i, token in enumerate(tokens)}
    v, hidden = len(tokens), 4
    destination.mkdir(parents=True, exist_ok=True)
    (destination / 'vocabulary.json').write_text(json.dumps(dict(adapter='character-step-v1', tokens=tokens, hidden_size=hidden), ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    inputs = [H.make_tensor_value_info('x', T.FLOAT, [1, 1, v]), H.make_tensor_value_info('h', T.FLOAT, [1, 1, hidden]), H.make_tensor_value_info('c', T.FLOAT, [1, 1, hidden])]
    outputs = [H.make_tensor_value_info('logits', T.FLOAT, [1, v]), H.make_tensor_value_info('next_h', T.FLOAT, [1, 1, hidden]), H.make_tensor_value_info('next_c', T.FLOAT, [1, 1, hidden])]
    counts = np.ones((v, v), dtype=np.float32)
    for text in texts:
        previous = 1
        for char in text:
            current = ids[char]
            counts[previous, current] += 1
            previous = current
    weights = np.log(counts / counts.sum(axis=1, keepdims=True))
    rng = np.random.default_rng(42)
    w = rng.normal(0, .1, (1, 4 * hidden, v)).astype(np.float32)
    r = rng.normal(0, .1, (1, 4 * hidden, hidden)).astype(np.float32)
    b = rng.normal(0, .1, (1, 8 * hidden)).astype(np.float32)
    decoder = rng.normal(0, .1, (hidden, v)).astype(np.float32)
    audit = {}
    for kind in ['counts', 'lstm']:
        if kind == 'counts':
            initializers = [N.from_array(weights, 'weights'), N.from_array(np.array([1, v], dtype=np.int64), 'shape')]
            nodes = [H.make_node('Reshape', ['x', 'shape'], ['flat']), H.make_node('MatMul', ['flat', 'weights'], ['logits']), H.make_node('Identity', ['h'], ['next_h']), H.make_node('Identity', ['c'], ['next_c'])]
        else:
            initializers = [N.from_array(w, 'W'), N.from_array(r, 'R'), N.from_array(b, 'B'), N.from_array(decoder, 'decoder'), N.from_array(np.array([1, hidden], dtype=np.int64), 'shape')]
            nodes = [H.make_node('LSTM', ['x', 'W', 'R', 'B', '', 'h', 'c'], ['Y', 'next_h', 'next_c'], hidden_size=hidden), H.make_node('Reshape', ['next_h', 'shape'], ['flat']), H.make_node('MatMul', ['flat', 'decoder'], ['logits'])]
        graph = H.make_graph(nodes, 'mywubi-character-step-v1-' + kind, inputs, outputs, initializers)
        model = H.make_model(graph, opset_imports=[H.make_opsetid('', 17)], ir_version=8, producer_name='MyWubi interface fixture')
        onnx.checker.check_model(model)
        path = destination / (kind + '.onnx')
        onnx.save(model, path)
        session = ort.InferenceSession(str(path), providers=['CPUExecutionProvider'])
        h = np.zeros((1, 1, hidden), dtype=np.float32)
        c = h.copy()
        cases, max_error = [], 0.
        for token in [1, ids['北'], ids['京'], ids['天']]:
            x = np.zeros((1, 1, v), dtype=np.float32)
            x[0, 0, token] = 1
            if kind == 'counts':
                expected = [weights[token:token + 1], h, c]
            else:
                gates = w[0, :, token] + r[0] @ h.reshape(-1) + b[0, :4*hidden] + b[0, 4*hidden:]
                i, o, f, g = np.split(gates, 4)
                sigmoid = lambda z: 1 / (1 + np.exp(-z))
                new_c = (sigmoid(f) * c.reshape(-1) + sigmoid(i) * np.tanh(g)).reshape(1, 1, hidden)
                new_h = (sigmoid(o) * np.tanh(new_c.reshape(-1))).reshape(1, 1, hidden)
                expected = [new_h.reshape(1, hidden) @ decoder, new_h, new_c]
            actual = session.run(None, dict(x=x, h=h, c=c))
            error = max(float(np.max(np.abs(a-e))) for a, e in zip(actual, expected))
            max_error = max(max_error, error)
            assert error <= 1e-6, (kind, error)
            cases.append(dict(token=token, h=h.reshape(-1).tolist(), c=c.reshape(-1).tolist(), logits=expected[0].reshape(-1).tolist(), next_h=expected[1].reshape(-1).tolist(), next_c=expected[2].reshape(-1).tolist()))
            h, c = actual[1], actual[2]
        audit[kind] = dict(max_absolute_error=max_error, tolerance=1e-6, cases=cases)
    # Intentionally unsupported graph contracts exercise rejection at load/probe time.
    bad_shape = H.make_model(H.make_graph([
        H.make_node('Identity', ['x'], ['logits']), H.make_node('Identity', ['h'], ['next_h']),
        H.make_node('Identity', ['c'], ['next_c'])], 'bad-shape', inputs, [
        H.make_tensor_value_info('logits', T.FLOAT, [1, 1, v]), *outputs[1:]]),
        opset_imports=[H.make_opsetid('', 17)], ir_version=8)
    onnx.save(bad_shape, destination / 'bad-shape.onnx')
    bad_operator = H.make_model(H.make_graph([
        H.make_node('MyWubiUnsupportedOperator', ['x'], ['logits'])], 'bad-operator', inputs, outputs),
        opset_imports=[H.make_opsetid('', 17)], ir_version=8)
    onnx.save(bad_operator, destination / 'bad-operator.onnx')
    (destination / 'numerical-reference.json').write_text(json.dumps(audit, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({kind: entry['max_absolute_error'] for kind, entry in audit.items()}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('destination', type=Path)
    export(parser.parse_args().destination)
