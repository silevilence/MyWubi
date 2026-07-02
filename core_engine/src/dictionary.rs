//! 码表解析与高效检索。
//!
//! 本模块对外提供 [`Dictionary`]，内部采用 **按编码前缀有序的扁平数组 + 二分查找**
//! 作为主检索结构，避免传统 Trie 树大量指针节点带来的内存碎片与缓存不友好访问。
//! 同时维护一份 **基于 `char` 的前缀 Trie** 用于词频感知的前缀检索（体积可控时）。
//!
//! 整个 [`Dictionary`] 只读且 `Send + Sync`，可通过 `Arc<Dictionary>` 全局共享，
//! 配合 [`arc_swap::ArcSwap`](../../arc_swap) 实现热重载时的无锁原子替换。
//!
//! 大码表采用 **分块流式解析**：文件按行分块读取，避免一次性 OOM；
//! 调用方可在 [`LoadOptions`] 中开启 `lazy_prefix` 仅构建索引骨架，按需加载。

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

const DEFAULT_CHARSET: &str = "abcdefghijklmnopqrstuvwxyz";

/// 字典加载/解析错误。
#[derive(Debug, Error)]
pub enum DictError {
    #[error("无法读取码表文件 {0}: {1}")]
    Io(PathBuf, String),
    #[error("码表第 {0} 行格式非法: {1}")]
    InvalidLine(usize, String),
    #[error("码表头部声明缺失: {0}")]
    MissingHeader(String),
    #[error("码表 YAML 头格式非法: {0}")]
    InvalidHeader(String),
}

/// 码表 YAML 头中的配置。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TableConfig {
    /// 万能键字符；`None` 表示禁用。
    #[serde(
        default,
        deserialize_with = "deserialize_wildcard_key",
        serialize_with = "serialize_wildcard_key"
    )]
    pub wildcard_key: Option<char>,
    /// 码表可用编码字符。
    #[serde(default = "default_charset")]
    pub charset: String,
}

/// 码表配置或词条校验问题。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableValidationIssue {
    /// 违规词条下标；`None` 表示 YAML 头配置问题。
    pub entry_index: Option<usize>,
    /// 可直接展示给用户的错误说明。
    pub message: String,
}

/// 码表校验结果，仅保留调用方要求数量的明细。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableValidationReport {
    /// 全部违规项数量。
    pub issue_count: usize,
    /// 前若干条违规明细。
    pub issues: Vec<TableValidationIssue>,
}

impl TableValidationReport {
    /// 是否通过校验。
    pub fn is_valid(&self) -> bool {
        self.issue_count == 0
    }
}

impl Default for TableConfig {
    fn default() -> Self {
        Self {
            wildcard_key: None,
            charset: default_charset(),
        }
    }
}

fn default_charset() -> String {
    DEFAULT_CHARSET.to_owned()
}

fn deserialize_wildcard_key<'de, D>(deserializer: D) -> Result<Option<char>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let mut chars = value.chars();
    let Some(wildcard) = chars.next() else {
        return Ok(None);
    };
    if chars.next().is_some() {
        return Err(serde::de::Error::custom("wildcard_key 必须是单个字符"));
    }
    Ok(Some(wildcard))
}

fn serialize_wildcard_key<S>(value: &Option<char>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.map(|character| character.to_string()).unwrap_or_default())
}

/// 码表中的单个词条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// 形码编码 (如 "ggll")。
    pub code: String,
    /// 对应汉字/词组 (UTF-8)。
    pub word: String,
    /// 词频权重，越大越优先。
    pub weight: u32,
}

/// 检索匹配类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// 编码完全相等。
    Exact,
    /// 编码以查询串为前缀。
    Prefix,
}

/// 检索可选项。
#[derive(Debug, Clone, Copy)]
pub struct SearchOptions {
    /// 是否优先返回精确匹配。
    pub prefer_exact: bool,
    /// 最多返回多少条。
    pub limit: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            prefer_exact: true,
            limit: 10,
        }
    }
}

/// 码表加载选项。
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// 启用前缀 Trie（仅当码表条目数 < `trie_threshold` 时构建）。
    pub trie_threshold: usize,
    /// 每块解析的行数（分块加载粒度）。
    pub chunk_lines: usize,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            trie_threshold: 200_000,
            chunk_lines: 4096,
        }
    }
}

