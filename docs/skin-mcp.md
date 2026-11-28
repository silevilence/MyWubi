# 用 Agent 制作皮肤

在 Windows 设置的「候选外观」开启「允许 AI 通过 MCP 接管皮肤编辑器」。编辑器打开后，点击顶部的「复制 MCP 连接地址」，在 Agent 的 MCP 设置中选 Streamable HTTP，粘贴完整地址。连接只在本机、本次开启期间有效。已有草稿会留在原窗口。

用户可以切换查看的平台、明暗和预览场景。顶部区分「等待 Agent 接管」和「AI 接管中」。点击「关闭 MCP，恢复手动编辑」或关闭设置开关，就能继续手动编辑。已提交到草稿的修改会保留。

## 制作一份可导入的包

1. 用 `resources/list` 查找开发资料，或调用 `lookup_skin_docs`，查询「Android 空格键背景」。`mywubi://skin/android-space` 提供可用于竹纸模板的最小示例。
2. 调用 `acquire`，保存返回的 `lease`。写工具要求提供这个值。每次成功验证写入凭据时重新计时，空闲五分钟后须再次接管。
3. 用 `inspect` 查看当前草稿。如果没有草稿，使用 `create_skin` 并指定 `template: "bamboo"`，或用 `open_skin` 打开本地包。使用 `set_identity` 指定作品的 ID、名称和版本。
4. 对同一目标调用 `preview`、`edit_appearance`、`preview`。例如选择 `platform: "android"`、`component: "jade"`、`variant: "dark"`，用以下参数为深色空格键绑定模板中的素材。

```json
{"images":{"space":{"resource":"shared:bamboo","mode":"stretch","opacity":0.8}}}
```

5. `preview` 的 `scenes` 可以传入 `[{"state":"ordinary"},{"state":"bubble","landscape":true,"scale":2}]` 查看 Android 键盘示意图。Windows 原生候选支持 `ordinary`、`long`、`disabled`，`scale: 2` 对应 192 DPI。每张 PNG 附带渲染来源。
6. 调用 `validate`，然后用 `export` 指定新的本地 `.mywubi-skin` 绝对路径。结束时 `release` 交还 Agent 写入权。用户关闭 MCP 后，可以在编辑器中选择「应用到 Windows」。

## 在真实宿主验证同一个文件

用 `test` 传入 `lease`、导出文件的 `path` 和 `platform`（`windows`、`android` 或 `both`）。Android 另传测试设备的 adb `serial`。可分别指定 `windowsComponent`、`androidComponent`，省略时选择对应平台的第一个外观组件。

- Windows 需要可启动的 Windows Sandbox 和编辑器同目录的 `im_engine.dll`。测试在 Sandbox 中注册输入法并启动 WPF 文本框。
- Android 需要已解锁的测试设备或模拟器，桌面能调用 adb，设备上已安装 MyWubi 调试版、包含 `McpSkinHostTest` 的测试 APK，以及独立的验收宿主。测试临时切换该设备的输入法与配置，退出时恢复。设备准备见 [Android 验收宿主说明](../android/acceptance-host/README.md)。
- 用返回的 `job` 调用 `test_status`。两端均用固定码表输入 `ab`，选择「以及」，核对外部文本框中的文字、焦点和选区。报告按平台列出运行中、通过、失败或未执行，并给出文件哈希、证据目录和原生截图。
