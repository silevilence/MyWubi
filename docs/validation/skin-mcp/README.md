# 皮肤编辑器 MCP 验收 · 2026-11-28

本次用「竹纸」模板制作了 [深色空格键示例包](agent.bamboo.space.mywubi-skin)。Windows Sandbox 和 Android 14 模拟器分别加载了同一份导出文件。包 ID 为 `agent.bamboo.space`，版本为 `1`，SHA-256 为 `132bbdf69574fe9c15e6c4c95068f96c7a8455ea4145af2293aad38499045ad9`。

## 草稿与预览

| 检查 | 结果 |
| --- | --- |
| 现有窗口接管 | 设置开关连接到当前草稿，编辑器显示等待状态。只读期间按 Ctrl+S 不会开始导出。顶部按钮和设置开关均可恢复人工编辑。 |
| 多个 Agent | HTTP 客户端能初始化、读取资料、接管、修改、预览并交还。另一个写入者被拒绝。超时和关闭后的旧凭据失效。 |
| 定向修改 | 对 Android 深色空格键引用缺失图片时，整批修改被拒绝。修正资源后能导出并重新打开。 |
| 看图返工 | 初稿使用完全不透明的竹纸背景，浅色键名难以辨认。将不透明度调到 20% 后导出。自动比较确认，Android 前后两图的差异都在空格键内，Windows 同场景前后逐像素一致。 |

[Android 修改前](android-before.png) · [第一次修改](android-first.png) · [最后一次修改](android-after.png) · [左右对照](android-comparison.png)

[Windows 修改前](windows-before.png) · [Windows 修改后](windows-after.png)

## 真实输入验收

两端均在独立宿主的文本框里输入 `ab`，选中候选词「以及」。断言读取宿主文本、键盘焦点和选区起止位置。截图记录候选或键盘可见时的画面。

| 环境 | 结果 | 证据 |
| --- | --- | --- |
| Windows Sandbox，WPF 文本框 | 通过。宿主进程加载了指定的皮肤和组件 `jade`，提交文字为「以及」，焦点在文本框，选区为 `[2, 2]`。 | [宿主截图](windows-host.png) · [断言](windows-host.json) · [皮肤加载记录](windows-loaded-skin.txt) |
| Android 14，Pixel 5 API 34 模拟器，外部 EditText | 通过。测试载入同一哈希的皮肤包，提交文字为「以及」，焦点在输入框，选区为 `[2, 2]`。 | [宿主截图](android-host.png) · [断言](android-host.json) |
| 同时测试两端，指定离线 Android 设备 | Android 返回 `not_run` 和原因。同次运行的 Windows 检查通过。 | [离线报告](android-offline.json) |

Windows 首轮已上屏，读取选区时遇到 UI Automation 类型名错误。Android 首轮在读取 adb 暂存文件时遇到应用存储权限限制。修正 Windows 选区读取方式和 Android 测试文件交接后，两端复测通过。Android 恢复原输入法，清除了本次设备暂存文件；随后关闭测试模拟器和 Windows 沙盒。

## 复现

1. 按[使用说明](../../skin-mcp.md)开启 MCP 并接管草稿，或直接使用本页的示例包。复用本地 DLL、调试 APK、测试 APK 和宿主 APK。
2. 调用 `test`，传入接管凭据、示例包的绝对路径、`platform: "both"` 和指定设备的 adb 序列号。用 `test_status` 读取报告及截图路径。验证离线场景时传入不存在的设备序列号。
3. Rust 回归使用 `cargo test --offline --locked --workspace --target x86_64-pc-windows-msvc -j4`。单元及集成测试共 651 项通过，9 项按测试定义忽略。编辑器库专项复测 47 项通过、2 项忽略。指定宿主的测试 `skin_mcp::tests::run_designated_host_acceptance` 已另行显式运行，见上表。
4. 首轮合并运行进入文档测试时，rustdoc 报告加载 `text_render` 依赖失败。单独执行 `cargo test --offline --locked --workspace --doc --target x86_64-pc-windows-msvc -j4` 通过了全部 10 个文档测试目标。随后单独运行第 3 步的整条命令，单元、集成及文档测试全部通过。桌面调试产物使用离线构建，`settings`、`text_render`、`core_engine` 的 Clippy 检查使用 `--all-targets -- -D warnings`。