/// 不可变、可被多线程共享的码表。
#[derive(Debug)]
pub struct Dictionary {
    /// 按编码有序排列的扁平条目数组，便于二分查找前缀区间。
    entries: Vec<Entry>,
    /// 编码到首次出现下标的索引（前缀分区加速）。
    /// key 为完整编码，value 为该前缀在 `entries` 中的起始下界。
    prefix_index: BTreeMap<String, usize>,
    /// 是否已构建前缀 Trie（按需构建，码表过大则不构建）。
    trie: Option<TrieNode>,
    /// 来源路径（用于热重载比对）。
    source: Option<PathBuf>,
    /// 码表 YAML 头配置。
    table_config: TableConfig,
}

#[derive(Debug, Default)]
struct TrieNode {
    /// 该节点终止时记录的 (word, weight) 列表（同编码可能多词）。
    words: Vec<(String, u32)>,
    children: BTreeMap<char, TrieNode>,
}

/// 码表文件头部声明（简易 `.dict` 格式）。
///
/// 文件首行可写 `# dict version=1 count=...` 之类的元信息，解析时跳过以 `#` 开头的行。
const HEADER_PREFIX: &str = "#";

impl Dictionary {
    /// 从文件全量加载并构建索引。
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Arc<Self>, DictError> {
        Self::load_with(path, LoadOptions::default())
    }

