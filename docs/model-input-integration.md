# 字词预测的模型接入与可选制品

2026-11-04。本轮优先完成上屏后的字词预测；智能排序的三种产品模式按用户指示暂缓。
模型增强默认关闭，没有通过独立质量验收的推荐模型。本文的运行成功不等于预测效果达标。

## 使用与配置

不装模型也可以开启“上屏后显示字词预测”，使用内置词典续接和常用搭配。选择建议只追加
一个字词或剩余片段；继续输入编码立即撤下预测，空格和回车不会自动提交预测。

两端设置里的“预测模型与资源”支持目录导入、完整性校验、选择、移除未选模型，以及来源、
许可、大小和任务能力查看。保存后才生效。Windows 从文件选择器导入目录，Android 使用
系统目录选择器，不申请整个共享存储的访问权。模型包格式和导出样例见
[后端规范](model-input-backends.md)。只支持现有 GGUF/ONNX 适配器，不执行包内脚本或原生插件。

```toml
[prediction]
enabled = false
wait_ms = 200
threads = 2
max_candidates = 16
idle_seconds = 60

# 不配置 model 时使用词典；选择的模型必须已导入本设备。
# [prediction.model]
# id = "authored-counts"
# version = "v1"
```

预算范围：等待 50–1000ms、CPU 线程 1–4、候选 1–64、空闲卸载 10–600 秒。
上下文最多 256 字，单个建议最多 16 字。旧配置保持默认关闭。配置仅保存模型 ID 和版本，
不将设备路径作为跨设备资源地址；实际包存于统一配置目录下的 `models/id--version`。
Windows AppData 配置不随版本安装目录变化；Android 使用应用私有 files 目录。
卸载应用、清除应用数据或删除便携目录不在“升级保留”的承诺范围内。

“已保存”不表示模型已加载。两端可单独刷新运行状态；基础版没有推理运行库时明确提示，
Android 禁止选择未包含的后端。模型缺失、加载失败、异常分数、超时或资源退让时，
本轮模型预测静默隐藏，普通编码输入继续工作；可切回“仅词典预测”。
等待期间不显示可误点的隐藏候选，发布后不再接受迟到重排。预测文本只有确认上屏后才加入前文。

## 调度与隔离

核心只生成带会话、编辑器、查询、上下文及配置版本的评分请求，保存一次性截止时间。
平台控制器在后台排队和轮询。新输入、切框、换模型、重载或关闭会作废旧结果；鼠标/触摸
提交的是候选身份和原视图版本。测试覆盖结果乱序、等待中输入、超时、失败与旧点击。

模型工作线程只有一个执行槽和一个可替换待执行槽。重复/连续输入不积累无界队列，模型和
词表可复用，暂停、禁用及空闲会卸载。切换只允许一个替代模型加载，成功后释放旧实例；
失败保留旧实例，恢复原选择即可复用，但不会用旧模型冒充新选择来显示预测。切换峰值可能
同时包含新旧两个模型，真机内存预算仍需验证。快速关闭/重开复用一个休眠工作线程，
避免不可取消的冷加载并发堆积。取消计算与结果作废是两件事：后端检查取消点；冷加载若
不能立即中断，输入线程不等待，完成后丢弃旧任务。模型加载、哈希校验、推理和析构都不持有 UI 锁。

Windows 的 `im_engine.dll` 只链接轻量 IPC。`model-worker.exe` 按需启动，包含 CPU 后端，
设置窗口退出不影响它。命名管道为当前用户专用 DACL、拒绝远程客户端且只允许首个实例，
消息最多 64KiB。由内核获取客户端 PID，隔离不同注入进程的会话编号，切换所有者重置状态。
后台传输每次有截止时间；服务退出后下一次有效请求尝试重启。60 秒无 IPC 自动退出。
受限宿主连不上服务时不改变权限，而是隐藏该轮模型预测。DLL 卸载前等待其有界 IPC 线程退出。

Android JNI 的管理端点不持有核心会话注册表锁执行推理。主线程只投递/轮询小消息；IME 隐藏、
连接失效和内存回调会取消/暂停。系统省电模式、严重热状态、低内存，或未充电且电量不超过
15% 时暂停；重新允许时仅处理新请求。状态读取最多每秒一次。ARM64 真机热/电量/长时稳定性仍需验收。

## 构建与交付

基础版沿用原构建，不强制携带模型、llama.cpp 或 ONNX Runtime。可选模型版：

```powershell
# Windows x64；构建机提供官方 ONNX Runtime 1.31.1 的 x64 DLL。
./build.ps1 -WithModels -OnnxRuntime C:/runtime/onnxruntime.dll
# 同版本的独立验证包可指定输出目录，保留已有发布制品。
# ./build.ps1 -WithModels -OnnxRuntime C:/runtime/onnxruntime.dll -OutputDirectory target/model-validation

# Android arm64-v8a + x86_64；固定 NDK 27.2.12479018、Rust 1.99.1、JDK 21。
./android/build-models.ps1
python android/check_apk.py android/app/build/outputs/apk/debug/app-debug.apk
python scripts/check_model_artifacts.py --apk android/app/build/outputs/apk/debug/app-debug.apk
```

Android 模型版使用官方 `onnxruntime-android:1.31.1` AAR，提取原生库供 Rust 动态加载；
llama.cpp 静态链接 C++ 运行库。Windows 将两个后端、ORT DLL 和 Visual Studio x64
可再分发 CRT 放在独立服务旁；采用微软支持的[应用本地部署](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files)，无需用户另装开发环境。
`model-native-runtime.json` 记录实际 DLL 版本与哈希，CRT 遵循微软分发条款。包中不收集
用户模型。所有运行库的版本、许可证及通知文件哈希见 `model_input/licenses/`。
`.github/workflows/models.yml` 检查可选制品；它不发布模型，也不把交叉编译当作真机运行。

开发用真实服务探针只发送自编的“北京 → 天气/大学”两个候选，不读取宿主输入：

```powershell
Start-Process ./target/release/model-worker.exe -WindowStyle Hidden
./target/release/probe-service.exe C:/model-store authored-counts v1 20
./target/release/probe-service.exe C:/model-store qwen25-05b q4km-v1 20
```

探针验证身份、有限分数、暂停和 IPC 截止时间，可报告往返分位延迟；这不是按键延迟、
代表性预测质量或耗电测量。真实 GGUF 需自行按已记录来源与许可准备，不随 APK 分发。

## 验收边界

已实现的产品路径与尚未通过的验收分别记录在 [逐项状态](model-input-status.md)。
词典预测真实宿主证据见 [预测验证](model-input-prediction.md)。统计 ONNX 图仅用于功能和
接口验证；现有 GGUF 尚无足够净收益证明。独立中文测试集、置信门槛、代表性设备预算、
ARM64 真机长测、完整 Windows 宿主矩阵及正式安装器升级仍保留未完成。
