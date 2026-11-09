# 内置表情资源

运行时仅加载 `.mwe` 文件。原始数据、许可、固定版本与 SHA-256 留在本目录供离线重建和审计；生成和输入都不访问 `latest`。

## Emoji（EM-02）

- 全集基准：[Unicode Emoji 18.0 emoji-test.txt](https://www.unicode.org/Public/18.0.0/emoji/emoji-test.txt)，3963 个 fully-qualified + 9 个 component = **3972** 个独立输出。
- 中文名称和检索关键词：CLDR **release-49** 的 [annotations/zh.xml](https://github.com/unicode-org/cldr/blob/release-49/common/annotations/zh.xml) 与 [annotationsDerived/zh.xml](https://github.com/unicode-org/cldr/blob/release-49/common/annotationsDerived/zh.xml)。3972 项均有中文完整触发词。CLDR 的 OK、DVD、DNA 名称原样保留，同时有中文关键词。
- 1029 个 minimally-qualified 与 243 个 unqualified（共 1272）存为标准输出的兼容别名，不额外生成候选。只在关联 CLDR/兼容别名时忽略 FE0F，实际输出不归一化。
- 包含全部肤色、性别、发型、职业、家庭、旗帜、键帽和 ZWJ 变体。排序保持 Unicode 文件的 CLDR 顺序，不按字体可用性删减条目。
- `emoji-coverage.json` 是自动生成的逐集合覆盖报告，`missing`、`extra` 均为空，中文触发覆盖 3972/3972；同时列出相较 17.0 的 19 个新增序列，旧版没有被删除的序列。
- Unicode/CLDR 的版权声明及 Unicode License v3 完整保留在 `sources/UNICODE-49-LICENSE.txt`，并内嵌在词库来源记录中。使用的是许可允许再分发的数据文件，不复制该许可另行限制的 UTS #35 规范正文。

### 与用户指定参考站点比较

2026-11-09 查看 [Emojiall 全部 Emoji](https://www.emojiall.com/zh-hans/all-emojis)：页面标示 3971 项，包含变体；站点同时已经列出 Emoji 18.0。固定官方 18.0 数据实测为 3972，**总数比参考页多 1**。站点计数与官方数据不完全一致，不能将这个计数当作标准全集清单，也不推断具体哪项导致差异。

本库使用官方全部序列，覆盖该版本所有 10 组；两边均包含肤色等变体。17.0 的 3953 项已由 18.0 完整保留，新增序列详见报告。直接下载参考站点 HTML 返回 HTTP 403，因此未取得可复核的站点全量逐序列清单；不声称已完成与站点的逐项等价比对。站点仅作版本、分类和数量参考，未复制图片或站点独有释义。

### 离线重建与验证

```powershell
python scripts/build_expressions.py
python scripts/build_expressions.py --check
cargo test -p core_engine --test expression_datasets --offline
```

脚本先核对原始文件大小和 SHA-256，缺失或变化则失败，不联网下载；`--check` 不改文件。Rust 测试独立读取官方 code point 列，逐序列比对输出及所有兼容别名，并验证每个触发词能访问全部对应变体、导出重载无损。原始数据在 `.gitattributes` 中禁止换行转换，避免 Windows checkout 改变校验和。

旧的 17.0/CLDR 48 原始文件保留用于版本对比和准备记录；活动词库固定为 18.0/49。当前生成结果仅代表数据完整性，候选框和宿主的字体显示仍由 EM-06/07/08 验收。