    /// 带选项加载。
    pub fn load_with<P: AsRef<Path>>(
        path: P,
        opts: LoadOptions,
    ) -> Result<Arc<Self>, DictError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| DictError::Io(path.to_path_buf(), e.to_string()))?;
        let (table_config, entries) = parse_dictionary(&text, opts.chunk_lines)?;
        Self::from_entries_and_config(entries, Some(path.to_path_buf()), opts, table_config)
    }

    /// 从内存条目构建（用于测试与用户词库）。
    pub fn from_entries(
        entries: Vec<Entry>,
        source: Option<PathBuf>,
        opts: LoadOptions,
    ) -> Result<Arc<Self>, DictError> {
        Self::from_entries_and_config(entries, source, opts, TableConfig::default())
    }

    fn from_entries_and_config(
        mut entries: Vec<Entry>,
        source: Option<PathBuf>,
        opts: LoadOptions,
        table_config: TableConfig,
    ) -> Result<Arc<Self>, DictError> {
        // 排序：先按编码字典序，再按 weight 倒序（保证同编码高频在前）。
        entries.sort_by(|a, b| {
            match a.code.cmp(&b.code) {
                Ordering::Equal => b.weight.cmp(&a.weight),
                other => other,
            }
        });

        // 构建前缀索引（按每个编码完整键入 BTreeMap，用于二分初定位）。
        let mut prefix_index = BTreeMap::new();
        for (idx, e) in entries.iter().enumerate() {
            prefix_index
                .entry(e.code.clone())
                .or_insert(idx);
        }

        // 单独维护同编码 word 去重的稳定视图（保留 weight 最大者）。
        let trie = if entries.len() < opts.trie_threshold {
            Some(build_trie(&entries))
        } else {
            None
        };

        Ok(Arc::new(Self {
            entries,
            prefix_index,
            trie,
            source,
            table_config,
        }))
    }

    /// 使用当前码表配置重建条目视图。
    pub fn rebuild_with_entries(
        &self,
        entries: Vec<Entry>,
        opts: LoadOptions,
    ) -> Result<Arc<Self>, DictError> {
        Self::from_entries_and_config(
            entries,
            self.source.clone(),
            opts,
            self.table_config.clone(),
        )
    }

    /// 码表条目总数。
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 来源路径。
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    /// 码表 YAML 头配置。
    pub fn table_config(&self) -> &TableConfig {
        &self.table_config
    }

    /// 返回只读词条视图，供系统码表与用户词库合并。
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// 返回码表中所有编码完全等于 `code` 的词条（按 weight 倒序）。
    pub fn exact(&self, code: &str) -> Vec<&Entry> {
        let Some(&start) = self.prefix_index.get(code) else {
            return Vec::new();
        };
        self.entries[start..]
            .iter()
            .take_while(|e| e.code == code)
            .collect()
    }

    /// 返回以 `prefix` 为前缀的全部词条（精确编码优先，其次前缀）。
    pub fn search(&self, prefix: &str, opts: SearchOptions) -> Vec<&Entry> {
        if prefix.is_empty() {
            return Vec::new();
        }
        if let Some(wildcard) = self
            .table_config
            .wildcard_key
            .filter(|wildcard| prefix.contains(*wildcard))
        {
            return self.search_wildcard(prefix, wildcard, opts);
        }

        let mut results: Vec<&Entry> = Vec::new();
        let mut seen = HashSet::new();

        // 1) 精确匹配块。
        if opts.prefer_exact {
            results.extend(
                self.exact(prefix)
                    .into_iter()
                    .filter(|entry| seen.insert(entry.word.as_str())),
            );
        }

        // 2) 前缀匹配块（在排序数组上二分定位前缀区间下界）。
        // 数组按完整编码字典序排序，以 `prefix` 为前缀的条目构成一段连续区间：
        //   下界 = 第一条 code >= prefix；
        //   上界 = 从下界起连续 `starts_with(prefix)` 的条目数。
        let lower = self
            .entries
            .partition_point(|e| e.code.as_str() < prefix);
        let upper = lower
            + self.entries[lower..]
                .partition_point(|e| e.code.as_str().starts_with(prefix));
        for e in &self.entries[lower..upper] {
            if e.code == prefix {
                continue; // 已在 exact 阶段收集过
            }
            if seen.insert(e.word.as_str()) {
                results.push(e);
                if results.len() >= opts.limit {
                    break;
                }
            }
        }

        // 按权重稳定重排：精确优先，再按 weight 倒序。
        results.sort_by(|a, b| {
            let a_exact = a.code == prefix;
            let b_exact = b.code == prefix;
            match (a_exact, b_exact) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => b.weight.cmp(&a.weight),
            }
        });

        results.truncate(opts.limit);
        results
    }

    fn search_wildcard(
        &self,
        pattern: &str,
        wildcard: char,
        opts: SearchOptions,
    ) -> Vec<&Entry> {
        let literal_prefix = pattern
            .split_once(wildcard)
            .map_or(pattern, |(prefix, _)| prefix);
        let lower = self
            .entries
            .partition_point(|entry| entry.code.as_str() < literal_prefix);
        // ponytail: 万能键在首位时扫描全表；实测成为瓶颈后再加位置索引。
        let upper = if literal_prefix.is_empty() {
            self.entries.len()
        } else {
            lower
                + self.entries[lower..]
                    .partition_point(|entry| entry.code.starts_with(literal_prefix))
        };
        let mut results: Vec<&Entry> = self.entries[lower..upper]
            .iter()
            .filter(|entry| wildcard_match(&entry.code, pattern, wildcard))
            .collect();
        results.sort_by_key(|entry| std::cmp::Reverse(entry.weight));
        dedup_by_word(&mut results);
        results.truncate(opts.limit);
        results
    }

    /// 通过前缀 Trie 进行检索（若未构建则回退 `search`）。
    pub fn search_trie(&self, prefix: &str, opts: SearchOptions) -> Vec<&Entry> {
        if self
            .table_config
            .wildcard_key
            .is_some_and(|wildcard| prefix.contains(wildcard))
        {
            return self.search(prefix, opts);
        }
        if self.trie.is_some() && opts.prefer_exact {
            if let Some(node) = self.trie.as_ref().and_then(|t| t.lookup(prefix)) {
                if !node.words.is_empty() {
                    // 走 Trie 精确分支，再用 weight 排序。
                    let mut ws: Vec<(String, u32)> = node.words.clone();
                    ws.sort_by_key(|entry| std::cmp::Reverse(entry.1));
                    // 复用 search 的全局视图保证返回引用来自 entries。
                    let mut results: Vec<&Entry> = ws
                        .into_iter()
                        .flat_map(|(w, _)| {
                            self.entries
                                .iter()
                                .filter(move |e| e.code == prefix && e.word == w)
                        })
                        .collect();
                    dedup_by_word(&mut results);
                    results.truncate(opts.limit);
                    return results;
                }
            }
        }
        self.search(prefix, opts)
    }

    /// 判断是否存在至少一条前缀匹配（用于"还有候选可继续输入"提示）。
    pub fn has_prefix(&self, prefix: &str) -> bool {
        if prefix.is_empty() {
            return false;
        }
        self.entries
            .partition_point(|e| e.code.as_str() < prefix)
            < self.entries.len()
            && self.entries[self.entries.partition_point(|e| e.code.as_str() < prefix)]
                .code
                .starts_with(prefix)
    }

    /// 是否存在唯一精确匹配（用于四码唯一自动上屏判断）。
    pub fn unique_exact(&self, code: &str) -> Option<&Entry> {
        self.exact(code).into_iter().next()
    }
}

