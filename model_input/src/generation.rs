//! Versioned generation policy. The GGUF hash also binds the embedded tokenizer.
use core_engine::assistance::{prediction::valid_proposal, Unavailable};
use serde::{Deserialize, Serialize};

pub const ADAPTER: &str = "qwen25-short-words-v1";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationPolicy {
    pub version: u32,
    pub adapter: String,
    pub model_sha256: String,
    pub max_tokens: usize,
    pub max_chars: usize,
    pub max_words: usize,
    /// Mean raw log probability of generated tokens, including the JSON syntax.
    /// This is generation-specific and is not a calibrated probability.
    pub minimum_mean_logprob: f32,
    pub evidence: String,
}
impl GenerationPolicy {
    pub fn validates(&self, hash: &str) -> bool {
        self.version == 1
            && self.adapter == ADAPTER
            && self.model_sha256.eq_ignore_ascii_case(hash)
            && (8..=96).contains(&self.max_tokens)
            && (1..=8).contains(&self.max_chars)
            && (1..=5).contains(&self.max_words)
            && self.minimum_mean_logprob.is_finite()
            && self.minimum_mean_logprob <= 0.0
            && !self.evidence.trim().is_empty()
            && self.evidence.len() <= 8192
    }
    pub fn finish(
        &self,
        prefix: &str,
        bytes: &[u8],
        mean: f32,
        limit: usize,
    ) -> Result<Vec<String>, Unavailable> {
        if !mean.is_finite() {
            return Err(Unavailable::NonFinite);
        }
        if mean < self.minimum_mean_logprob {
            return Err(Unavailable::LowConfidence);
        }
        // Parsing a complete array prevents budget exhaustion from publishing fragments.
        let words: Vec<String> =
            serde_json::from_slice(bytes).map_err(|_| Unavailable::Proposals)?;
        if words.len() > self.max_words {
            return Err(Unavailable::Proposals);
        }
        let mut result = Vec::new();
        for word in words {
            if valid_proposal(prefix, &word)
                && word.chars().count() <= self.max_chars
                && !result.contains(&word)
            {
                result.push(word);
            }
        }
        result.truncate(limit);
        if result.is_empty() {
            Err(Unavailable::Empty)
        } else {
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> GenerationPolicy {
        GenerationPolicy {
            version: 1,
            adapter: ADAPTER.into(),
            model_sha256: "a".repeat(64),
            max_tokens: 64,
            max_chars: 6,
            max_words: 3,
            minimum_mean_logprob: -3.,
            evidence: "fixture only".into(),
        }
    }
    #[test]
    fn complete_utf8_json_only_and_no_whitelist() {
        let p = policy();
        assert_eq!(
            p.finish(
                "研究量子",
                "[\"纠缠态\",\"计算\",\"计算\"]".as_bytes(),
                -1.,
                3
            )
            .unwrap(),
            ["纠缠态", "计算"]
        );
        for bytes in [
            b"[\"".as_slice(),
            b"[\"\xff\"]",
            "解释：[\"计算\"]".as_bytes(),
        ] {
            assert!(p.finish("量子", bytes, -1., 3).is_err());
        }
        assert_eq!(p.finish("量子", b"[]", -1., 3), Err(Unavailable::Empty));
        assert_eq!(
            p.finish("量子", "[\"量子\"]".as_bytes(), -1., 3),
            Err(Unavailable::Empty)
        );
        assert_eq!(
            p.finish("量子", "[\"计算\"]".as_bytes(), -4., 3),
            Err(Unavailable::LowConfidence)
        );
    }
    #[test]
    fn policy_binds_model_tokenizer_adapter_and_budgets() {
        let mut p = policy();
        assert!(p.validates(&"a".repeat(64)));
        assert!(!p.validates(&"b".repeat(64)));
        p.max_tokens = 1000;
        assert!(!p.validates(&"a".repeat(64)));
    }
}
