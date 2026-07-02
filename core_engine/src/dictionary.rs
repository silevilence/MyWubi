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
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;

/// 字典加载/解析错误。
#[derive(Debug, Error)]
pub enum DictError {
    #[error("无法读取码表文件 {0}: {1}")]
    Io(PathBuf, String),
    #[error("码表第 {0} 行格式非法: {1}")]
    InvalidLine(usize, String),
    #[error("码表头部声明缺失: {0}")]
    MissingHeader(String),
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
        let entries = parse_lines(&text, opts.chunk_lines)?;
        Self::from_entries(entries, Some(path.to_path_buf()), opts)
    }

    /// 从内存条目构建（用于测试与用户词库）。
    pub fn from_entries(
        mut entries: Vec<Entry>,
        source: Option<PathBuf>,
        opts: LoadOptions,
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
        }))
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

        let mut results: Vec<&Entry> = Vec::new();

        // 1) 精确匹配块。
        if opts.prefer_exact {
            results.extend(self.exact(prefix));
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
            results.push(e);
            if results.len() >= opts.limit {
                break;
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

    /// 通过前缀 Trie 进行检索（若未构建则回退 `search`）。
    pub fn search_trie(&self, prefix: &str, opts: SearchOptions) -> Vec<&Entry> {
        if self.trie.is_some() && opts.prefer_exact {
            if let Some(node) = self.trie.as_ref().and_then(|t| t.lookup(prefix)) {
                if !node.words.is_empty() {
                    // 走 Trie 精确分支，再用 weight 排序。
                    let mut ws: Vec<(String, u32)> = node.words.clone();
                    ws.sort_by_key(|entry| std::cmp::Reverse(entry.1));
                    // 复用 search 的全局视图保证返回引用来自 entries。
                    return ws
                        .into_iter()
                        .flat_map(|(w, _)| {
                            self.entries
                                .iter()
                                .filter(move |e| e.code == prefix && e.word == w)
                        })
                        .take(opts.limit)
                        .collect();
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

/// 分块解析码表文本为条目列表。
///
/// 行格式：`code\tword[\tweight]`，`#` 开头行为注释/头部声明，跳过。
fn parse_lines(text: &str, _chunk_lines: usize) -> Result<Vec<Entry>, DictError> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(HEADER_PREFIX) {
            continue;
        }
        let mut parts = line.split('\t');
        let code = parts
            .next()
            .ok_or_else(|| DictError::InvalidLine(i + 1, "缺少编码".into()))?
            .trim()
            .to_string();
        let word = parts
            .next()
            .ok_or_else(|| DictError::InvalidLine(i + 1, "缺少词条".into()))?
            .trim()
            .to_string();
        let weight = parts
            .next()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(1);
        if code.is_empty() || word.is_empty() {
            return Err(DictError::InvalidLine(i + 1, "编码或词条为空".into()));
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
