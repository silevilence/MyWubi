//! Offline CLI; data labels never enter baseline/model scoring inputs.
use core_engine::Dictionary;
use model_input::evaluation::*;
use std::{collections::BTreeMap, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut samples: Vec<Sample> = serde_json::from_str(&std::fs::read_to_string(
        root.join("model_input/evaluation/samples.json"),
    )?)?;
    validate(&samples)?;
    let mut dictionaries = BTreeMap::new();
    for (name, path) in [
        ("wubi", "tables/wubi86.dict"),
        ("pinyin", "tables/pinyin.dict"),
        ("user", "model_input/evaluation/user.dict"),
    ] {
        dictionaries.insert(name, Dictionary::load(root.join(path))?);
    }
    if std::env::args().any(|arg| arg == "--freeze-candidates") {
        for sample in samples.iter_mut().filter(|s| s.task == "rank") {
            let dictionary = dictionaries
                .get(sample.dictionary.as_str())
                .ok_or("unknown dictionary")?;
            sample.input.candidates = dictionary_baseline(dictionary, &sample.input);
        }
        std::fs::write(
            root.join("model_input/evaluation/samples.json"),
            serde_json::to_string_pretty(&samples)? + "\n",
        )?;
    }
    let stats = WordStatistics::train(&samples);
    let mut report = BTreeMap::<String, Metrics>::new();
    for sample in samples.iter().filter(|s| s.split != "train") {
        let ranked = if sample.task == "rank" {
            let dictionary = dictionaries
                .get(sample.dictionary.as_str())
                .ok_or("unknown dictionary")?;
            let ranked = dictionary_baseline(dictionary, &sample.input);
            if ranked != sample.input.candidates {
                return Err(format!("candidate snapshot changed: {}", sample.id).into());
            }
            ranked
        } else {
            stats.recall(&sample.input.prefix, 64)
        };
        for category in ["all", sample.category.as_str()] {
            report
                .entry(format!("{}/{}/{}", sample.split, sample.task, category))
                .or_default()
                .observe(&sample.target, &ranked, &ranked, 5);
        }
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
