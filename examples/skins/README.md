# 皮肤作者模板

这些 ZIP 包可直接从设置的皮肤页导入，导入不会自动应用。参数配置与图片均不包含码表、输入历史、个人词库或本机路径。

| 文件 | 用途与预期 |
|---|---|
| bamboo-dual.mywubi-skin | 原创「竹纸」，Windows 与 Android 共用一份 PNG，分别配置九宫格和内容留白 |
| bamboo-windows.mywubi-skin | Windows 单端；Android 可保存，但不能应用 |
| bamboo-android.mywubi-skin | Android 单端；Windows 可制作，但不能应用到 Windows 输入法 |
| parameters-dual.mywubi-skin | 不依赖图片能力的浅深色参数模板 |
| compat-future-required.mywubi-skin | Android 外观缺少必需的 future/9 能力，应禁用该组件；Windows 可用 |
| compat-optional-plain.mywubi-skin | 图片为可选能力，旧参数客户端使用明确声明的 plain 回退 |
| invalid-damaged-png.mywubi-skin | 故意损坏 PNG 校验；不能预览、应用或覆盖活动皮肤 |

格式及字段见 [契约](../../docs/skin-package-format.md)，操作步骤见 [作者指南](../../docs/skin-author-guide.md)。
`assets/bamboo-paper.png` 是通过内置 imagegen 生成并人工选用的原始 PNG，未做裁切或改色。
它是静态装饰，不含键位；两端文字和状态反馈使用原生参数样式。素材随本仓库 MIT 许可提供。
生成提示和来源记录在 [provenance.md](provenance.md)。

复用仓库素材重新生成到一个新目录：

```powershell
cargo run -p core_engine --example skin_templates --offline --locked -- target/skin-templates
```

生成器不覆盖同名文件。需要新的输出时换一个目录；不要为验证重新下载图片、字体或模型。
最终设备覆盖和限制见 [验收记录](../../docs/skin-validation.md)。
