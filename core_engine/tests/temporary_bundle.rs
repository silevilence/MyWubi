use core_engine::{
    config::Hotkey, Dictionary, InputEvent as E, InputMode, StateMachine, TemporaryTables,
    Transition,
};
use std::{path::Path, sync::Arc};

#[test]
fn distributed_example_bundle_runs_both_modes_and_reverse_lookup() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/temporary/main.dict");
    let main = Dictionary::load(&path).unwrap();
    let tables = Arc::new(TemporaryTables::load(&main, &path, &Hotkey::default()));
    assert!(tables.load_errors.is_empty());
    let mut sm = StateMachine::new(main);
    sm.set_temporary_tables(tables);
    for (input, key, expected) in [
        ("`1zhong'guoren", E::Space, "中国人"),
        ("`1zhongguo'ren", E::Space, "中国人"),
        ("`1xi'an", E::Space, "西安"),
        ("`1nihao", E::Enter, "nihao"),
        ("`2brb", E::Char('2'), "be right back soon"),
        ("`2Hello!", E::Space, "Hello!"),
    ] {
        for c in input.chars() {
            sm.handle(E::Char(c));
        }
        assert_eq!(
            sm.handle(key),
            Transition::Commit(expected.into()),
            "{input}"
        );
        assert_eq!(sm.mode(), InputMode::Main);
    }
}