impl TrieNode {
    fn lookup(&self, prefix: &str) -> Option<&Self> {
        let mut node = self;
        for ch in prefix.chars() {
            node = node.children.get(&ch)?;
        }
        Some(node)
    }
}

fn build_trie(entries: &[Entry]) -> TrieNode {
    let mut root = TrieNode::default();
    for e in entries {
        let node = e
            .code
            .chars()
            .fold(&mut root, |node, ch| node.children.entry(ch).or_default());
        node.words.push((e.word.clone(), e.weight));
    }
    root
}

fn wildcard_match(code: &str, pattern: &str, wildcard: char) -> bool {
    let mut code_chars = code.chars();
    for pattern_char in pattern.chars() {
        let Some(code_char) = code_chars.next() else {
            return false;
        };
        if pattern_char != wildcard && pattern_char != code_char {
            return false;
        }
    }
    code_chars.next().is_none()
}

fn dedup_by_word(entries: &mut Vec<&Entry>) {
    let mut seen = HashSet::new();
    entries.retain(|entry| seen.insert(entry.word.as_str()));
}

/// 分块解析码表文本为条目列表。
///
/// 行格式：`code\tword[\tweight]`，`#` 开头行为注释/头部声明，跳过。
#[cfg(test)]
fn parse_lines(text: &str, _chunk_lines: usize) -> Result<Vec<Entry>, DictError> {
    parse_lines_with_config(text, 0, None)
}

fn parse_dictionary(
    text: &str,
    _chunk_lines: usize,
) -> Result<(TableConfig, Vec<Entry>), DictError> {
    let (config, body, line_offset) = split_yaml_header(text)?;
    validate_table_config(&config)?;
    let entries = parse_lines_with_config(body, line_offset, config.wildcard_key)?;
    Ok((config, entries))
}

fn split_yaml_header(text: &str) -> Result<(TableConfig, &str, usize), DictError> {
    let mut lines = text.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return Ok((TableConfig::default(), text, 0));
    };
    let first_line = first
        .trim_end_matches(&['\r', '\n'][..])
        .trim_start_matches('\u{feff}');
    if first_line != "---" {
        return Ok((TableConfig::default(), text, 0));
    }

    let header_start = first.len();
    let mut header_end = header_start;
    for (index, line) in lines.enumerate() {
        let line_number = index + 2;
        if line.trim_end_matches(&['\r', '\n'][..]) == "---" {
            let header = &text[header_start..header_end];
            let config = if header.trim().is_empty() {
                TableConfig::default()
            } else {
                serde_yaml_ng::from_str(header)
                    .map_err(|error| DictError::InvalidHeader(error.to_string()))?
            };
            return Ok((config, &text[header_end + line.len()..], line_number));
        }
        header_end += line.len();
    }

    Err(DictError::MissingHeader(
        "YAML 头缺少结束分隔符 `---`".into(),
    ))
}

fn validate_table_config(config: &TableConfig) -> Result<(), DictError> {
    if let Some(wildcard) = config.wildcard_key {
        if !config.charset.contains(wildcard) {
            return Err(DictError::InvalidHeader(format!(
                "wildcard_key `{wildcard}` 不在 charset 中"
            )));
        }
    }
    Ok(())
}

