# MI-31 自定义模型接入

双端「预测模型与资源」提供离线接入指南和显式试运行入口。指南正文由同一个[内嵌文件](../model_input/catalog/custom-model-guide.md)提供，说明 GGUF 架构、ONNX 字符递归图、分词器、包清单与准入参数。目录下载自动补齐清单和策略；只有开发者准备新模型包需要运行包装工具。

试运行模型选择独立于已保存的活动模型，默认前文为「北京」，只能在点击按钮后执行。Windows 使用一次性工作器进程；Android 使用独立模型实例，并限制同时只有一个试运行。两端均不读取当前编辑器正文，不修改预测开关或活动模型，不把样例成功标为质量达标。

兼容性检查先验证路径、文件身份和所需分词器/策略，再检查 GGUF 架构；元数据读取、字符串和数组均有界。导入和试运行复用同一诊断。原有离线评测工具仍可管理仅评分、尚未校准的包；双端预测入口要求对应策略完整。

试运行分别展示加载/预热与推理耗时，预测预算仍为 1000 ms。空候选表示无召回或未过准入，不伪造成功候选。失败保持原配置和基础输入会话。

从本地文件复现：按内嵌指南将已取得的 Qwen2.5-0.5B-Instruct Q4_K_M 包装为 `local-qwen@v1`，导入包目录，再选择该包试运行「北京」。其权重和策略必须匹配指南中的固定 SHA-256；没有任意 GGUF 即插即用的承诺。

Windows Sandbox 已按指南导入 `local-qwen@v1`，打开离线指南并在发布版设置中独立试运行「北京」，得到「市」；加载与预热 9255 ms，推理 810 ms。修改测试前文后旧结果清空，已保存配置 SHA-256 前后完全一致。

证据：[离线指南](validation/mi31-windows-guide.png)、[试运行结果](validation/mi31-windows-final-trial.png)、[修改样例后清空结果](validation/mi31-windows-result-cleared.png)、[配置未改变](validation/mi31-windows-config-preserved.json)、[实际产物哈希](validation/mi31-windows-artifacts.json)。同一本地包的独立 Rust 导入/试运行回归通过，推理 380 ms，见[日志](validation/mi31-real-local-idle.log)。

Android 14 / API 34 x86_64 模拟器完成离线指南、模型独立选择、显式样例试运行与修改前文清空结果；「北京→市」加载/预热 6946 ms，推理 798 ms，保存配置未改变。坏包、不支持架构、缺分词器、缺预测策略的导入/试运行诊断逐字一致，既有输入会话仍可继续上屏。连同设置保存、重建及草稿回归，共 4 项设备测试通过。

证据：[设备测试](validation/mi31-android-tests.log)、[试运行截图](validation/mi31-android-trial.png)、[耗时与结果](validation/mi31-android-trial-result.txt)。验收中移除了试运行期间持续刷新的进度动画，改用静态状态提示，避免软件渲染与 CPU 推理竞争；自动化也在后台完成后再滚动读取结果。

本项只验证兼容性，不改变既有模型质量结论；Android 实测范围为 x86_64 模拟器，不替代 ARM 真机性能验收。
