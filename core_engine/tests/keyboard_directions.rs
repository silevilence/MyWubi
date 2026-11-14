use core_engine::skin::{directions::*, keyboard::KeyAction};
use std::collections::BTreeMap;

#[test]
fn all_eight_and_merged_groups_round_trip_without_neighbor_fallback() {
    let actions = std::array::from_fn(|i| KeyAction::Character((b'a' + i as u8) as char));
    for kind in [
        Preset::Exact,
        Preset::WideUp,
        Preset::WideDown,
        Preset::WideSides,
    ] {
        let groups = preset(kind, actions.clone());
        let saved = toml::to_string(&groups).unwrap();
        let reopened: BTreeMap<String, KeyAction> = toml::from_str(&saved).unwrap();
        assert_eq!(groups, reopened);
        assert_eq!(resolve(&groups, &BTreeMap::new(), &[]).unwrap().len(), 8);
    }
    let groups = preset(Preset::WideUp, actions);
    let effective = resolve(&groups, &BTreeMap::new(), &["right".into()]).unwrap();
    for direction in [Direction::UpLeft, Direction::Up, Direction::UpRight] {
        assert_eq!(effective[&direction].action, KeyAction::Character('a'));
    }
    assert!(!effective.contains_key(&Direction::Right));
}
#[test]
fn unknown_overlap_and_disabled_override_are_rejected() {
    let groups = [
        ("up+up_right".into(), KeyAction::Space),
        ("up".into(), KeyAction::Enter),
    ]
    .into();
    assert!(resolve(&groups, &BTreeMap::new(), &[]).is_err());
    assert!(resolve(
        &[("north".into(), KeyAction::Space)].into(),
        &BTreeMap::new(),
        &[]
    )
    .is_err());
    let groups = [("up".into(), KeyAction::Space)].into();
    assert!(resolve(&BTreeMap::new(), &groups, &["up".into()]).is_err());
    let replaced = resolve(&groups, &[("up".into(), KeyAction::Enter)].into(), &[]).unwrap();
    assert_eq!(replaced[&Direction::Up].action, KeyAction::Enter);
    assert!(!replaced[&Direction::Up].inherited);
}
#[test]
fn sectors_have_clockwise_boundaries_and_hysteresis() {
    for (index, direction) in ALL.into_iter().enumerate() {
        let angle = (index as f32 * 45.0).to_radians();
        assert_eq!(
            classify(angle.sin() * 50.0, -angle.cos() * 50.0, None),
            Some(direction)
        );
    }
    let angle = 24.0f32.to_radians();
    assert_eq!(
        classify(angle.sin() * 50.0, -angle.cos() * 50.0, None),
        Some(Direction::UpRight)
    );
    assert_eq!(
        classify(angle.sin() * 50.0, -angle.cos() * 50.0, Some(Direction::Up)),
        Some(Direction::Up)
    );
    assert_eq!(classify(1.0, 1.0, Some(Direction::Up)), None);
    assert_eq!(classify(f32::NAN, 40.0, None), None);
}
