# MI-32～MI-35 原始证据

结论与未验项见 [验收报告](../mi32-35-review.md)。目录内仅有固定公开测试文本，
没有真实用户前文或模型权重。

| 文件组 | 用途与边界 |
| --- | --- |
| `environment.json`、`devices.json` | MI-32 初始环境/制品与 Intel Vulkan 设备发现；其中 worker 哈希属于初始构建 |
| `model-manifest.json`、`generation-policy.json` | 所有实际模型对比使用的包身份、权重/策略哈希与候选规则 |
| `*-native.jsonl`、`vulkan-first-driver-run.jsonl` | 原生加载、预热、预测、drop；首轮慢结果保留 |
| `vulkan-lifecycle.jsonl` | 服务加载、超时、释放与恢复；当时有后台构建，不作为稳定性能结论 |
| `missing-device.jsonl`、`cuda-unavailable.jsonl` | 缺指定设备/缺 CUDA 的 CPU 回退，不是 NVIDIA 实测 |
| `*-pipeline.*` | MI-35 中间构建的探索记录，含 Vulkan 超时；不并入最终构建统计 |
| `cpu-final.*`、`vulkan-final.*` | 最终同一构建，各 60 次，预加载后短测 |
| `*-cold-final.*` | 最终构建按需加载，各 6 次，保留首次请求超时 |
| `auto-final.*` | 最终构建自动模式短基准后保留 CPU，原因在 prepared.execution |
| `vulkan-stability.*` | 最终构建、10 秒空闲后、至少 1800 秒连续请求；只有 closed 事件且无进程错误才能算完整运行 |
| `cpu-residency.*` | GPU 长测后的 CPU 空闲/连续短采样，含一次热态超时；不与更早短测合并 |
| `final-summary.json` | 所有最终测量组的自动汇总，含逐前文与长测前后五分钟分位 |
| `vulkan-component.json` | 最终可选组件的文件、依赖与许可清单；未包含模型权重 |
| `checks.json` | 构建、测试和 Clippy 的日志摘要与哈希；忽略项不算通过 |
| `power-availability.json` | 电池读数与温度接口拒绝访问；不是耗电/温度测量 |

同一前缀的 `.jsonl` 为逐次请求，`.resources.jsonl` 为约两秒一次的工作进程资源，
`.meta.json` 为该次实际二进制哈希和测量配置，`.stderr` 保存标准错误（空文件即
无输出）。`visible` 是共享 Session 候选可发布，不包括 TSF 宿主和 GDI 绘制。
Windows 路径、PID 与会话版本仅用于追溯，不能当作候选质量差异。

通过 `model_input/tools/report_device_comparison.py` 从完整运行生成汇总；其分位
只针对首个请求之后的有效发布，同时报告所有热态请求的超时率和展示比例。
资源窗口基于进程启动附近的两个时钟，边缘排除一秒并标明 approximate，不称为
严格同步采样；缺失计数器与功耗/温度均保留 null。
