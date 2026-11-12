//! Platform-independent expression resources. They never participate in code-table parsing.
//!
//! Files start with a magic line followed by JSON. Output strings are preserved byte for
//! byte; only complete trigger strings are indexed. Loading happens outside input dispatch.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Separate file signature prevents accidental use as a main/temporary code table.
pub const MAGIC: &str = "# MyWubi Expressions 1";
const MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpressionKind {
    Emoji,
    Kaomoji,
    Kana,
    Math,
    Punctuation,
    Symbols,
}

impl ExpressionKind {
    /// Stable category order, shared by settings and candidate append.
    pub const ALL: [Self; 6] = [
        Self::Emoji,
        Self::Kaomoji,
        Self::Kana,
        Self::Math,
        Self::Punctuation,
        Self::Symbols,
    ];

    /// Stable resource and protocol name.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Emoji => "emoji",
            Self::Kaomoji => "kaomoji",
            Self::Kana => "kana",
            Self::Math => "math",
            Self::Punctuation => "punctuation",
            Self::Symbols => "symbols",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Emoji => "Emoji",
            Self::Kaomoji => "颜文字",
            Self::Kana => "假名",
            Self::Math => "数学符号",
            Self::Punctuation => "标点",
            Self::Symbols => "其他符号",
        }
    }
    pub fn default_path(self) -> PathBuf {
        PathBuf::from(match self {
            Self::Emoji => "expressions/emoji.mwe",
            Self::Kaomoji => "expressions/kaomoji.mwe",
            Self::Kana => "expressions/kana.mwe",
            Self::Math => "expressions/math.mwe",
            Self::Punctuation => "expressions/punctuation.mwe",
            Self::Symbols => "expressions/symbols.mwe",
        })
    }
}

