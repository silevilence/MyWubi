//! 状态机按键处理性能基准测试。
//!
//! 用以确保单次 `handle()` 调用延迟控制在微秒级。
//! 运行方式：`cargo bench -p core_engine`。

use core_engine::dictionary::{Entry, LoadOptions};
use core_engine::Dictionary;
use core_engine::{InputEvent, StateMachine, Transition};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;

fn build_dict() -> Arc<Dictionary> {
    let entries = vec![
        Entry {
            code: "a".into(),
            word: "工".into(),
            weight: 999,
        },
        Entry {
            code: "ggll".into(),
            word: "王".into(),
            weight: 100,
        },
        Entry {
            code: "ggll".into(),
            word: "壬".into(),
            weight: 50,
        },
        Entry {
            code: "ggll".into(),
            word: "丰".into(),
            weight: 20,
        },
        Entry {
            code: "ggh".into(),
            word: "理".into(),
            weight: 80,
        },
        Entry {
            code: "gghg".into(),
            word: "五".into(),
            weight: 200,
        },
        Entry {
            code: "ggtt".into(),
            word: "五笔".into(),
            weight: 60,
        },
        Entry {
            code: "ggli".into(),
            word: "班".into(),
            weight: 30,
        },
        Entry {
            code: "gt".into(),
            word: "五".into(),
            weight: 10,
        },
    ];
    Dictionary::from_entries(entries, None, LoadOptions::default()).unwrap()
}

fn bench_handle_char(c: &mut Criterion) {
    let dict = build_dict();
    c.bench_function("handle_char_first", |b| {
        b.iter_with_setup(
            || StateMachine::new(Arc::clone(&dict)),
            |mut m| {
                let t = m.handle(InputEvent::Char('a'));
                black_box(t);
                m
            },
        );
    });
}

fn bench_handle_full_sequence(c: &mut Criterion) {
    let dict = build_dict();
    let seq: Vec<InputEvent> = "ggll".chars().map(InputEvent::Char).collect();
    c.bench_function("handle_four_chars", |b| {
        b.iter_with_setup(
            || StateMachine::new(Arc::clone(&dict)),
            |mut m| {
                for ev in black_box(&seq) {
                    let t = m.handle(black_box(ev.clone()));
                    if let Transition::Commit(_) = t {
                        break;
                    }
                }
                m
            },
        );
    });
}

fn bench_handle_space_commit(c: &mut Criterion) {
    let dict = build_dict();
    c.bench_function("handle_space_commit", |b| {
        b.iter_with_setup(
            || {
                let mut m = StateMachine::new(Arc::clone(&dict));
                m.handle(InputEvent::Char('a'));
                m
            },
            |mut m| {
                let t = m.handle(InputEvent::Space);
                black_box(t);
                m
            },
        );
    });
}

fn bench_handle_backspace(c: &mut Criterion) {
    let dict = build_dict();
    c.bench_function("handle_backspace", |b| {
        b.iter_with_setup(
            || {
                let mut m = StateMachine::new(Arc::clone(&dict));
                m.handle(InputEvent::Char('a'));
                m
            },
            |mut m| {
                let t = m.handle(InputEvent::Backspace);
                black_box(t);
                m
            },
        );
    });
}

criterion_group!(
    name = state_machine_benches;
    config = Criterion::default().sample_size(100);
    targets = bench_handle_char, bench_handle_full_sequence, bench_handle_space_commit, bench_handle_backspace
);
criterion_main!(state_machine_benches);
