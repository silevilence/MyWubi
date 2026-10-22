use core_engine::{Dictionary, SearchOptions};

fn dictionary(body: &str) -> std::sync::Arc<Dictionary> {
    Dictionary::from_text(&format!("---\ntype: pinyin\nmax_code_len: 64\n---\n{body}")).unwrap()
}

#[test]
fn partial_boundaries_and_contiguous_prefixes_follow_the_contract() {
    let d = dictionary("zhong'guo'ren\t中国人\t1\nni'hao'ma\t你好吗\t1\n");
    for input in [
        "zhongguoren",
        "zhong'guoren",
        "zhongguo'ren",
        "zhong'guo'ren",
        "zhong'",
        "nihao",
        "ni'h",
        "ni'hao'm",
    ] {
        assert_eq!(
            d.search(input, SearchOptions::default()).len(),
            1,
            "{input}"
        );
    }
    for input in [
        "zhon'guoren",
        "zhong'g'uoren",
        "nhm",
        "ni'h'ma",
        "'ni",
        "ni''",
        "ni'hao'ma'",
    ] {
        assert!(
            d.search(input, SearchOptions::default()).is_empty(),
            "{input}"
        );
    }
}

#[test]
fn explicit_boundaries_disambiguate_before_limit_and_deduplication() {
    let d =
        dictionary("xian\t先\t100\nxi'an\t西安\t1\nxi'an\t西安\t2\nni\t你\t100\nni'hao\t你好\t1\n");
    let words = |input, limit| {
        d.search(
            input,
            SearchOptions {
                limit,
                prefer_exact: true,
            },
        )
        .into_iter()
        .map(|e| e.word.as_str())
        .collect::<Vec<_>>()
    };
    assert_eq!(words("xian", 10), ["先", "西安"]);
    assert_eq!(words("xi'an", 1), ["西安"]);
    assert_eq!(words("ni'", 1), ["你好"]);
    assert_eq!(words("xi'an", 0), Vec::<&str>::new());
}

#[test]
fn pinyin_code_validation_counts_letters_only() {
    assert!(
        Dictionary::from_text("---\ntype: pinyin\nmax_code_len: 5\n---\nni'hao\t你好\n").is_ok()
    );
    for code in ["'ni", "ni'", "ni''hao", "Ni", "ni3", "ni hao", "nǐ"] {
        assert!(
            Dictionary::from_text(&format!(
                "---\ntype: pinyin\nmax_code_len: 64\n---\n{code}\t词\n"
            ))
            .is_err(),
            "{code}"
        );
    }
    assert!(
        Dictionary::from_text("---\ntype: pinyin\nmax_code_len: 4\n---\nni'hao\t你好\n").is_err()
    );
}

#[test]
fn exact_results_precede_heavier_prefixes_unless_disabled() {
    let d = dictionary("ni\t你\t1\nni'hao\t你好\t100\n");
    assert_eq!(d.search("ni", SearchOptions::default())[0].word, "你");
    assert_eq!(
        d.search(
            "ni",
            SearchOptions {
                prefer_exact: false,
                limit: 10
            }
        )[0]
        .word,
        "你好"
    );
    assert!(d.has_prefix("ni'h"));
    assert_eq!(
        d.search_trie("ni'h", SearchOptions::default())[0].word,
        "你好"
    );
}
