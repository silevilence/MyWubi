# MI-30 双端模型下载与管理

Windows 设置和 Android 设置共用固定模型目录与 Rust 下载管理器，提供 Hugging Face、HF 镜像和 ModelScope 选择。列表显示文件体积、估算内存、后端/能力、来源、许可与实际验证状态。当前 Qwen2.5 条目仍标为实验模型，不以兼容性验收代替质量推荐。

下载、校验和包扫描在后台执行。界面区分进行中、取消中、失败、可续传和已校验；完成后刷新可选模型列表，不改变总开关、活动模型或预算。缺后端时禁用下载入口并提供兼容发行包链接。已选模型不能移除；未选模型的移除会一并清理其下载缓存与收据。

验收中修正了三个实际问题：Windows 展开资源面板后保存按钮移出窗口，现改为固定底栏与中央滚动区；Android 只读状态查询曾触发退出草稿确认，现只对真实未保存更改确认；完整下载曾复制出第二份权重，现校验后原子移动到已安装目录，避免约 469 MiB 的重复占用。

Windows Sandbox 已从关闭预测、无模型开始，通过 ModelScope 实际下载；中途取消、关闭设置、重新打开并续传，最终完成哈希校验。下载后开关仍关闭，随后手动选择、开启、保存并观察就绪。关闭设置后，真实记事本完成「北京→市」空格上屏；发布版设置重新打开仍保留模型、开关与就绪状态。

Windows 证据：[取消下载](validation/mi30-windows-cancel.png)、[下载完成且总开关关闭](validation/mi30-windows-downloaded-off.png)、[就绪](validation/mi30-windows-ready.png)、[生成候选](validation/mi30-windows-generated-candidate.png)、[真实上屏](validation/mi30-windows-generated-commit.png)、[重新打开设置](validation/mi30-windows-reopened.png)。

Android 14 / API 34 x86_64 模拟器也从关闭预测、无选中模型开始，通过 ModelScope 完整下载并校验。安装后无第二份完整下载缓存，配置仍关闭且无模型选择；手动选择、提前加载、开启和保存后，关闭重开仍保持一致。实际输入框完成「北京→市→北京市」。

最终 [230.335 秒设备验收日志](validation/mi30-android-network.log)通过；[下载收据](validation/mi30-android-download-receipt.json)、[下载就绪截图](validation/mi30-android-download-ready.png)、[保存配置](validation/mi30-android-saved-prediction.json)、[加载状态](validation/mi30-android-ready-phases.json)、[真实上屏](validation/mi30-android-downloaded-prediction.png)均已保存。

预测性能限制如实保留：[第一次完整流程](validation/mi30-android-first-prediction-miss.log)未得到「市」；最终流程的[逐次记录](validation/mi30-android-prediction-attempts.json)显示第一次独立提交没有候选，第二次独立提交成功。验收最多允许三次重新输入，每次都重新确认上屏并创建新请求，不延长 1000 ms 预算、不接纳旧请求迟到响应。这是功能验收，不是首次响应率或模型质量达标结论。

验收脚本等待异步包校验完成、滚动停止，并刷新 Compose 无障碍节点；不再因加载页底部或陈旧节点误判功能缺失。模拟器测试与 Windows Sandbox 串行执行。实测不替代 Android ARM 真机性能验收。
