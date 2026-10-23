# 拼音和英文码表来源核查

核查日期：2026-10-23。以下结论针对列出的具体上游版本；生成文件的条数、转换规则和校验值以码表目录中的来源说明为准。

## 选型结论

| 用途 | 建议来源 | 许可结论 | 使用取舍 |
| --- | --- | --- | --- |
| 简体拼音 | Rime `rime-pinyin-simp` | Apache-2.0，满足保留声明等条件后可以转换、提交 Git 和再分发 | 包含字词、全拼和权重，适合作为本软件的拼音基础词库 |
| 英文单词 | SCOWL 2020.12.07，等级不超过 60 | 汇编及组件允许再分发，须保留完整版权条款和修改说明 | 可按常用程度和美英拼写筛选，避免超大生僻词库；等级不能冒充精确语料词频 |

这些建议基于下列一手来源。未采用仅有“免费下载”说明、没有明确再分发授权的词库。

## 拼音：Rime 袖珍简化字拼音

固定提交为 `0c6861ef7420ee780270ca6d993d18d4101049d0`。

上游 [AUTHORS](https://github.com/rime/rime-pinyin-simp/blob/0c6861ef7420ee780270ca6d993d18d4101049d0/AUTHORS) 明确将 `pinyin_simp.dict.yaml` 归为源自 Android Pinyin IME 的 Apache-2.0 数据；[词典文件](https://github.com/rime/rime-pinyin-simp/blob/0c6861ef7420ee780270ca6d993d18d4101049d0/pinyin_simp.dict.yaml) 的文件头也保留了 Android 来源，数据包含文字、拼音、权重三列。它适合作为无需从其他词库推导读音的直接转换来源。

[许可证第 2、4 条](https://github.com/rime/rime-pinyin-simp/blob/0c6861ef7420ee780270ca6d993d18d4101049d0/LICENSE) 允许复制、修改和分发，要求向接收者提供许可证、保留相关归属声明，并显著说明修改。如果上游分发物带有 NOTICE，也应保留相关内容。因此转换后的码表应注明来源、固定提交和格式转换，随表携带 LICENSE 与 AUTHORS。不能把数据重新宣称为本项目独占版权。

质量判断：这是基础词库，不能据此保证新词覆盖或整句输入效果；输入体验还取决于本软件的拼音分词与排序实现。

## 英文：SCOWL 2020.12.07

采用官方 [2020.12.07 发布目录](https://sourceforge.net/projects/wordlist/files/SCOWL/2020.12.07/)，对应 `en-wl/wordlist` 的 `rel-2020.12.07` 标签、提交 `5ef55f9c42730ebe4394a78b77855468a6f15dd2`。这是有固定发布包的历史版本，不是最新版本。

本次 SourceForge 下载端点实际返回 HTML，因此从 [Debian 保存的上游原始 tarball](https://deb.debian.org/debian/pool/main/s/scowl/scowl_2020.12.07.orig.tar.gz) 获取数据。文件长度为 2,569,810 字节，SHA-256 为 `5587667caa20c4891390c2d42dbb4d5c4c3f41bee77af1457ece3ba23fb859cc`，与 [Debian 源包描述文件](https://deb.debian.org/debian/pool/main/s/scowl/scowl_2020.12.07-4.dsc) 的声明一致；完整归档目录读取成功。该核验是哈希比对，不声称已验证描述文件的 PGP 签名。

官方 [SCOWLv1 README](https://wordlist.aspell.net/scowl_v1-readme/) 说明词表按拼写地区、类别和等级拆分，并推荐等级 60 用于拼写检查。可选 `english` 加 `american`、`british` 的常规单词，合并去重后为前缀补全提供美英拼写。等级是粗粒度选词依据，转换中可用于排序，但必须说明它不是实际出现次数。需要纯字母编码时，应明确记录对大小写、连字符、撇号和重音词的处理，避免静默改错单词。

[该版本完整 Copyright](https://github.com/en-wl/wordlist/blob/5ef55f9c42730ebe4394a78b77855468a6f15dd2/scowl/Copyright) 明确允许使用、复制、修改、分发和销售词表及脚本输出，附带多个组件声明。主要要求包括保留版权、许可及免责声明，标明修改，并遵守作者/机构名称不得用于背书宣传的限制。UKACD 还要求显著版权声明及原文随附；其重音信息被其他数据使用，因此不能仅凭选取等级低于 80 就断言完全不涉及它。建议随生成码表原样保留整个 Copyright，而不手工删减组件条款。

## 入库和发布要求

- 码表、来源说明及上游完整许可证一起纳入版本控制；记录下载地址、固定版本、SHA-256、筛选和转换规则。
- 转换后的文件头标明经过修改以及许可证文件位置；英文另保留涉及组件的版权声明。
- 安装包、便携包以及单独分享码表时，也应携带许可文件，不能只在 Git 仓库留一份。
- 本次结论不自动覆盖未来新增来源；更新词库时重新检查对应版本的授权和归属。
