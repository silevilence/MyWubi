# MI 预测准入与词边界验证（2026-11-04）

本轮完成 MI-10 的开发子项，并补充 MI-15/16 的预测回归和模型选择说明。
MI-10 整体验收仍未完成：没有独立语义质量达标的推荐模型；没有把整句续写纳入范围。

## 实现与行为

- 召回先识别前文末尾最长的已知完整词。较短的内部后缀不能重新起词，防止
  「分钟 → 钟头 → 头发」这样的尾字接龙。最多检查 16 个后缀、每个 1024 行，
  支持用户词和 Unicode scalar 边界；仍保留完整词扩展与显式下一词搭配。
- 模型原始分数不直接授予显示资格。包内哈希校验的策略文件绑定权重、词表及适配器，
  检查最佳分、前两项分差，再剔除低于绝对门槛的其它候选。单候选只检查绝对门槛。
- 响应保留完整评分身份，同时携带 `prediction_accepted`。核心检查准入身份合法、唯一，
  只展示获准项；旧响应缺少准入、空列表、非法身份、超时或失败均不显示。
  选词沿用不可变身份；迟到响应不能再次改序。
- 没有策略的旧包不损坏基础输入或离线评分，但预测返回 `Uncalibrated`；双端设置解释原因，
  不提供该包的预测选择。低置信返回 `LowConfidence`，已加载工作线程仍为 Ready。
- 无候选后普通空格/新编码正常。候选栏没有新增状态文字。

## 可复现命令

```powershell
cargo test --workspace
cargo test -p model_input --features llama-cpp-2,ort --lib
cargo clippy -p core_engine -p model_input --all-targets --features model_input/llama-cpp-2,model_input/ort -- -D warnings
cargo run -p model_input --bin prediction-replay
cargo run -p model_input --bin evaluate -- --prediction-recall
python -m unittest discover -s model_input/tools -p test_calibrate_prediction.py -v
```

Windows 后端测试使用已记录的官方 ORT DLL。Android 模型版沿用 `android/build-models.ps1`。

校准输入 JSON 包含 `adapter`、权重/词表 SHA-256，以及 `samples`。每个样本含
`id`、`group`、`split`（dev/test）、实际光标前的 `prefix`、人工标注的 `acceptable`
列表和 `candidates: [{text, score}]`；空的 acceptable 表示应当停止。
分数必须由对应适配器对同一候选和前文实际计算，目标/未来文字不能进入请求。
开发集必须同时有正例和停止例，跨划分的重复前文/来源组会被拒绝。

```powershell
python model_input/tools/calibrate_prediction.py scores.json --policy-output prediction-policy.json
python model_input/tools/package_model.py onnx model.onnx model-package --vocabulary vocabulary.json --prediction-policy prediction-policy.json --id example --version v2 --source '模型来源与版本' --license '实际许可'
# 导入前仍须用模型包校验入口验证哈希、适配器和文件边界。
```

只用 dev 选择分数门槛与分差；test 只验证冻结后的参数。工具报告召回、展示覆盖率、
首选精度、全部展示候选精度和负例误展示。默认精度 0.9、覆盖率 0.1 是可调的工程约束，
不是经设备/产品验收确定的通用门槛。无符合条件的 dev 参数或固定参数在 test 失败时不导出。
工具对齐 Rust f32 分数，并记录输入文件哈希；修改 test 标签不会改变 dev 选中的参数。
不要反复查看/调参同一 test 后仍称其独立保留集。

## 验证结果

- 330 项工作区测试通过；17 项带双后端的库测试通过；上述严格 Clippy 与格式检查通过。
- 4 项 Python 校准测试通过：负例停止、弱项过滤、测试标签隔离、数据泄漏/非有限分数拒绝，
  以及没有合格参数时不产出策略。
- 30 条双行为配置回放（15 个手写案例 × 两种配置）均符合预设的候选与停止要求。
  每种配置 10 个正例均召回至少一个标注目标，5 个停止例均无候选。
  数据由开发者撰写、已用于回归，不能作为独立质量证据。
- 8 个连续首选轨迹均在 16 步上限之前停止。例如「五 → 一 → 节」停止；
  「分钟」直接停止。仍会出现「谢谢你们好」「输入法设置为」等语义不足的组合。
  「北京市 → 场」来自词库的完整词「北京市场」；连续提交前文断言确认未丢失「北京市」。
  词边界规则不能解决语义歧义，不以停止成功冒充整句通顺。
- API 34 x64 模拟器验证词典/ONNX 预测上屏、完整词后停止、普通空格恢复、继续编码、
  外部编辑、旋转撤下旧预测、真实 ONNX 低置信隐藏、模型目录与保存删除保护。
  最终 5 项定向仪器测试全部通过（91.034 秒）。
  仪器测试使用明确标注为测试用途的宽松/拒绝策略，不给实验模型套用未经验证的产品门槛。
- Windows 独立工作进程完成 3/3 次真实 ONNX 请求；探针校验评分身份及非空准入身份。
  首次完成约 184 ms，三次请求中位数约 23 ms；这是固定双候选测试包的服务往返，
  不代表键盘延迟、真实模型性能或预测质量。
- Windows Sandbox 新注册路径 `C:\MyWubiTrialFix3`，Edge 新进程中实际输入
  `g`、空格、空格、`2` 得到「五一期间」且候选消失；再按空格、`g`、空格得到
  「五一期间 五」并开始新一轮预测。旧窗口保留用户内容，仍可能加载旧 DLL。
- Windows Release DLL SHA-256：`9db9b03ac8b53d4649e83e86ac7c1a23af301e747d16dcaa3e7d2467ae420a86`。
  Android 双 ABI 模型 APK、JVM 单测和测试 APK 构建通过；8 项运行库许可证及载荷检查通过。

原始记录：[连续选词回放](mi-prediction-admission/replay.json)、
[既有召回评测](mi-prediction-admission/recall.json)、
[Android 联测](mi-prediction-admission/android-tests.log)、
[Windows 服务请求](mi-prediction-admission/windows-service.jsonl)。
既有八条预测测试仍为召回 2/8、Top-1 1/8、Top-3 2/8，没有据此宣称语义质量提升。

首轮 Android 宿主测试出现首轮冷加载无预测和停止标记的断言失败。核心视图与连续前文
回归均通过；测试刷新无障碍缓存、允许冷加载超时后用新提交重新请求，随后复测通过，
没有以延长产品等待预算或接受迟到结果解决。首次双后端全目标构建遇到 Rust 元数据
缓存错误；随后双后端库测试与全目标严格 Clippy 通过。已有 LNK4104 导出警告仍存在。

## 仍未完成

独立中文正/负例质量集、可推荐模型及实际校准参数、完整 Windows 宿主失败矩阵、
TalkBack 实际朗读、服务重建专项、ARM64 真机与长时资源测量、正式安装器升级联合验收。
排序产品接入继续按用户要求暂缓。工程回归通过不能覆盖上述验收。
