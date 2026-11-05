# MI-27 双端预测配置

沿用共享 `PredictionConfig`，未增加平行格式。默认配置和旧配置缺失字段时仍关闭预测；`model = None` 明确表示词典预测，选择模型后不会因缺文件、下载失败或缺后端自动改回词典。关闭开关保留选择和预算，不自动下载。

两端提供总开关、词典/模型选择、50～1000 ms 等待预算、CPU 线程、候选上限与加载策略。Windows 保持模式不因空闲卸载；Android 提前加载受输入法前台生命周期、低内存和电量限制，不承诺后台常驻。保持模式下空闲卸载滑块禁用。GPU 尚未实现，因此不提供伪生效选项。

Windows 设置从相邻工作器的 `--features` 查询实际编译能力，ONNX 额外校验运行库可加载；不再要求 GGUF 必须同时有 ONNX DLL。探测运行于后台，不初始化权重、不启动常驻服务。缺组件提供发行包入口；缺模型提供导入入口及保留原选择的诊断。下载源与双端管理入口见 [MI-28～MI-30 下载说明](model-input-download-ui.md)。

设置页面展开时每两秒读取实际服务状态，区分未启动、空闲、加载/预热、就绪、暂停及失败；保存状态与运行状态分别呈现。Windows 创建工作器失败时仍准确报告配置已保存，同时展示组件修复说明。Android 保留原有热重载通知和保存草稿机制。

验收（2026-11-05）：

- [共享配置与会话](validation/mi27-core-tests.log)：配置 4 项、预测会话 18 项通过，新增旧配置默认关闭、全字段保存重读、缺模型身份保留及 LegacyV1/TouchV1 热切换保留待输入编码。
- [Windows 设置真实保存入口](validation/mi27-settings-test.log)：1 项通过，重新构造设置状态读取同一文件，未下载、未安装模型，选择/开关/预算无隐式变化。
- [Android 设置设备测试](validation/mi27-android-settings-tests.log)：API 34 x86_64 上 2 项通过，包含保存、销毁并重新创建设置、无模型、缺失身份和关闭后保留选择。[构建与单元测试](validation/mi27-android-build.log)通过；本轮 APK 使用已有模型版 JNI，改动的 Kotlin 和设备测试重新构建。共享 Rust 状态机另有上述测试，JNI Features 响应结构保持兼容。
- [真实组件探测](validation/mi27-gguf-without-ort.json)：最终 feature 工作器复制到没有 ONNX DLL 的独立目录后返回 llama=true / onnx=false。

关闭/重建和实际权重释放沿用已通过的 [MI-24 / MI-25 生命周期验证](model-input-preload.md)。本项没有修改推理策略或扩大 1000 ms 预算；不宣称模型质量通过。
