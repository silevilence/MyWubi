use core_engine::{
    Dictionary, Entry, InputEvent as E, ReverseLookup, StateMachine, TemporaryTables, Transition,
};
use std::sync::Arc;

#[test]
fn reverse_lookup_is_exact_deduplicated_and_shortest_first() {
    let main = Dictionary::from_text("a\t你\nwg\t你好\nwq\t你好\nwq\t你好\nwqvb\t你好\n").unwrap();
    let index = ReverseLookup::new(&main);
    assert_eq!(index.codes("你好"), ["wg", "wq", "wqvb"]);
    assert!(index.codes("你好呀").is_empty());
    assert!(index.codes("你你").is_empty());
}

#[test]
fn hints_use_only_main_file_and_never_change_commit_text() {
    for enabled in [true, false] {
        let main = Dictionary::from_text(&format!(
            "---\ntemporary:\n  key: '`'\n  reverse_lookup: {enabled}\n---\nwq\t你好\n"
        ))
        .unwrap();
        let mut entries = main.entries().to_vec();
        entries.push(Entry {
            code: "a".into(),
            word: "你好吗".into(),
            weight: 1,
        });
        let merged = main
            .rebuild_with_entries(entries, Default::default())
            .unwrap();
        let auxiliary = Dictionary::from_text(
            "---\ntype: pinyin\nmax_code_len: 20\n---\nni'hao\t你好\t2\nni'hao'ma\t你好吗\t1\n",
        )
        .unwrap();
        let mut sm = StateMachine::new(merged);
        sm.set_temporary_tables(Arc::new(TemporaryTables {
            pinyin: Some(auxiliary),
            reverse: Some(ReverseLookup::new(&main)),
            ..Default::default()
        }));
        sm.handle(E::Char('`'));
        sm.handle(E::Char('1'));
        sm.handle(E::Char('n'));
        let Transition::Candidates {
            code_hints,
            candidates,
            ..
        } = sm.handle(E::Char('i'))
        else {
            panic!("expected candidates")
        };
        assert_eq!(candidates, ["你好", "你好吗"]);
        assert_eq!(code_hints, [if enabled { "主码: wq" } else { "" }, ""]);
        assert_eq!(sm.handle(E::Space), Transition::Commit("你好".into()));
    }
}
