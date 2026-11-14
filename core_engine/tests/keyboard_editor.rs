use core_engine::skin::{self,editor::Draft,keyboard::*};
#[test]
fn edit_one_component_preserves_other_platform_and_unknown_extensions() {
    let mut draft=Draft::builtin("jade").unwrap();skin::keyboard_templates::add(&mut draft.package,"shape14").unwrap();
    let before=draft.package.files["windows/appearance.toml"].clone();
    let c=draft.package.platforms["android"].components.iter().find(|c|c.id=="shape14").unwrap();
    let (mut layout,mapping)=skin::keyboard::layout(&draft.package,c).unwrap();
    let action=layout.keys[0].action.clone();layout.keys[0].label="新标签".into();
    layout.keys[0].extensions.insert("author-note".into(),toml::Value::String("保留".into()));
    draft.update_keyboard("shape14",&layout,mapping.as_ref()).unwrap();
    assert_eq!(draft.package.files["windows/appearance.toml"],before);
    let c=draft.package.platforms["android"].components.iter().find(|c|c.id=="shape14").unwrap();
    let (checked,_)=skin::keyboard::layout(&draft.package,c).unwrap();
    assert_eq!(checked.keys[0].action,action);assert_eq!(checked.keys[0].extensions["author-note"].as_str(),Some("保留"));
    let dir=tempfile::tempdir().unwrap();draft.export_new(&dir.path().join("edited.mywubi-skin")).unwrap();
}
#[test]
fn invalid_overlap_and_mapping_never_replace_previous_component() {
    let mut draft=Draft::builtin("jade").unwrap();skin::keyboard_templates::add(&mut draft.package,"shape9").unwrap();
    let before=draft.package.files.clone();
    let c=draft.package.platforms["android"].components.iter().find(|c|c.id=="shape9").unwrap();
    let (mut layout,mapping)=skin::keyboard::layout(&draft.package,c).unwrap();
    layout.keys[1].x=layout.keys[0].x;
    assert!(draft.update_keyboard("shape9",&layout,mapping.as_ref()).is_err());assert!(draft.package.files==before);
    assert!(skin::combination::validate_layout(&layout,mapping.as_ref()).is_err());
    assert!(layout.validate_geometry(mapping.as_ref(),&[KeyAction::Space,KeyAction::Enter,KeyAction::Cancel]).is_err());
}
