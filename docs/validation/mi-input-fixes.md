# 字词预测试用反馈修复（2026-11-04）

用户在 Windows Sandbox 的 Edge 和经典记事本中报告：状态词混进候选、
空格/数字无法选择预测，以及记事本中文显示为方块。

## 原因与修复

1. 预测召回把“续接/联想”写进了编码提示字段，Windows 又把“字词预测”和
   临时模式名拼进编码行。已移除这些文字；反查只显示真实编码，不再加“主码:”前缀。
   临时模式菜单中不可用的项用灰色表示，错误原因不再跟在选项后。
   Android 预测、安全输入、加载和错误状态改用图标/进度标记；读屏描述仍保留。
   设置说明和实际操作按钮的名称保留。
2. 空格原先被有意当成空输入处理；数字键实际进入 `Char('1'..'9')` 路径，预测
   只处理 `Select`，因此漏掉了实体键盘。共享会话现在统一处理已显示预测的空格、
   数字和显式首选操作，按当前显示顺序选中候选。隐藏的推理结果不能抢占输入，
   新编码仍撤下预测，回车仍交给宿主，显式字面输入不被重新解释。
3. 沙盒中复制的经典记事本默认使用 Lucida Console、西欧字符集；中文字体已安装，
   但该字体的中文回退不可用。保持原文本不变，改用 Microsoft YaHei UI 后原来的
   方块显示为“工”。修复了该沙盒的记事本字体，并同步本地试用启动脚本；
   输入法代码不修改宿主应用字体或操作系统字体设置。

## 验证

`cargo test -p core_engine --test prediction_session` 先复现两个失败：
`prediction_candidates_contain_words_without_status_labels` 与
`visible_predictions_accept_space_and_number_keys_without_literal_characters`。
修复后，`cargo test -p core_engine -p im_engine` 共 280 项通过，包含 11 项预测
集成测试和真实 Windows 适配器的按键预判→提交回归。

核心严格 Clippy、workspace fmt、Windows Release DLL/设置程序构建通过。
Windows crate 的严格 Clippy 仍被既有 17 项诊断阻挡，涉及 COM 指针接口、
旧渲染循环和 key_filter 测试等；没有压制诊断或将该检查记为通过。

Android 双 ABI 模型版、JVM 单测与测试 APK 构建通过；原生运行库和 8 份许可证
载荷检查通过。API 34 x64 模拟器的词典预测、ONNX 模型预测（均含空格选词），
以及 Chrome 敏感输入/下一项回归共 3 项通过，264.092 秒。
原始输出见 [Android 回归日志](mi-input-fixes-android.log)。

在现有沙盒中新建 `C:\MyWubiTrialFix1` 注册修复 DLL，保留用户原窗口和文字，
另启记事本与独立 Edge 测试窗口。两者均实际执行：

1. `g`、空格：上屏“五”，显示纯词语预测候选。
2. 再按空格：选中第一项“一”，正文成为“五一”。
3. 按 `2`：选中第二项“期间”，正文成为“五一期间”，没有空格或数字泄漏。

记事本中文正常显示；候选区不出现预测类型或模式说明文字。宿主机输入法注册未改动。
沙盒 DLL SHA-256：`8f862203793e9170afe3e09558878ce3aef9c7f77375a8e6d44465338cab093b`。
本轮不重做独立模型质量或 ARM64 真机验收，也不改变其未完成状态。
