# Windows 按需推理组件

应用、推理组件、模型权重是三个独立交付对象。统一应用包仅提供基础输入、词典预测和组件管理；预测默认关闭。Android 继续由 APK 交付原生库。

组件清单由发布构建通过 `MYWUBI_COMPONENT_CATALOG` 编译进应用。普通开发构建为空清单，不提供未经发布审核的下载。模型清单不能选择组件来源、可执行文件或运行库；已有模型包白名单继续拒绝 DLL/EXE。

每个组件记录版本、应用 semver 范围、系统/架构、能力、精确依赖、官方 Release 来源、许可证、压缩包大小/哈希、展开大小和逐文件大小/哈希。仅接受该应用版本官方 Release 下的固定制品。下载和启动均验证完整性，不能从模型目录或 PATH 选择服务。升级应用会重新检查清单兼容性。

首批 `cpu` 包同时提供 `llama`（GGUF）和 `onnx`（ONNX）能力，共用服务与 app-local MSVC 依赖，避免为两个后端重复安装 CRT。清单还支持依赖图并去重、拒绝缺失依赖和循环。GPU 包可用相同协议表达独立能力，不代表 GPU 设备验收已通过。

组件保存在配置旁的 `components/<id>--<version>/`；模型仍在原 `models/` 下。安装包和便携目录都使用同一个定位规则。暂存下载不会成为可启动组件，版本更新不覆盖旧目录。关闭预测不删除文件。

旧模型版程序旁的 DLL/EXE 不会被自动信任或搬入组件目录。应用升级保留配置、模型及词库；用户从组件管理重新获取经过当前应用清单授权的组件。旧文件可由旧安装器管理，不作为新版本可用性的依据。

## 构建与升级

Windows 发布需要 Python 3.12+、Rust/MSVC、Velopack 及 ONNX Runtime 1.31.1 x64。运行 `./build.ps1 -Version 0.7.0 -OnnxRuntime <onnxruntime.dll> -OutputDirectory <新输出目录>`，先生成 CPU ZIP 和 `components.json`，再把该清单编译进轻量应用。CI 从固定版本 Python wheel 取得 ONNX Runtime，最终资产共同发布。相同版本输出目录已有 nupkg 时拒绝重打包，避免混合两次构建。

分步构建可先运行 `scripts/build_components.ps1`，再将同一输出目录中的 `components.json` 传给 `build.ps1 -ComponentCatalog`。启动检查会核对应用内清单与独立 ZIP 的大小及哈希；组件打包检查两后端特性、运行库初始化、PE 架构、许可和必需依赖。普通开发构建不会自动获得发布下载源。

标准升级保留 AppData 配置、模型和词库；便携覆盖保留已有用户文件。旧模型版遗留的同目录服务不会启动，新协议管道包含应用版本，也不会误连旧服务。只有当前编译清单明确授权且兼容、全部文件校验通过的组件可复用。清单按顺序表达版本偏好：更新缺失或损坏时，可回退到清单内仍受信且兼容的已安装版本，不能从安装收据扩展信任。首批清单采用精确应用版本，跨版本默认需要新组件。

体积测量运行 `python scripts/measure_component_distribution.py --application target/velopack-staging --release <发布输出目录> --output <测量目录> --model-bytes <模型下载字节数>`。脚本固定相同二进制与压缩方法，重建旧版布局用于对照；全量更新预算不等于实测增量更新量。实测与限制见 [MI-36～MI-39 验收记录](validation/mi36-39-review.md)。