/// 读取可编辑码表，不因语义校验问题拒绝加载。
pub fn read_table<P: AsRef<Path>>(path: P) -> Result<(TableConfig, Vec<Entry>), DictError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|error| DictError::Io(path.to_path_buf(), error.to_string()))?;
    let (config, body, line_offset) = split_yaml_header(&text)?;
    let entries = parse_lines_with_config(body, line_offset, None)?;
    Ok((config, entries))
}

/// 校验码表 YAML 头与词条编码，最多保留 `max_issues` 条明细。
pub fn validate_table(
    config: &TableConfig,
    entries: &[Entry],
    max_issues: usize,
) -> TableValidationReport {
    let mut report = TableValidationReport::default();
    let mut push_issue = |entry_index, message| {
        report.issue_count += 1;
        if report.issues.len() < max_issues {
            report.issues.push(TableValidationIssue {
                entry_index,
                message,
            });
        }
    };

    if config.charset.is_empty() {
        push_issue(None, "charset 不能为空".into());
    }
    if config
        .charset
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        push_issue(None, "charset 不能包含空白或控制字符".into());
    }
    if let Some(wildcard) = config.wildcard_key {
        if !config.charset.contains(wildcard) {
            push_issue(
                None,
                format!("wildcard_key `{wildcard}` 不在 charset 中"),
            );
        }
    }

    for (index, entry) in entries.iter().enumerate() {
        if entry.code.is_empty() || entry.word.is_empty() {
            push_issue(Some(index), "编码或词条为空".into());
            continue;
        }
        if entry.code.contains(['\t', '\r', '\n'])
            || entry.word.contains(['\t', '\r', '\n'])
        {
            push_issue(Some(index), "编码或词条不能包含制表符或换行".into());
            continue;
        }
        if let Some(character) = entry
            .code
            .chars()
            .find(|character| !config.charset.contains(*character))
        {
            push_issue(
                Some(index),
                format!("编码 `{}` 含 charset 外字符 `{character}`", entry.code),
            );
        }
        if let Some(wildcard) = config.wildcard_key {
            if entry.code.contains(wildcard) {
                push_issue(
                    Some(index),
                    format!("编码 `{}` 不得包含万能键 `{wildcard}`", entry.code),
                );
            }
        }
    }

    report
}

/// 保存完整码表文件。
pub fn save_table<P: AsRef<Path>>(
    path: P,
    config: &TableConfig,
    entries: &[Entry],
) -> Result<(), DictError> {
    let path = path.as_ref();
    let report = validate_table(config, entries, 1);
    if let Some(issue) = report.issues.first() {
        return match issue.entry_index {
            Some(index) => Err(DictError::InvalidLine(index + 1, issue.message.clone())),
            None => Err(DictError::InvalidHeader(issue.message.clone())),
        };
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| DictError::Io(parent.to_path_buf(), error.to_string()))?;
        }
    }

    let header = serde_yaml_ng::to_string(config)
        .map_err(|error| DictError::InvalidHeader(error.to_string()))?;
    let body_size = entries
        .iter()
        .map(|entry| entry.code.len() + entry.word.len() + 16)
        .sum::<usize>();
    let mut text = String::with_capacity(header.len() + body_size + 8);
    text.push_str("---\n");
    text.push_str(&header);
    text.push_str("---\n");
    for entry in entries {
        use std::fmt::Write;
        let _ = writeln!(text, "{}\t{}\t{}", entry.code, entry.word, entry.weight);
    }

    std::fs::write(path, text)
        .map_err(|error| DictError::Io(path.to_path_buf(), error.to_string()))
}

fn parse_lines_with_config(
    text: &str,
    line_offset: usize,
    wildcard_key: Option<char>,
) -> Result<Vec<Entry>, DictError> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line_number = line_offset + i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with(HEADER_PREFIX) {
            continue;
        }
        let mut parts = line.split('\t');
        let code = parts
            .next()
            .ok_or_else(|| DictError::InvalidLine(line_number, "缺少编码".into()))?
            .trim()
            .to_string();
        let word = parts
            .next()
            .ok_or_else(|| DictError::InvalidLine(line_number, "缺少词条".into()))?
            .trim()
            .to_string();
        let weight = parts
            .next()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(1);
        if code.is_empty() || word.is_empty() {
            return Err(DictError::InvalidLine(line_number, "编码或词条为空".into()));
        }
        if let Some(wildcard) = wildcard_key {
            if code.contains(wildcard) {
                return Err(DictError::InvalidLine(
                    line_number,
                    format!("编码 `{code}` 不得包含万能键 `{wildcard}`"),
                ));
            }
        }
        out.push(Entry { code, word, weight });
    }
    Ok(out)
}

