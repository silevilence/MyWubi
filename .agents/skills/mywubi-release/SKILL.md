---
name: mywubi-release
description: 为 MyWubi 准备 Windows、Android 或双端正式版本，按平台基线起草变更说明，经用户确认后发布并核验 GitHub Release。
---

# MyWubi 正式发布

1. 读取默认分支和所有正式 Release 元数据。用 `gh api --paginate --slurp 'repos/{owner}/{repo}/releases?per_page=100'` 或等效连接器获取全部页。用 `scripts/release_contract.py` 的 baselines 找出两端最近交付标签，解析为实际 Git 提交。旧格式按已有资产识别。列出本地领先的提交，分别检查两条基线到目标源码的 diff、依赖、资源和构建产物。共享代码变更要追踪两端实际调用，历史单端积压的改动应计入对应平台。
2. 复用 generate-changelog 的写法，先在对话中给用户可读的草稿。推荐 windows、android 或 both，解释涉及的平台行为和累计变化。首次切换须双端发布过渡版。列出拟用全局标签、各端基线、目标源码提交、拟进行的版本和说明修改。请用户最终确认版本、范围、说明和发布授权。用户改选时先裁剪说明，未选平台的改动留到下次。用户要求演练时使用本地样例。
3. 确认后定稿 CHANGELOG，同步 workspace、crate 和本地依赖版本。包含 Android 时同步 versionName，并使 versionCode 高于上次正式 Android 清单。用项目 Git 规范提交发布准备，核对该提交相比已确认源码只包含约定的版本和说明修改。版本、源码或平台实质变化时重新确认。给核验过的提交打唯一标签。先推送已确认的默认分支并核对远端手动工作流已生效，再推送指定标签；旧版工作流推标签即发布。
4. 优先用能准确返回运行 ID 的连接器。用 gh 时生成 UUID，执行 `gh workflow run release.yml --ref <默认分支> -f tag=<标签> -f scope=<平台> -f request_id=<UUID>`。从返回链接，或从 `gh run list --workflow release.yml --event workflow_dispatch --json databaseId,displayTitle,url` 精确匹配 UUID 获取 ID。等待运行可见时只轮询，重复 dispatch 会创建另一运行。跟踪 `gh run watch <ID> --exit-status`。失败时定位具体任务日志，检查标签、公开状态、草稿身份。暂时性故障用 `gh run rerun <ID> --failed` 恢复原运行。遇到不同运行的草稿、同名不同内容或已公开版本时停下说明。需改源码时重新定版本并确认。
5. 用下面的命令核对远端标签、提交、正式状态、说明、范围、运行身份与资产清单。结合选中 CI 任务的上传记录查看命名、大小和摘要是否合理。返回 Release 与运行链接、两端最近交付版本及异常项。保留旧标签及历史组件。发布后仅读取 API 元数据与小型范围 JSON，安装和覆盖升级测试安排在发布前。

```sh
python scripts/verify_release.py <标签> <windows|android|both> <提交SHA> <运行ID> <发布说明文件>
```
