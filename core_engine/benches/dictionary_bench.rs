//! 码表检索性能基准测试。
//!
//! 用以确保单次检索（exact / 前缀 / Trie）延迟控制在微秒级。
//! 运行方式：`cargo bench -p core_engine`。

use core_engine::dictionary::{Entry, LoadOptions, SearchOptions};
use core_engine::Dictionary;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;

/// 生成一份约 1 万条、形如 `aaaa`..`zzzz` 的伪五笔码表，
/// 用于在基准中模拟真实量级的码表。
fn build_dict(n_prefix: usize, per_prefix: usize) -> Arc<Dictionary> {
    let letters: Vec<char> = ('a'..='z').collect();
    let mut entries = Vec::with_capacity(n_prefix * per_prefix);
    let mut idx = 0u32;
    for i in 0..n_prefix {
        let c1 = letters[i % letters.len()];
        let c2 = letters[(i / letters.len()) % letters.len()];
        for k in 0..per_prefix {
            entries.push(Entry {
                code: format!("{c1}{c2}{c1}{c2}"),
                word: format!("word{idx}"),
                weight: (per_prefix - k) as u32,
            });
            idx += 1;
        }
    }
    Dictionary::from_entries(entries, None, LoadOptions::default()).unwrap()
}

fn bench_exact(c: &mut Criterion) {
    let d = build_dict(2000, 5);
    let sample_code = "aaa";
    c.bench_function("exact_lookup", |b| {
        b.iter(|| {
            let r = black_box(&d).exact(black_box(sample_code));
            black_box(r);
        });
    });
}

fn bench_prefix_search(c: &mut Criterion) {
    let d = build_dict(2000, 5);
    let opts = SearchOptions {
        prefer_exact: true,
        limit: 10,
    };
    c.bench_function("prefix_search_limit_10", |b| {
        b.iter(|| {
            let r = black_box(&d).search(black_box("aa"), black_box(opts));
            black_box(r);
        });
    });
}

fn bench_trie_search(c: &mut Criterion) {
    let d = build_dict(2000, 5);
    let opts = SearchOptions {
        prefer_exact: true,
        limit: 10,
    };
    c.bench_function("trie_search_limit_10", |b| {
        b.iter(|| {
            let r = black_box(&d).search_trie(black_box("aa"), black_box(opts));
            black_box(r);
        });
    });
}

fn bench_has_prefix(c: &mut Criterion) {
    let d = build_dict(2000, 5);
    c.bench_function("has_prefix", |b| {
        b.iter(|| {
            let r = black_box(&d).has_prefix(black_box("ab"));
            black_box(r);
        });
    });
}

fn bench_unique_exact(c: &mut Criterion) {
    let d = build_dict(2000, 5);
    c.bench_function("unique_exact", |b| {
        b.iter(|| {
            let r = black_box(&d).unique_exact(black_box("aaaaaa"));
            black_box(r);
        });
    });
}

criterion_group!(
    name = dictionary_benches;
    config = Criterion::default().sample_size(100);
    targets = bench_exact, bench_prefix_search, bench_trie_search, bench_has_prefix, bench_unique_exact
);
criterion_main!(dictionary_benches);
