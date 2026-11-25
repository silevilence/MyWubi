# 分平台正式发布

每个全局稳定标签 Vx.y.z 对应一个 Git 提交。公开后的标签、说明绑定的范围和制品均保留。补发使用更高的新标签。第一次启用本协议必须同时交付两端的新版检查器。

发布时上传一个 `release-scope-{windows|android|both}.json`。文件名供客户端在发布列表内筛选。正文包含 schema、tag、commit、scope、run_id、platforms 和 assets。assets 记录各制品的大小及 SHA-256，供公开后与平台上传元数据比对。platforms 按 windows/android 分别保存最近实际交付的 tag 和 commit，Android 同时保存 versionCode。选中的平台更新记录，其他平台继承上次交付。CI 通过已发布标签和实际资产追溯旧版基线，比较 Android 上次正式清单的 versionCode。Rust workspace 记录仓库构建版本；发布范围包含 Android 时同步 APK 版本。

必需资产名称由 `scripts/release_contract.py` 的 required 生成，Windows 包含 x64 安装、便携、完整 nupkg、Velopack feed、受信组件目录及其全部归档。Android 包含同签名标准和完整版 APK、各自更新清单与原生库报告。SHA256SUMS.txt 枚举本次全部实际交付文件及范围记录，自身除外。

正式客户端跳过草稿、预发布和明确仅交付另一端的条目。旧条目通过已有资产识别；新条目声明本平台却缺少必需清单时报错。只有完整检查结束才报告无适用更新，服务错误或读取中断应提示重试。下载绑定检查得到的具体版本和资产。
