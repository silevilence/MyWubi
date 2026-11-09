//! Deterministic, offline search over expression metadata, never over output bytes.
//! Relationship provenance and limits: expressions/SEARCH-RELATIONS.md.
use crate::expressions::ExpressionEntry;
use std::collections::{BTreeMap, BTreeSet};

// First term is the internal search concept. These curated associations are not
// general linguistic equivalences (for example riding is associated with bicycles).
const RELATIONS: &[&[&str]] = &[
    &["禁止", "不准", "不许", "严禁", "禁行", "不允许"],
    &["自行车", "单车", "脚踏车", "骑车", "骑行"],
    &[
        "开心",
        "高兴",
        "快乐",
        "喜悦",
        "喜极而泣",
        "笑颜逐开",
        "微笑",
        "大笑",
        "哈哈",
        "呵呵",
        "笑",
    ],
    &[
        "难过",
        "悲伤",
        "伤心",
        "不开心",
        "不高兴",
        "不快乐",
        "哭泣",
        "哭",
        "悲痛",
    ],
    &["生气", "愤怒", "恼火"],
    &["惊讶", "吃惊", "震惊"],
    &["猫", "猫咪", "猫脸", "喵"],
    &["狗", "狗狗", "犬"],
    &["车", "车辆", "交通工具"],
];
const RESTRICTIONS: &[&str] = &["禁止", "不是", "没有", "不要", "勿", "不", "无", "非"];

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum MatchQuality {
    Exact,
    AllTerms,
    Loose,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionMatch {
    pub index: usize,
    pub quality: MatchQuality,
    pub matched_terms: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResults {
    pub terms: Vec<String>,
    /// Validated original word boundaries, retained for UI explanations. They do
    /// not force token breaks: 禁止+自+行车 and 禁止自行车 have the same semantics.
    pub words: Vec<String>,
    pub matches: Vec<ExpressionMatch>,
}

#[derive(Debug)]
struct IndexedEntry {
    names: Vec<usize>,
    fields: Vec<usize>,
    loose_names: Vec<String>,
}

/// Shared fields are interned so common categories are searched only once per
/// explicit lookup, even when thousands of kaomoji carry the same tags.
#[derive(Debug)]
pub(crate) struct SearchIndex {
    fields: Vec<String>,
    rows: Vec<IndexedEntry>,
    lexicon: BTreeMap<char, Vec<String>>,
}

fn normalized(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '＋' && *c != '+')
        .flat_map(char::to_lowercase)
        .collect()
}

fn concepts(text: &str) -> String {
    let mut remaining = text;
    let mut output = String::new();
    while !remaining.is_empty() {
        let relation = RELATIONS
            .iter()
            .flat_map(|group| group.iter().map(move |alias| (*alias, group[0])))
            .filter(|(alias, _)| remaining.starts_with(alias))
            .max_by_key(|(alias, _)| alias.len());
        if let Some((alias, concept)) = relation {
            output.push_str(concept);
            remaining = &remaining[alias.len()..];
        } else if let Some(c) = remaining.chars().next() {
            output.push(c);
            remaining = &remaining[c.len_utf8()..];
        }
    }
    output
}

fn subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|wanted| chars.any(|c| c == wanted))
}

fn field_matches(field: &str, term: &str) -> bool {
    field.match_indices(term).any(|(offset, _)| {
        !["不", "不是", "没有", "不要", "无", "非"]
            .iter()
            .any(|prefix| field[..offset].ends_with(prefix))
    })
}

impl SearchIndex {
    pub(crate) fn new(entries: &[ExpressionEntry]) -> Self {
        let mut field_ids = BTreeMap::<String, usize>::new();
        let mut fields = Vec::new();
        let mut vocabulary = BTreeSet::new();
        let mut intern = |text: String| {
            let next = fields.len();
            *field_ids.entry(text.clone()).or_insert_with(|| {
                fields.push(text);
                next
            })
        };
        let rows = entries
            .iter()
            .map(|entry| {
                let names = std::iter::once(&entry.name)
                    .chain(&entry.aliases)
                    .map(|name| intern(normalized(name)))
                    .collect();
                let mut row_fields = BTreeSet::new();
                for raw in std::iter::once(&entry.name)
                    .chain(&entry.aliases)
                    .chain(&entry.triggers)
                    .chain(&entry.tags)
                {
                    let text = normalized(raw);
                    let semantic = concepts(&text);
                    if !semantic.is_empty() {
                        vocabulary.insert(semantic.clone());
                    }
                    row_fields.insert(intern(semantic));
                }
                IndexedEntry {
                    names,
                    fields: row_fields.into_iter().collect(),
                    loose_names: std::iter::once(&entry.name)
                        .chain(&entry.aliases)
                        .map(|s| concepts(&normalized(s)))
                        .collect(),
                }
            })
            .collect();
        vocabulary.extend(RELATIONS.iter().map(|group| group[0].to_owned()));
        vocabulary.extend(RESTRICTIONS.iter().map(|s| (*s).to_owned()));
        let mut lexicon: BTreeMap<char, Vec<String>> = BTreeMap::new();
        for word in vocabulary {
            if let Some(first) = word.chars().next() {
                lexicon.entry(first).or_default().push(word);
            }
        }
        for words in lexicon.values_mut() {
            words.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        }
        Self {
            fields,
            rows,
            lexicon,
        }
    }

