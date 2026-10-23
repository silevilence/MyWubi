# 可分发的辅助码表

| 文件 | 条数 | 来源 | 数据许可 |
| --- | ---: | --- | --- |
| `pinyin.dict` | 65,125 | [Rime 袖珍简化字拼音](https://github.com/rime/rime-pinyin-simp)，固定提交 `0c6861ef7420ee780270ca6d993d18d4101049d0` | Apache-2.0 |
| `english.dict` | 79,650 | [SCOWL](https://wordlist.aspell.net/other/) 2020.12.07 | SCOWL 及各组件的宽松许可，详见完整 Copyright |

这两个文件允许按其条款提交 Git、修改和再分发，不因项目代码采用 MIT 而变更其原有许可。完整许可、归属和修改说明已嵌入各自 `.dict` 的注释中；`licenses/` 另存上游原文，供审阅。现有构建和打包流程复制 `.dict` 时会同时带走内嵌声明。

## 在 MyWubi 中使用

将两个文件放在正在使用的主码表旁边。在「码表与词库 → 管理当前码表」的临时输入配置中，填写拼音码表 `pinyin.dict`、英文码表 `english.dict`，入口键可设为反引号（键盘上的 `` ` ``）。也可以在主码表已有的 YAML 头中合并下列配置，不要覆盖其原有编码字符集等字段：

```yaml
temporary:
  key: '`'
  pinyin_table: pinyin.dict
  english_table: english.dict
  reverse_lookup: true
```

这里的相对路径以**主码表所在目录**为基准，不要再加 `tables/`。无正在输入的编码时，按反引号打开菜单，`1` 进入临时拼音、`2` 进入临时英文。空格提交首选，数字选词，回车提交原文，一次上屏后返回主模式。

拼音支持 `nihao` / `ni'hao` → 你好、`xi'an` → 西安，`v` 表示 ü；英文支持完整单词及前缀补全。也可在「使用码表」中直接选择 `pinyin.dict` 作为主码表。本次只增加可选码表，不更改当前启用的五笔码表或全局配置。

## 转换规则与取舍

- 拼音：保留上游所有字词、读音及整数词频；将 `文字<TAB>拼音<TAB>词频` 转为 `编码<TAB>文字<TAB>词频`，音节间空格改为单引号。声明 `type: pinyin`，最大码长 64（不计分隔符），关闭万能键。这个简体优先的基础库仍含少量繁体/异体字，不是完整的现代热词库。
- 英文：合并 `english-words`、`american-words`、`british-words` 的等级 ≤60，共 21 个词表；另外从 `english-upper.10` 仅取代词 `I`。常规词只保留纯 ASCII 小写单词，编码为小写，输出保留 `I` 的大小写；不加载独立的专名表，不剥去重音或标点来制造新拼写。含撇号、连字符、重音或其他大写形式的条目不收录。
- 英文权重为 `(100 - SCOWL 等级) × 1000`，同词取最高权重。这是粗粒度的常用程度排序，**不是实际语料词频**。美式和英式拼写同时保留。声明 `type: shape`、最大码长 32，以复用现有英文字面前缀检索；没有额外英文类型或简写规则。
- 两表按编码和词条稳定排序，以 `(编码, 词条)` 去重；本次拼音没有重复记录。输出为 UTF-8、LF 换行。

## 复现和核验

需要 Python 3.10+，无第三方依赖。在仓库根目录执行：

```powershell
python scripts/build_supplemental_tables.py --download
python scripts/build_supplemental_tables.py --check
cargo test -p core_engine --test bundled_dictionaries --locked
```

首次命令下载固定版本并校验 SHA-256，原始文件缓存在 Git 忽略的 `target/dictionary-sources/`；已有缓存时不联网。`--check` 只比较生成结果，不改写文件。脚本不会执行下载内容，也不把归档路径解压到磁盘。

`sources.json` 记录上游 URL、输入和输出校验值、实际条数、英文源文件及过滤统计。SCOWL 使用 Debian 保存的原始上游发布包；包哈希与 Debian `.dsc` 的声明一致，详见 [来源核查报告](../docs/dictionary-source-research.md)。更新来源时必须重新核查许可、版本和校验值，不能简单取消哈希检查。

## 再分发与编辑

请保留 `.dict` 中完整的版权注释，或在改写、另存和分享时随附 `licenses/` 中对应的完整声明及修改说明。当前码表编辑器重新序列化保存时不会保留原始注释，因此编辑后再分发应重新附上这些声明。不要仅复制词条、丢弃许可；也不要以原作者或相关机构的名义为产品背书。

本说明及许可结论只适用于上面两个辅助码表，不覆盖目录内其他码表。
