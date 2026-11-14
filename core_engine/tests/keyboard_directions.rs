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

#[test]
fn shared_android_gesture_traces_and_editing_keep_semantics() {
    use core_engine::skin::directions::*;
    let cases:serde_json::Value=serde_json::from_str(include_str!("fixtures/protocol/gestures.json")).unwrap();
    for case in cases.as_array().unwrap(){let bound=case["bound"].as_array().map(|a|a.iter().map(|i|ALL[i.as_u64().unwrap() as usize]).collect()).unwrap_or_else(||ALL.into());
        let mut gesture=Gesture::new(bound,true);if case["hold"]==true {gesture.hold();}
        for point in case["points"].as_array().unwrap(){gesture.move_to(point[0].as_f64().unwrap() as f32,point[1].as_f64().unwrap() as f32);}
        assert_eq!(gesture.release().map(i64::from),case["expected"].as_i64(),"{}",case["name"]);assert_eq!(gesture.release(),None);
    }
    let mut groups=preset(Preset::WideUp,std::array::from_fn(|_|core_engine::skin::keyboard::KeyAction::Character('a')));
    replace(&mut groups,&[Direction::Up].into(),Some(core_engine::skin::keyboard::KeyAction::Character('b'))).unwrap();
    let resolved=resolve(&Default::default(),&groups,&[]).unwrap();
    assert_ne!(resolved[&Direction::Up].action,resolved[&Direction::UpLeft].action);
}
