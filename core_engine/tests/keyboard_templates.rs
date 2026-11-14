use core_engine::skin::{combination, keyboard::*, keyboard_templates};
#[test]
fn template_covers_alphabet_and_survives_package_roundtrip() {
    let package = keyboard_templates::package().unwrap();
    let component = package.platforms["android"]
        .components
        .iter()
        .find(|c| c.id == "shape14")
        .unwrap();
    let (layout, mapping) = layout(&package, component).unwrap();
    assert_eq!(
        layout
            .keys
            .iter()
            .filter(|k| matches!(k.action, KeyAction::Group(_)))
            .count(),
        14
    );
    layout
        .validate_geometry(
            mapping.as_ref(),
            &[KeyAction::Space, KeyAction::Enter, KeyAction::Cancel],
        )
        .unwrap();
    assert!(compatibility(&package, &combination::supported()).unwrap()["shape14"].enabled);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("template.mywubi-skin");
    package.export_new(&path).unwrap();
    let reopened = core_engine::skin::Package::open(&path).unwrap();
    assert_eq!(reopened.metadata.id, package.metadata.id);
    for (path, bytes) in &package.files {
        if !path.ends_with("metadata.toml") {
            assert!(reopened.files[path] == *bytes, "changed resource {path}");
        }
    }
}
#[test]
fn fourteen_key_real_dictionary_retains_reference_words() {
    use core_engine::{
        group_input::Position,
        group_search::{query, SearchStatus, WORK_BUDGET},
        Dictionary,
    };
    let package = keyboard_templates::package().unwrap();
    let c = package.platforms["android"]
        .components
        .iter()
        .find(|c| c.id == "shape14")
        .unwrap();
    let (_, mapping) = layout(&package, c).unwrap();
    let mapping = mapping.unwrap();
    let dict = Dictionary::load("../tables/wubi86.dict").unwrap();
    for entry in dict.entries().iter().step_by(1300) {
        let positions: Vec<_> = entry
            .code
            .chars()
            .map(|c| Position {
                characters: mapping
                    .groups
                    .values()
                    .find(|g| g.contains(c))
                    .unwrap()
                    .clone(),
                selected: None,
            })
            .collect();
        let result = query(&dict, &positions, WORK_BUDGET);
        assert!(matches!(
            result.status,
            SearchStatus::Complete | SearchStatus::BudgetExceeded
        ));
        assert!(
            result.candidates.iter().any(|m| m.text == entry.word),
            "lost {}",
            entry.code
        );
    }
}

#[test]
fn nine_key_groups_are_semantic_and_missing_mapping_is_rejected() {
    let package = keyboard_templates::package().unwrap();
    let c = package.platforms["android"]
        .components
        .iter()
        .find(|c| c.id == "shape9")
        .unwrap();
    let (layout, mapping) = layout(&package, c).unwrap();
    let mut mapping = mapping.unwrap();
    assert_eq!(
        layout
            .keys
            .iter()
            .filter(|k| matches!(k.action, KeyAction::Group(_)))
            .count(),
        9
    );
    assert_eq!(mapping.groups.values().map(String::len).sum::<usize>(), 26);
    layout
        .validate_geometry(
            Some(&mapping),
            &[KeyAction::Space, KeyAction::Enter, KeyAction::Cancel],
        )
        .unwrap();
    mapping.groups.remove("g0");
    assert!(layout
        .validate_geometry(
            Some(&mapping),
            &[KeyAction::Space, KeyAction::Enter, KeyAction::Cancel]
        )
        .is_err());
}

#[test]
fn pinyin_template_is_bound_to_pinyin_and_covers_separator() {
    let package=keyboard_templates::package().unwrap();
    let c=package.platforms["android"].components.iter().find(|c|c.id=="pinyin9").unwrap();
    let (layout,mapping)=layout(&package,c).unwrap();
    assert_eq!(layout.modes,vec![Mode::Pinyin]);
    assert!(layout.charset.contains('\''));
    layout.validate_geometry(mapping.as_ref(),&[KeyAction::Space,KeyAction::Enter,KeyAction::Cancel]).unwrap();
}

#[test]
fn full_keyboard_is_exact_and_templates_reimport_without_resource_changes() {
    let package=keyboard_templates::package().unwrap();
    let c=package.platforms["android"].components.iter().find(|c|c.id=="full26").unwrap();
    let (layout,mapping)=layout(&package,c).unwrap();assert!(mapping.is_none());
    assert_eq!(layout.keys.iter().filter(|k|matches!(k.action,KeyAction::Character(_))).count(),26);
    let bytes=package.encoded().unwrap();let first=core_engine::skin::Package::read(std::io::Cursor::new(bytes)).unwrap();
    let second=core_engine::skin::Package::read(std::io::Cursor::new(first.encoded().unwrap())).unwrap();
    assert!(first.files==second.files);
    for id in ["full26","shape14","shape9","pinyin9"] {assert!(compatibility(&second,&combination::supported()).unwrap()[id].enabled);}
}