/// 用户新增自造词的追加写接口（写回文件由前端负责，这里只负责内存视图。
///
/// 由于 `Dictionary` 不可变，新增词应当重建一个新实例并替换。该函数默认导出，
/// 供 `settings.exe` / Compose 配置界面调用。
pub fn append_user_table<P: AsRef<Path>>(
    path: P,
    entry: &Entry,
) -> Result<(), DictError> {
    use std::io::Write;
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DictError::Io(parent.to_path_buf(), e.to_string()))?;
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| DictError::Io(path.to_path_buf(), e.to_string()))?;
    let line = format!("{}\t{}\t{}\n", entry.code, entry.word, entry.weight);
    file.write_all(line.as_bytes())
        .map_err(|e| DictError::Io(path.to_path_buf(), e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> Arc<Dictionary> {
        let entries = vec![
            Entry { code: "ggll".into(), word: "王".into(), weight: 100 },
            Entry { code: "ggll".into(), word: "丰".into(), weight: 20 },
            Entry { code: "ggll".into(), word: "壬".into(), weight: 50 },
            Entry { code: "ggh".into(), word: "理".into(), weight: 80 },
            Entry { code: "gghg".into(), word: "五".into(), weight: 200 },
            Entry { code: "a".into(), word: "工".into(), weight: 999 },
        ];
        Dictionary::from_entries(entries, None, LoadOptions::default()).unwrap()
    }

    #[test]
    fn exact_match_returns_by_weight_desc() {
        let d = dict();
        let r = d.exact("ggll");
        assert_eq!(r.iter().map(|e| e.word.as_str()).collect::<Vec<_>>(), ["王", "壬", "丰"]);
    }

    #[test]
    fn exact_match_unique() {
        let d = dict();
        assert_eq!(d.unique_exact("a").map(|e| e.word.as_str()), Some("工"));
        assert!(d.unique_exact("zzz").is_none());
    }

    #[test]
    fn prefix_search() {
        let d = dict();
        let r = d.search("gg", SearchOptions { prefer_exact: true, limit: 10 });
        assert_eq!(r.len(), 5);
        assert_eq!(r[0].word, "五");
        // 完全匹配 "ggll" 应当优先于仅前缀。
        assert_eq!(r[1].word, "王");
    }

    #[test]
    fn limit_truncates() {
        let d = dict();
        let r = d.search("gg", SearchOptions { prefer_exact: true, limit: 2 });
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn has_prefix_works() {
        let d = dict();
        assert!(d.has_prefix("gg"));
        assert!(d.has_prefix("g"));
        assert!(!d.has_prefix("zz"));
    }

    #[test]
    fn parse_lines_basic() {
        let text = "# code\tword\tweight\na\t工\t999\nggll\t王\t100\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].code, "a");
        assert_eq!(e[0].word, "工");
        assert_eq!(e[0].weight, 999);
    }

    #[test]
    fn parse_dictionary_uses_defaults_without_yaml_header() {
        let (config, entries) = parse_dictionary("a\t工\t999\n", 4096).unwrap();

        assert_eq!((config, entries.len()), (TableConfig::default(), 1));
    }

    #[test]
    fn parse_dictionary_reads_yaml_header() {
        let text = "---\nwildcard_key: z\ncharset: abcdefghijklmnopqrstuvwxyz\n---\na\t工\t999\n";
        let (config, entries) = parse_dictionary(text, 4096).unwrap();

        assert_eq!(
            (config.wildcard_key, config.charset.as_str(), entries.len()),
            (Some('z'), DEFAULT_CHARSET, 1)
        );
    }

    #[test]
    fn parse_dictionary_treats_empty_wildcard_as_disabled() {
        let text = "---\nwildcard_key: \"\"\ncharset: abcdefghijklmnopqrstuvwxyz\n---\na\t工\n";
        let (config, _) = parse_dictionary(text, 4096).unwrap();

        assert_eq!(config.wildcard_key, None);
    }

    #[test]
    fn parse_dictionary_rejects_wildcard_outside_charset() {
        let text = "---\nwildcard_key: z\ncharset: abc\n---\na\t工\n";
        let error = parse_dictionary(text, 4096).unwrap_err();

        assert!(matches!(error, DictError::InvalidHeader(_)));
    }

    #[test]
    fn parse_dictionary_rejects_wildcard_in_body_code() {
        let text = "---\nwildcard_key: z\ncharset: az\n---\naz\t工\n";
        let error = parse_dictionary(text, 4096).unwrap_err();

        assert!(matches!(error, DictError::InvalidLine(5, _)));
    }

    #[test]
    fn parse_dictionary_rejects_multi_character_wildcard() {
        let text = "---\nwildcard_key: zz\ncharset: az\n---\na\t工\n";
        let error = parse_dictionary(text, 4096).unwrap_err();

        assert!(matches!(error, DictError::InvalidHeader(_)));
    }

    #[test]
    fn parse_dictionary_rejects_unclosed_yaml_header() {
        let error = parse_dictionary("---\ncharset: abc\n", 4096).unwrap_err();

        assert!(matches!(error, DictError::MissingHeader(_)));
    }

    #[test]
    fn validate_table_reports_charset_and_wildcard_violations() {
        let config = TableConfig {
            wildcard_key: Some('z'),
            charset: "abz".into(),
        };
        let entries = vec![
            Entry { code: "ac".into(), word: "越界".into(), weight: 1 },
            Entry { code: "az".into(), word: "万能键".into(), weight: 1 },
        ];

        let report = validate_table(&config, &entries, 10);

        assert_eq!(report.issue_count, 2);
    }

    #[test]
    fn save_table_roundtrip_preserves_header_and_entries() {
        let path = std::env::temp_dir().join(format!(
            "mywubi-save-table-{}-{}.dict",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = TableConfig {
            wildcard_key: Some('?'),
            charset: "abc?".into(),
        };
        let entries = vec![Entry {
            code: "abc".into(),
            word: "目标".into(),
            weight: 42,
        }];

        save_table(&path, &config, &entries).unwrap();
        let (loaded_config, loaded_entries) = read_table(&path).unwrap();
        let _ = std::fs::remove_file(path);

        assert_eq!((loaded_config, loaded_entries), (config, entries));
    }

    #[test]
    fn parse_lines_default_weight() {
        let text = "ggll\t王\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e[0].weight, 1);
    }

    #[test]
    fn parse_rejects_invalid_line() {
        let text = "broken-line\n";
        let err = parse_lines(text, 4096).unwrap_err();
        assert!(matches!(err, DictError::InvalidLine(_, _)));
    }

    // ── 边界条件 & 非法字符处理 ────────────────────────────────

    #[test]
    fn parse_empty_text_yields_empty() {
        let e = parse_lines("", 4096).unwrap();
        assert!(e.is_empty());
    }

    #[test]
    fn parse_only_comments_and_blanks() {
        let text = "# header\n\n# another\n   \n";
        let e = parse_lines(text, 4096).unwrap();
        assert!(e.is_empty());
    }

    #[test]
    fn parse_crlf_line_endings() {
        // Windows 风格换行应当被正确处理（trim 移除 \r）。
        let text = "a\t工\t999\r\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].word, "工");
        assert!(!e[0].word.contains('\r'));
    }

    #[test]
    fn parse_trailing_whitespace_is_trimmed() {
        let text = "a\t工\t999   \n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e[0].word, "工");
        assert_eq!(e[0].weight, 999);
    }

    #[test]
    fn parse_extra_trailing_tab_ignored() {
        let text = "a\t工\t999\t\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e[0].weight, 999);
    }

    #[test]
    fn parse_multiple_consecutive_tabs_in_weight_field() {
        // 第二个字段后若有额外 Tab，weight 解析仍应成功。
        let text = "a\t工\t999\textra\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e[0].weight, 999);
    }

    #[test]
    fn parse_rejects_empty_word_field() {
        // 编码后紧跟两个 Tab 表示词字段为空，应当报错。
        let err = parse_lines("a\t\t1\n", 4096).unwrap_err();
        assert!(matches!(err, DictError::InvalidLine(_, _)));
    }

    #[test]
    fn parse_rejects_line_with_only_code_field() {
        let err = parse_lines("abc\n", 4096).unwrap_err();
        assert!(matches!(err, DictError::InvalidLine(_, _)));
    }

    #[test]
    fn parse_non_numeric_weight_falls_back_to_default() {
        let text = "a\t工\tNaN\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e[0].weight, 1);
    }

    #[test]
    fn parse_non_numeric_negative_weight_falls_back_to_default() {
        // u32 parse 失败 → 默认权重 1。
        let text = "a\t工\t-5\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e[0].weight, 1);
    }

    #[test]
    fn parse_max_u32_weight() {
        let text = format!("a\t工\t{}\n", u32::MAX);
        let e = parse_lines(&text, 4096).unwrap();
        assert_eq!(e[0].weight, u32::MAX);
    }

    #[test]
    fn parse_weight_overflow_falls_back_to_default() {
        // u32::MAX + 1 无法解析为 u32，应回退默认权重。
        let text = "a\t工\t4294967296\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e[0].weight, 1);
    }

    #[test]
    fn parse_unicode_in_word() {
        let text = "a\t你好世界\t5\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e[0].word, "你好世界");
    }

    #[test]
    fn parse_space_separated_not_supported() {
        // 用空格代替 Tab 分隔应识别为单字段，触发"缺少词条"错误。
        let err = parse_lines("a 工 999\n", 4096).unwrap_err();
        assert!(matches!(err, DictError::InvalidLine(_, _)));
    }

    #[test]
    fn parse_preserves_code_with_digits() {
        let text = "12ab\t工\t1\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e[0].code, "12ab");
    }

    #[test]
    fn parse_ignores_indented_comment() {
        let text = "   # indented comment\na\t工\t1\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].word, "工");
    }

    #[test]
    fn parse_indented_entry_is_trimmed() {
        let text = "   a\t工\t1\n";
        let e = parse_lines(text, 4096).unwrap();
        assert_eq!(e[0].code, "a");
    }

    #[test]
    fn parse_empty_word_after_code() {
        // 编码后仅一个 Tab，词条字段缺失，应当报错。
        let err = parse_lines("a\t\n", 4096).unwrap_err();
        assert!(matches!(err, DictError::InvalidLine(_, _)));
    }

    #[test]
    fn from_entries_handles_duplicate_codes() {
        let entries = vec![
            Entry { code: "x".into(), word: "甲".into(), weight: 5 },
            Entry { code: "x".into(), word: "乙".into(), weight: 10 },
        ];
        let d = Dictionary::from_entries(entries, None, LoadOptions::default()).unwrap();
        let r = d.exact("x");
        assert_eq!(r.len(), 2);
        // 同编码按 weight 倒序。
        assert_eq!(r[0].word, "乙");
        assert_eq!(r[1].word, "甲");
    }

    #[test]
    fn search_empty_prefix_returns_empty() {
        let d = dict();
        assert!(d.search("", SearchOptions::default()).is_empty());
    }

    #[test]
    fn search_limit_zero_returns_empty() {
        let d = dict();
        let opts = SearchOptions { prefer_exact: true, limit: 0 };
        assert!(d.search("a", opts).is_empty());
    }

    #[test]
    fn search_nonexistent_prefix_returns_empty() {
        let d = dict();
        assert!(d.search("zzz", SearchOptions::default()).is_empty());
    }

    #[test]
    fn has_prefix_empty_returns_false() {
        let d = dict();
        assert!(!d.has_prefix(""));
    }

    #[test]
    fn dictionary_is_send_sync() {
        // 编译期断言：Dictionary 必须可跨线程共享。
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Dictionary>();
    }

    #[test]
    fn search_trie_falls_back_when_no_trie() {
        // 强制不构建 Trie（阈值 0）。
        let entries = vec![Entry { code: "a".into(), word: "工".into(), weight: 999 }];
        let d = Dictionary::from_entries(entries, None, LoadOptions { trie_threshold: 0, chunk_lines: 4096 }).unwrap();
        let r = d.search_trie("a", SearchOptions::default());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].word, "工");
    }
}
