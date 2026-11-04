use model_input::evaluation::*;

fn samples() -> Vec<Sample> {
    serde_json::from_str(include_str!("../evaluation/samples.json")).unwrap()
}

#[test]
fn authored_corpus_is_disjoint_and_train_only_statistics_do_not_learn_test_labels() {
    let mut cases = samples();
    validate(&cases).unwrap();
    let before = WordStatistics::train(&cases).recall("北京", 64);
    for case in cases.iter_mut().filter(|s| s.split == "test") {
        case.target = "不能从测试集学到".into();
    }
    assert_eq!(before, WordStatistics::train(&cases).recall("北京", 64));
    cases[1].group = cases[0].group.clone();
    cases[1].split = if cases[0].split == "test" {
        "train"
    } else {
        "test"
    }
    .into();
    assert!(validate(&cases).is_err());
}

#[test]
fn missing_targets_are_recall_failures_and_wrong_reorders_have_a_denominator() {
    let baseline = vec!["甲".into(), "乙".into()];
    let mut m = Metrics::default();
    m.observe("甲", &baseline, &["乙".into(), "甲".into()], 1);
    m.observe("丙", &baseline, &baseline, 1);
    assert_eq!(
        (
            m.samples,
            m.recalled,
            m.baseline_correct,
            m.correct_to_wrong,
            m.page_actions
        ),
        (2, 1, 1, 1, 1)
    );
}

#[test]
fn explicit_pinyin_boundaries_exclude_incompatible_candidates() {
    let dictionary = core_engine::Dictionary::from_text(
        "---\ntype: pinyin\nmax_code_len: 64\n---\nxian\t仙\t10\nxi'an\t西安\t5\n",
    )
    .unwrap();
    let input = Input {
        prefix: "下周去".into(),
        code: "xi'an".into(),
        candidates: vec![],
    };
    assert_eq!(dictionary_baseline(&dictionary, &input), ["西安"]);
}
