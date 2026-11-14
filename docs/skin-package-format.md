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
