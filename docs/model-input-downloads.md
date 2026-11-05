# MI-28 模型下载基础能力

下载由用户显式发起。Rust 下载器由配置程序/JNI 的独立后台线程调用，不进入输入会话锁，也不修改 `config.toml`。运行只需要发行包，不要求 Python。模型权重不放入安装包。

编译入程序的目录固定仓库、40 位提交版本、本地模型身份、文件长度与 SHA-256。当前仅列实验性的 Qwen2.5 0.5B Instruct Q4_K_M；它的双端兼容性已验证，独立质量评测不推荐，因此没有推荐标记。分词器内嵌于该 GGUF；程序补齐与权重绑定的原 v1 生成策略，并下载原始 Apache-2.0 许可证。不存在运行远程脚本、自动改量化、自动选模型或自动开启预测的路径。

下载源 Hugging Face / HF 镜像使用同一固定仓库版本和文件名，仅来源域名不同；后续独立 ModelScope 映射另见 MI-29。来源协议依据 [HF 固定版本下载文档](https://huggingface.co/docs/huggingface_hub/guides/download)与 [HF 镜像说明](https://hf-mirror.com/)。实现使用 Rust TLS 校验证书，支持标准代理环境变量，不关闭证书校验。

## 完整性与恢复

- 8 MiB 范围请求，逐次检查 HTTP 206 的起止与总长度；200 响应从头覆盖，错误 Range 不追加。请求连接/响应有界，取消逐读取块检查，网络阻塞时等待请求期限结束，界面应显示取消中。
- 未完成数据在模型目录的 `.downloads` 下，普通模型枚举不会显示；跨进程文件锁防止同一制品被并发写入。取消、断网保留已写字节，重试或切源以同一清单续传，最终重新核对全部文件哈希。
- 完整坏文件清空后允许重试；版本或清单不匹配拒绝续接，不把旧内容当成新模型。路径拒绝链接、越界与可执行扩展名。
- 下载前预留剩余文件、原子安装副本和 64 MiB 余量；最终通过整个模型包验证后调用现有原子导入。失败不替换已安装版本，也不改活动配置。
- `.receipts` 保留实际源、仓库版本、制品哈希及历次源/偏移。下载阶段保留续传缓存，安装阶段需要一份临时副本，因此空间检查高于单个权重大小。

## 验收（2026-11-05）

[6 项回归](validation/mi28-download-final-tests.log)通过。真实 HTTP 测试服务器覆盖中途断连、取消续传、损坏后重下、错误 Range、忽略 Range 后完整重下；空间不足使用生产检查函数注入不足值，未实际填满开发机磁盘。管理入口拒绝未评审模型身份且保存配置字节不变。

两源分别完成 491400032 字节权重、许可证和生成策略的完整校验与入库：[HF](validation/mi28-hf-download.log)、[镜像成功重试](validation/mi28-mirror-retry.log)、[逐文件同一性](validation/mi28-source-identity.json)。实际网络存在限制：HF 直连超时，使用本机既有 HTTP 代理完成；镜像直连首次失败的[原日志](validation/mi28-mirror-download.log)保留，重试经同一代理成功。代理只是传输通道，收据仍记录实际仓库来源，未使用本地权重替代下载。

[HF 取消](validation/mi28-hf-cancel.log)后同源续传并完成。另在独立目录 [HF 取消](validation/mi28-cross-hf-cancel.log)后切到[镜像继续下载并取消](validation/mi28-cross-mirror-cancel.log)，[来源记录](validation/mi28-cross-source-attempts.jsonl)显示镜像从 1052853 字节继续；此时 `model_packages list STORE` 返回空列表。该缓存保留给 MI-29 的跨源完成测试。

[HF 收据](validation/mi28-hf-receipt.json)与[镜像收据](validation/mi28-mirror-receipt.json)对应同一 `qwen25-words@q4km-v1`，下载完仍须用户选择并保存。双端完整界面链路属于 MI-30。

复测命令：

```powershell
cargo test -p model_input --lib download --locked
cargo run -p model_input --bin download-model -- STORE qwen25-words hf
cargo run -p model_input --bin download-model -- STORE qwen25-words mirror 1048576
# 省略最后的字节上限即可续传至校验完成。
```

## MI-29 ModelScope 独立映射与三源验收

ModelScope 使用独立 API 与映射字段，不通过替换 HF 域名拼接地址。此模型映射到 `Qwen/Qwen2.5-0.5B-Instruct-GGUF` 的固定提交 `4537f71740dd2c56a15ffa12e5fbb9f15095f20c`；HF 固定提交为 `9217f5db79a29953eb74d5343926648285ec7e67`。两个仓库提交标识不同，文件身份以逐个固定文件名、长度和 SHA-256 对齐。[来源元数据](validation/mi29-mapping-metadata.json)保留官方 API 公布的权重和许可信息，[官方模型页](https://modelscope.cn/models/Qwen/Qwen2.5-0.5B-Instruct-GGUF)说明来源。

下载前检查映射是否存在、版本是否固定以及映射哈希/长度是否等于本地清单；缺映射或不同量化直接报告不可用，不尝试相近文件。下载使用 `api/v1/models/REPO/repo?Revision=COMMIT&FilePath=FILE`，最终仍经过同一包完整性校验。收据使用实际 ModelScope 提交，不误记成 HF 提交。

[8 项回归](validation/mi29-tests.log)通过，覆盖缺映射、映射哈希不匹配、移动版本拒绝、源 404 保留旧部分并重试，以及原有取消、断网、坏文件和空间检查。实际 ModelScope 直连从 HF→镜像保留的 2110054 字节继续，[取消一次](validation/mi29-ms-cancel.log)后[重试完成并入库](validation/mi29-ms-download.log)。[收据](validation/mi29-ms-receipt.json)保留三源历次偏移；它是跨源续传完成包，并非宣称每一字节都重新从 ModelScope 下载。

[最终三源文件身份](validation/mi29-three-source-identity.json)与 HF、镜像分别完整下载的权重、策略及许可证逐字节哈希相同。本地身份始终为 `qwen25-words@q4km-v1`，配置未由下载器读写，未自动激活。CLI 源参数新增 `modelscope`；界面接入见 MI-30。
