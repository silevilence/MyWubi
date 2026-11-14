# 键盘组件格式 1（AK-02）

沿用 `skin-package-format.md` 的 ZIP、路径、限额和不可变版本库。`android/metadata.toml` 中的 `layout` 放在 `layouts/`，`mapping` 放在 `mappings/`；组件 ID/版本由 metadata 持有，布局的 mapping 必须同时列为精确版本依赖。外观可省略，使用内置样式；推荐外观不意味着自动选择。共享资源仍经 `shared:<id>` 引用，禁止跨包路径。导入不改变活动版本。

能力仅放对应组件的 required：`keyboard.layout/1`、`keyboard.directions8/1`、`keyboard.direction-groups/1`、`input.groups/1`。根及平台公共层不要求这些能力。旧客户端继续使用独立外观，禁用未知布局/映射和依赖闭包；不得将 Group 解读为字符文本。相同包可声明多个布局，独立选择。

映射包含 `format=1`、`charset`、`modes` 与 `[groups]`，如 `left="ab"`。组为 1～32 个唯一字符，必须属于目标字符集；不执行脚本。模式为 Main、Pinyin、English、Number、Phone、Symbols。稳定的组 ID 与字母配对无关。

布局包含 `format`、`charset`、`modes`、逻辑画布 `width/height`、可选 `mapping/recommended_appearance`、必需 `fallback` 与 `[[keys]]`。按键有稳定 id、label、x/y/width/height、role 和 action。动作采用 `{kind="Character", value="a"}` 或 `{kind="Group", value="left"}`；功能仅允许 Rust `KeyAction` 枚举中的语义操作。label 改动不得修改 action。可选 landscape 使用同一布局数据模型。

方向表 `[directions]` 为布局默认，`[keys.directions]` 为按键覆盖；方向合并用 `up_left+up+up_right` 表达一个组，`disabled_directions` 显式禁止继承。完整方向解析和冲突检查由 AK-04 实现，触摸状态机由 AK-05 实现；仅解析声明不宣称设备已支持。

role 使用 key、function 等语义外观角色。`required_styles` 缺失拒绝组合；`optional_styles` 为角色声明内置回退。现有图片外观只按语义角色绘制，不是固定键位底图。固定键位图片必须另有明确的适配能力和对应布局身份/几何声明；当前不声明支持该能力，不能把固定 26 键素材直接套用十四键。组合解析、模式/字符集、功能可达性和素材一致性在 AK-18 统一检查。

共享入口 `skin::keyboard::compatibility` 在容器能力/精确依赖检查之外验证布局及映射内容，内容错误禁用相应依赖闭包，不破坏另一平台或可独立使用的外观。未知合法扩展字段保留。AK-02 的自动验证覆盖缺组能力、缺映射版本、坏映射、多布局、纯布局、既有纯皮肤/双平台、损坏 ZIP 不覆盖旧版本。
