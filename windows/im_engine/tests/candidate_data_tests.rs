// windows/im_engine/tests/candidate_data_tests.rs

use im_engine::candidate_data::{CandidateData, CandidateItem, ThemeSnapshot};

#[test]
fn hidden_has_visible_false() {
    let theme = ThemeSnapshot::default();
    let data = CandidateData::hidden(theme.clone());
    assert!(!data.visible);
    assert!(data.items.is_empty());
    assert_eq!(data.theme.font_size, theme.font_size);
}

#[test]
fn visible_constructor_sets_all_fields() {
    let theme = ThemeSnapshot::default();
    let items = vec![
        CandidateItem { label: "1.".into(), text: "五".into(), hint: String::new() },
        CandidateItem { label: "2.".into(), text: "一".into(), hint: String::new() },
    ];
    let data = CandidateData::visible(
        "gggg".into(), items.clone(), 0, 0, 3, None, theme.clone(),
    );
    assert!(data.visible);
    assert_eq!(data.spelling, "gggg");
    assert_eq!(data.items.len(), 2);
    assert_eq!(data.highlighted, 0);
    assert_eq!(data.page, 0);
    assert_eq!(data.total_pages, 3);
}

#[test]
fn default_is_hidden() {
    let data = CandidateData::default();
    assert!(!data.visible);
}
