# 内置码表与许可

## 五笔 86

`wubi86.dict` 已替换开发占位表，使用 [Rime 官方五笔 86](https://github.com/rime/rime-wubi) 的固定提交 `152a0d3f3efe40cae216d1e3b338242446848d07`，保留 **136,239 条**字词。该版本也由 [Debian rime-data-wubi](https://packages.debian.org/sid/rime-data-wubi) 分发；这是选择其作为稳定基础表的依据，不代表保证所有用词习惯或热词覆盖。

数据按 **LGPL-3.0** 分发，与本项目程序代码的 MIT 许可分开。`.dict` 是可直接修改的 UTF-8 源码表，文件内嵌上游作者、修改说明、完整 LGPL 与 GPL 文本；`licenses/rime-wubi-AUTHORS.txt`、`licenses/rime-wubi-LICENSE.txt`、`licenses/GPL-3.0.txt` 另存原文。发布和分享时保留这些声明，修改后的码表继续按 LGPL-3.0 提供可编辑形式。GPL 文本复用本机 Git for Windows 所附 XZ 的标准 `COPYING.GPLv3`，脚本校验其 SHA-256；规范文本见 [GNU GPL v3](https://www.gnu.org/licenses/gpl-3.0.html)。

默认配置已关联同目录下的 `pinyin.dict`、`english.dict`，并开启五笔反查。Windows 在空编码时按反引号，再按 `1` 进入拼音、`2` 进入英文；Android 使用键盘的拼音入口。输入 `nihao` 可选「你好」，拼音候选同时提示对应五笔编码，一次上屏后返回主模式。两端现有码表编辑界面可修改辅助表和反查配置；Windows 还可修改入口键。

转换只调整格式，不重编五笔编码：保留原整数权重，163 条没有权重的记录设为 0；省略 Rime 专用的造词 stem 列；按编码、词条排序。上游共 136,916 条，其中 677 条 `z` 开头的符号快捷码被排除，以保留 MyWubi 的 `z` 万能键。其余记录无重复。常用示例：`a → 工`、`g → 一`、`wqiy → 你`、`gggg → 王`、`ggtt → 五笔`。

新安装使用正式表。已有用户目录中的码表不会被启动程序自动覆盖；要更新，请备份自定义表后，将发行包内的三个 `.dict` 及许可一并放入使用中的码表目录，或导入为新码表后切换。不要继续依赖旧占位表里的示例编码。

## 码表显示名称

可在 `.dict` 文件开头的 YAML 头中设置 `display_name`，PC 码表选择器、安卓码表列表及键盘状态栏会优先显示它：

```yaml
---
display_name: "我的五笔"
charset: abcdefghijklmnopqrstuvwxyz
max_code_len: 4
---
```

词条仍放在结束分隔符之后。已有 YAML 头时只需添加 `display_name`，保留其他字段。
省略、空字符串或全空白名称会回退到原有名称（PC 文件名；安卓内置名称或导入文件名）。
名称仅用于显示，文件路径、辅助表引用和选择标识保持不变；两端码表编辑器也可修改并保存名称。

## 辅助码表来源

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

拼音支持 `nihao` / `ni'hao` → 你好、`xi'an` → 西安，`v` 表示 ü；英文支持完整单词及前缀补全。也可在「使用码表」中直接选择 `pinyin.dict` 作为主码表。内置五笔 86 已预设上述辅助配置；自定义主码表可按需填写。

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

本说明及许可结论适用于 `wubi86.dict`、`pinyin.dict`、`english.dict`，不覆盖目录内其他码表。