    fn prefix<'a>(&'a self, text: &str) -> Option<&'a str> {
        // Curated concepts stay whole, particularly negative emotion phrases.
        if let Some(term) = RELATIONS
            .iter()
            .map(|g| g[0])
            .chain(RESTRICTIONS.iter().copied())
            .filter(|term| text.starts_with(term))
            .max_by_key(|term| term.len())
        {
            return Some(term);
        }
        let words = self.lexicon.get(&text.chars().next()?)?;
        let first = words.iter().find(|word| text.starts_with(word.as_str()))?;
        if first.len() == text.len() {
            // A complete name must not suppress equivalent matches whose conditions
            // occur in separate tags. Prefer a fully known decomposition if one exists.
            for part in words
                .iter()
                .filter(|word| word.len() < text.len() && text.starts_with(word.as_str()))
            {
                let mut rest = &text[part.len()..];
                while let Some(next) = rest
                    .chars()
                    .next()
                    .and_then(|c| self.lexicon.get(&c))
                    .and_then(|terms| terms.iter().find(|term| rest.starts_with(term.as_str())))
                {
                    rest = &rest[next.len()..];
                    if rest.is_empty() {
                        return Some(part);
                    }
                }
            }
        }
        Some(first)
    }

    fn terms(&self, query: &str) -> Vec<(String, bool)> {
        let mut result = Vec::new();
        let mut remaining = query;
        while !remaining.is_empty() {
            if let Some(word) = self.prefix(remaining) {
                result.push((word.to_owned(), true));
                remaining = &remaining[word.len()..];
            } else {
                // Preserve an unknown run as one condition. Never drop unknown text
                // or expand it into arbitrary single-character matches.
                let end = remaining
                    .char_indices()
                    .skip(1)
                    .find(|(i, _)| self.prefix(&remaining[*i..]).is_some())
                    .map_or(remaining.len(), |(i, _)| i);
                result.push((remaining[..end].to_owned(), false));
                remaining = &remaining[end..];
            }
        }
        let mut seen = BTreeSet::new();
        result.retain(|(s, _)| seen.insert(s.clone()));
        result
    }

    pub(crate) fn search(&self, query: &str, words: &[String]) -> SearchResults {
        let original = normalized(query);
        let query = concepts(&original);
        let words = if normalized(&words.concat()) == original {
            words.to_vec()
        } else {
            vec![query.clone()]
        };
        if query.is_empty() || query.chars().count() > 512 {
            return SearchResults {
                terms: Vec::new(),
                words,
                matches: Vec::new(),
            };
        }
        let tokens = self.terms(&query);
        // Compute each field's coverage once, then union it per entry.
        let coverage: Vec<Vec<bool>> = self
            .fields
            .iter()
            .map(|field| {
                tokens
                    .iter()
                    .map(|(term, _)| field_matches(field, term))
                    .collect()
            })
            .collect();
        let contains: Vec<bool> = self
            .fields
            .iter()
            .map(|field| field_matches(field, &query))
            .collect();
        let mut complete = Vec::new();
        let mut partial = Vec::new();
        for (index, entry) in self.rows.iter().enumerate() {
            let matched_terms = (0..tokens.len())
                .filter(|&term| entry.fields.iter().any(|&field| coverage[field][term]))
                .count();
            let exact = entry
                .names
                .iter()
                .any(|&field| self.fields[field] == original);
            let quality = if exact {
                MatchQuality::Exact
            } else if entry.fields.iter().any(|&field| contains[field])
                || (matched_terms == tokens.len() && tokens.iter().all(|(_, known)| *known))
            {
                MatchQuality::AllTerms
            } else if tokens
                .iter()
                .enumerate()
                .filter(|(_, (term, _))| RESTRICTIONS.iter().any(|r| term.starts_with(r)))
                .all(|(term, _)| entry.fields.iter().any(|&field| coverage[field][term]))
                && entry
                    .loose_names
                    .iter()
                    .any(|name| subsequence(&query, name))
            {
                MatchQuality::Loose
            } else {
                MatchQuality::Partial
            };
            let row = ExpressionMatch {
                index,
                quality,
                matched_terms,
            };
            if quality != MatchQuality::Partial {
                complete.push(row);
            } else if matched_terms > 0 {
                partial.push(row);
            }
        }
        let matches = if complete.is_empty() {
            partial.sort_by(|a, b| {
                b.matched_terms
                    .cmp(&a.matched_terms)
                    .then(a.index.cmp(&b.index))
            });
            partial
        } else {
            complete.sort_by_key(|row| (row.quality, row.index));
            complete
        };
        SearchResults {
            terms: tokens.into_iter().map(|(s, _)| s).collect(),
            words,
            matches,
        }
    }
}
