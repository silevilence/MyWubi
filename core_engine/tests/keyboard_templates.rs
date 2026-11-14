use core_engine::skin::{keyboard::*,keyboard_templates,combination};
#[test]
fn template_covers_alphabet_and_survives_package_roundtrip() {
    let package=keyboard_templates::package().unwrap();
    let component=package.platforms["android"].components.iter().find(|c|c.id=="shape14").unwrap();
    let (layout,mapping)=layout(&package,component).unwrap();
    assert_eq!(layout.keys.iter().filter(|k|matches!(k.action,KeyAction::Group(_))).count(),14);
    layout.validate_geometry(mapping.as_ref(),&[KeyAction::Space,KeyAction::Enter,KeyAction::Cancel]).unwrap();
    assert!(compatibility(&package,&combination::supported()).unwrap()["shape14"].enabled);
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("template.mywubi-skin");
    package.export_new(&path).unwrap();
    let reopened=core_engine::skin::Package::open(&path).unwrap();
    assert_eq!(reopened.metadata.id,package.metadata.id);
    for (path,bytes) in &package.files {if !path.ends_with("metadata.toml") {assert!(reopened.files[path]==*bytes,"changed resource {path}");}}
}
#[test]
fn fourteen_key_real_dictionary_retains_reference_words() {
    use core_engine::{Dictionary,group_input::Position,group_search::{query,WORK_BUDGET,SearchStatus}};
    let package=keyboard_templates::package().unwrap();
    let c=package.platforms["android"].components.iter().find(|c|c.id=="shape14").unwrap();
    let (_,mapping)=layout(&package,c).unwrap();let mapping=mapping.unwrap();
    let dict=Dictionary::load("../tables/wubi86.dict").unwrap();
    for entry in dict.entries().iter().step_by(1300) {
        let positions:Vec<_>=entry.code.chars().map(|c|Position{characters:mapping.groups.values().find(|g|g.contains(c)).unwrap().clone(),selected:None}).collect();
        let result=query(&dict,&positions,WORK_BUDGET);
        assert!(matches!(result.status,SearchStatus::Complete|SearchStatus::BudgetExceeded));
        assert!(result.candidates.iter().any(|m|m.text==entry.word),"lost {}",entry.code);
    }
}

#[test]
fn nine_key_groups_are_semantic_and_missing_mapping_is_rejected() {
    let package=keyboard_templates::package().unwrap();
    let c=package.platforms["android"].components.iter().find(|c|c.id=="shape9").unwrap();
    let (layout,mapping)=layout(&package,c).unwrap();let mut mapping=mapping.unwrap();
    assert_eq!(layout.keys.iter().filter(|k|matches!(k.action,KeyAction::Group(_))).count(),9);
    assert_eq!(mapping.groups.values().map(String::len).sum::<usize>(),26);
    layout.validate_geometry(Some(&mapping),&[KeyAction::Space,KeyAction::Enter,KeyAction::Cancel]).unwrap();
    mapping.groups.remove("g0");
    assert!(layout.validate_geometry(Some(&mapping),&[KeyAction::Space,KeyAction::Enter,KeyAction::Cancel]).is_err());
}