/// Auto-append and dedicated search independently request the same resource.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExpressionSelection {
    pub append: bool,
    pub search: bool,
    /// None selects the bundled resource, relative to the shared config directory.
    pub resource: Option<PathBuf>,
}
impl ExpressionSelection {
    pub fn needed(&self) -> bool {
        self.append || self.search
    }
    pub fn path(&self, config_path: &Path, kind: ExpressionKind) -> PathBuf {
        crate::config_path::resolve_resource_path(
            config_path,
            self.resource.as_deref().unwrap_or(&kind.default_path()),
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExpressionsConfig {
    pub emoji: ExpressionSelection,
    pub kaomoji: ExpressionSelection,
    pub kana: ExpressionSelection,
    pub math: ExpressionSelection,
    pub punctuation: ExpressionSelection,
    pub symbols: ExpressionSelection,
}

impl ExpressionsConfig {
    pub fn selection(&self, kind: ExpressionKind) -> &ExpressionSelection {
        match kind {
            ExpressionKind::Emoji => &self.emoji,
            ExpressionKind::Kaomoji => &self.kaomoji,
            ExpressionKind::Kana => &self.kana,
            ExpressionKind::Math => &self.math,
            ExpressionKind::Punctuation => &self.punctuation,
            ExpressionKind::Symbols => &self.symbols,
        }
    }
    pub fn selection_mut(&mut self, kind: ExpressionKind) -> &mut ExpressionSelection {
        match kind {
            ExpressionKind::Emoji => &mut self.emoji,
            ExpressionKind::Kaomoji => &mut self.kaomoji,
            ExpressionKind::Kana => &mut self.kana,
            ExpressionKind::Math => &mut self.math,
            ExpressionKind::Punctuation => &mut self.punctuation,
            ExpressionKind::Symbols => &mut self.symbols,
        }
    }
}

/// Source identifiers resolve to notices shipped with the resource; revisions are pinned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionSource {
    pub id: String,
    pub url: String,
    pub revision: String,
    pub license: String,
    pub notice: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionEntry {
    pub text: String,
    pub name: String,
    pub triggers: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Exact duplicate outputs are rejected; variants remain independent entries.
    pub order: u32,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionDocument {
    pub kind: ExpressionKind,
    pub name: String,
    pub version: String,
    pub sources: Vec<ExpressionSource>,
    pub entries: Vec<ExpressionEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExpressionError {
    #[error("无法读取表情词库 {0}: {1}")]
    Io(PathBuf, String),
    #[error("表情词库格式错误: {0}")]
    Invalid(String),
    #[error("表情词库 JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
}

/// Validated immutable document and exact trigger index, shared by both frontends.
#[derive(Debug)]
pub struct ExpressionLibrary {
    document: ExpressionDocument,
    exact: BTreeMap<String, Vec<usize>>,
    search: crate::expression_search::SearchIndex,
}

impl ExpressionLibrary {
    pub fn parse(text: &str, expected: ExpressionKind) -> Result<Self, ExpressionError> {
        let (magic, json) = text
            .trim_start_matches('\u{feff}')
            .split_once('\n')
            .ok_or_else(|| ExpressionError::Invalid("缺少专用词库文件标识".into()))?;
        if magic.trim_end_matches('\r') != MAGIC {
            return Err(ExpressionError::Invalid(
                "不是 MyWubi 表情词库，不能使用普通码表".into(),
            ));
        }
        if text.len() as u64 > MAX_BYTES {
            return Err(ExpressionError::Invalid("词库超过 64 MiB".into()));
        }
        let document: ExpressionDocument = serde_json::from_str(json)?;
        Self::new(document, expected)
    }

    pub fn new(
        mut document: ExpressionDocument,
        expected: ExpressionKind,
    ) -> Result<Self, ExpressionError> {
        let invalid = |s: &str| ExpressionError::Invalid(s.into());
        if document.kind != expected {
            return Err(invalid("词库类型不符"));
        }
        if document.name.trim().is_empty() || document.version.trim().is_empty() {
            return Err(invalid("缺少名称或版本"));
        }
        if document.entries.is_empty() || document.entries.len() > 100_000 {
            return Err(invalid("词条数量必须在 1～100000 之间"));
        }
        let mut sources = HashSet::new();
        for source in &document.sources {
            if [
                &source.id,
                &source.url,
                &source.revision,
                &source.license,
                &source.notice,
            ]
            .into_iter()
            .any(|s| s.trim().is_empty())
                || !sources.insert(source.id.as_str())
            {
                return Err(invalid(
                    "来源缺少标识、地址、版本、许可或声明，或来源标识重复",
                ));
            }
        }
        let mut outputs = HashSet::new();
        for entry in &document.entries {
            if !valid_single_line(&entry.text)
                || entry.text.trim().is_empty()
                || !outputs.insert(entry.text.as_str())
            {
                return Err(invalid("输出为空、重复、多行、含控制字符或超过 512 字符"));
            }
            if !valid_single_line(&entry.name)
                || entry.name.trim().is_empty()
                || entry.triggers.is_empty()
                || entry
                    .triggers
                    .iter()
                    .chain(&entry.tags)
                    .chain(&entry.aliases)
                    .any(|s| !valid_single_line(s) || s.trim().is_empty())
            {
                return Err(invalid("名称或触发词/标签/别名非法"));
            }
            if entry.sources.is_empty()
                || entry
                    .sources
                    .iter()
                    .any(|id| !sources.contains(id.as_str()))
            {
                return Err(invalid("词条引用了缺失的来源"));
            }
        }
        document.entries.sort_by_key(|entry| entry.order);
        let mut exact: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, entry) in document.entries.iter().enumerate() {
            for trigger in &entry.triggers {
                let indices = exact.entry(trigger.clone()).or_default();
                if indices.last() != Some(&index) {
                    indices.push(index);
                }
            }
        }
        let search = crate::expression_search::SearchIndex::new(&document.entries);
        Ok(Self {
            document,
            exact,
            search,
        })
    }

    pub fn load(path: &Path, expected: ExpressionKind) -> Result<Self, ExpressionError> {
        let mut text = String::new();
        std::fs::File::open(path)
            .and_then(|file| file.take(MAX_BYTES + 1).read_to_string(&mut text))
            .map_err(|error| ExpressionError::Io(path.to_owned(), error.to_string()))?;
        Self::parse(&text, expected)
    }

    pub fn document(&self) -> &ExpressionDocument {
        &self.document
    }
    /// Dedicated-mode lookup only; automatic append keeps the strict trigger index.
    pub fn search(&self, query: &str, words: &[String]) -> crate::expression_search::SearchResults {
        self.search.search(query, words)
    }
    pub fn exact(&self, word: &str) -> impl Iterator<Item = &ExpressionEntry> {
        self.exact
            .get(word)
            .into_iter()
            .flatten()
            .map(|&index| &self.document.entries[index])
    }
    pub(crate) fn exact_indices(&self, word: &str) -> &[usize] {
        self.exact.get(word).map_or(&[], Vec::as_slice)
    }
    /// Export without normalizing spaces, variation selectors, or combining characters.
    pub fn encode(&self) -> Result<String, ExpressionError> {
        Ok(format!(
            "{MAGIC}\n{}\n",
            serde_json::to_string(&self.document)?
        ))
    }
}

fn valid_single_line(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= 512
        && !text
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
}

#[derive(Debug, Default)]
pub struct ExpressionResource {
    pub library: Option<Arc<ExpressionLibrary>>,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct ExpressionResources {
    pub emoji: ExpressionResource,
    pub kaomoji: ExpressionResource,
    pub kana: ExpressionResource,
    pub math: ExpressionResource,
    pub punctuation: ExpressionResource,
    pub symbols: ExpressionResource,
}
impl ExpressionResources {
    pub fn resource(&self, kind: ExpressionKind) -> &ExpressionResource {
        match kind {
            ExpressionKind::Emoji => &self.emoji,
            ExpressionKind::Kaomoji => &self.kaomoji,
            ExpressionKind::Kana => &self.kana,
            ExpressionKind::Math => &self.math,
            ExpressionKind::Punctuation => &self.punctuation,
            ExpressionKind::Symbols => &self.symbols,
        }
    }

    pub fn load(config_path: &Path, config: &ExpressionsConfig) -> Self {
        fn one(
            path: &Path,
            selection: &ExpressionSelection,
            kind: ExpressionKind,
        ) -> ExpressionResource {
            if !selection.needed() {
                return ExpressionResource::default();
            }
            let path = selection.path(path, kind);
            let (library, error) = match ExpressionLibrary::load(&path, kind) {
                Ok(library) => (Some(Arc::new(library)), None),
                Err(error) => (
                    None,
                    Some(format!("{} ({}): {error}", kind.label(), path.display())),
                ),
            };
            ExpressionResource {
                library,
                path: Some(path),
                error,
            }
        }
        Self {
            emoji: one(config_path, &config.emoji, ExpressionKind::Emoji),
            kaomoji: one(config_path, &config.kaomoji, ExpressionKind::Kaomoji),
            kana: one(config_path, &config.kana, ExpressionKind::Kana),
            math: one(config_path, &config.math, ExpressionKind::Math),
            punctuation: one(
                config_path,
                &config.punctuation,
                ExpressionKind::Punctuation,
            ),
            symbols: one(config_path, &config.symbols, ExpressionKind::Symbols),
        }
    }
    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        ExpressionKind::ALL
            .into_iter()
            .map(|kind| self.resource(kind))
            .filter_map(|resource| resource.path.as_ref())
    }
    pub fn errors(&self) -> impl Iterator<Item = &str> {
        ExpressionKind::ALL
            .into_iter()
            .map(|kind| self.resource(kind))
            .filter_map(|resource| resource.error.as_deref())
    }
}
