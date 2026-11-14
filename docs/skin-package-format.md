# 皮肤包格式 1

SK-02 固定的容器为标准 ZIP（`.mywubi-skin` 或 `.zip`），根文件 `metadata.toml`。所有 metadata 为 UTF-8 TOML。解析入口为 `core_engine::skin::Package::from_files`，其文件映射保留未知 TOML 字段、组件文件和资源的原始字节。导入不意味着应用。

```toml
format = 1
id = "author.skin"
version = "1.0.0"
name = "我的皮肤"
author = "作者"
license = "MIT"
[capabilities]
required = ["container.zip/1"]
[platforms]
windows = "windows/metadata.toml"
android = "android/metadata.toml"
[resources]
paper = "shared/assets/paper.png"
```

包/组件 ID 和版本由 1～64 个 ASCII 字母、数字、点、下划线和连字符组成，不能是 `.` 或 `..`。组件依赖匹配精确版本，不选择“最新”。平台入口由根显式指定且固定为对应目录的 `metadata.toml`。单端包省略另一端；只有布局的包合法。根格式不支持或结构损坏拒绝整包。

```toml
platform = "android"
[capabilities]
required = ["android.base/1"]
[resources]
key = "assets/key.png"
[[components]]
id = "jade"
version = "1"
kind = "appearance"
path = "appearance.toml"
[components.capabilities]
required = ["appearance.parameters/1"]
[components.capabilities.optional]
"appearance.shadow/1" = "none"
[[components]]
id = "fourteen"
version = "1"
kind = "layout"
path = "layouts/fourteen.toml"
[components.capabilities]
required = ["input.groups/1"]
[components.dependencies]
mapping = "1"
```

组件路径相对平台目录。组件数据文件是 TOML。种类以 `appearance`、`layout`、`mapping` 起步；旧客户端只解释自己认识的种类。未知种类禁用但完整保留。能力 ID 带 `/主版本`，必需能力列表与可选能力表不能重复；每项可选能力必须明确回退值。客户端仅支持自己已实现的能力及回退语义，不能把未知动作当文本执行。

根能力失败拒绝整包；平台公共能力失败禁用该平台；组件能力/版本依赖/循环依赖失败只禁用该组件及依赖闭包。另一平台不受影响。布局、分组输入、图片等能力声明在组件层，不提升到公共层。`container.zip/1` 表示此容器契约；`windows.base/1` 和 `android.base/1` 表示平台公共结构；`appearance.parameters/1` 表示参数外观。新组件能力不由目录名称推断。

资源 ID 分为 `shared:<id>` 和 `platform:<id>` 两个命名空间。根索引仅指向 `shared/` 下的资源；平台索引仅指向本平台 `assets/`。禁止绝对路径、URL、反斜杠、空段、点段、Windows 设备别名/ADS、尾点/空格及大小写冲突，文件不能同时作为目录。全部声明资源必须存在；运行端只解码所用平台资源。未知组件的资源依赖不能推断为“未使用”。

有效外观依次覆盖：内置默认 → 根 defaults → 平台 defaults → 所选外观组件 defaults → light/dark 变体 → 用户明确覆盖 → 系统可访问性。仅合并声明支持的标量字段；列表整体替换，空列表显式清空；缺字段继承，类型冲突报错。布局/映射不参加外观合并。只提供单变体时按组件明确声明的默认变体回退并提示，不自动反相。新颜色为 `#RRGGBB` 或 `#RRGGBBAA`，旧 ARGB 必须显式转换。

外观字段及运行时读取由 SK-04 提供；ZIP 读取、限额及导入事务由 SK-03 提供。本文不把 metadata 兼容测试宣称为设备渲染支持。

## ZIP 导入与版本库（SK-03）

压缩包限 64 MiB、展开后总计 128 MiB、单资源 32 MiB、单 TOML 1 MiB、512 个条目、每平台 128 个组件。只接受常规文件和目录，拒绝链接、别名路径、文件/目录冲突及未声明平台目录。读取全部文件并校验后才写版本库；不把 ZIP 内路径提取到文件系统。

`archive::import` 保存为 `<id>@<version>.mywubi-skin`，在版本库内暂存、同步、原子无覆盖发布。完全相同版本重复导入返回 already_present；相同身份内容不同返回 Conflict，调用者提示另存新版本。旧版始终保留，应用层可选择旧版本回退；导入不改配置或输入会话。`export_new` 也不覆盖现有文件。当前 API 不提供删除在用版本的操作。

导出只来自包内文件映射，不读取本机配置、最近输入、词库或绝对路径。未知组件原始文件（包括注释）与共享资源原样保存；已知根/平台 metadata 重新序列化但保留合法扩展字段。运行平台的图片解码另由图片能力实现，ZIP 层不解码任何图片。

## 参数解析与配置（SK-04）

外观文件包含 `default_variant`、`defaults` 与 `variants.light` / `variants.dark`。共享与平台 defaults 也可含 light/dark 子表，先应用公共字段再应用所选变体。字段是扁平语义标量，支持名称见 `parameters::COLORS` 和 `validate_patch`；未知字段、类型冲突、非有限数和越界数报错，包括当前未选中的变体。合法未知组件保持只读。

`parameters::resolve` 供运行端与预览共用。`builtin_package` 生成墨绿、素简和高对比的双端可编辑副本。`skins.windows` / `skins.android` 分别保存 preset（legacy/jade/plain/contrast）、mode（system/light/dark）、selection（id/version/component）、use_skin_metrics 与 overrides。缺少此节按 legacy 解释，不改写旧 appearance。默认保留 Windows 旧字号和 Android 使用偏好，采用推荐字号需显式选择；选择包优先于 preset。选择字段不出现在导出的皮肤包内。

SK-04 只交付共享解析、配置读写和双端设置草稿入口；实际加载/渲染分别由 SK-05/06/12 接入。已验证 5 项定向参数测试、核心严格 Clippy、Windows settings 与 Android JNI 检查、离线 Kotlin 编译。Kotlin 检查复用本地原生库；最终 APK 需重新编译两种 ABI 后统一验收。

## 独立加载（SK-05）

`skin::runtime` 从配置同目录的 skins 版本库加载完整快照，ArcSwap 成功后一次发布，失败保留旧包和参数。Windows 独立线程监听配置/皮肤目录，并检测系统明暗/高对比；候选窗口使用快照指针刷新，DPI 仍通过窗口度量失效。纯外观配置变化保持 RuntimeSnapshot 的 revision、词库和模型资源，不使输入会话重新初始化。

Android 通过 ResolveSkin 在 loader 线程预解析浅深变体；SameInputConfiguration 在配置通知和保存校验时识别纯外观，避免 BuildRuntime 和 ApplyRuntime。皮肤加载失败保留旧外观；冷启动可用旧默认显示。服务保留完整变体，具体键盘语义样式由 SK-12 接入。版本库无删除 API，活动快照持有包字节，因此资源替换失败或旧文件被外部删除不破坏已发布内容。

自动化检查覆盖失败不发布、旧快照仍可读、1000 次切换释放退休快照、字号保留、输入配置等价及 FFI 对待确认输入无副作用。尚未用这些结果替代最终多宿主、可见延迟或真实设备证据。
