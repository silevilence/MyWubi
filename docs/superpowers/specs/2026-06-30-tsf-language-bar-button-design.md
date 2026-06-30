# TSF 语言栏按钮设计

## 目标

为 `windows/im_engine` 增加中英文状态按钮：

- 注册为 `ITfLangBarItemButton`
- 显示运行时绘制的“中”或“英”图标
- 点击时切换输入模式
- 与 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` 双向同步
- 激活时注册，停用时完整清理

不增加图片资源、依赖或新的进程间通信。

## 结构

现有 `TextService` 直接实现语言栏按钮、语言栏 item source 和
compartment event sink 接口。这样按钮点击可以直接复用
`toggle_ime_mode()`，不引入独立 COM 对象及其回调生命周期。

新增状态只保存 TSF 要求的最小生命周期数据：

- 语言栏 item sink 与 cookie
- compartment event sink cookie
- 按钮是否显示

## 生命周期

`ActivateEx`：

1. 获取 `ITfLangBarItemMgr`，将当前 `TextService` 注册为按钮。
2. 获取 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE`。
3. 订阅该 compartment 的 `ITfCompartmentEventSink`。
4. 读取 compartment 初值并同步 `ime_mode`。

`Deactivate` 按相反顺序取消 compartment sink、移除语言栏 item，并清空
item sink 引用。任何单步失败只记录日志，不阻止其他清理步骤。

## 状态流

按钮点击调用 `toggle_ime_mode()`。该方法成为模式切换的单一入口：

1. 翻转 `ime_mode`。
2. 写入 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE`。
3. 切到英文时重置状态机、结束 composition、隐藏候选框。
4. 通知语言栏 sink 更新图标、文字和提示。

外部组件改变 compartment 时，`OnChange` 读取新值并执行同样的本地清理
与语言栏刷新，但不再次写入 compartment，避免通知回环。

## 显示

`GetIcon` 使用 Win32 GDI 在内存位图上绘制 16×16 的“中”或“英”，再生成
调用方负责释放的 `HICON`。不需要 `assets` 占位文件。

按钮文字和 tooltip 分别返回当前状态对应的“中文模式”或“英文模式”。
状态变化通过 `ITfLangBarItemSink::OnUpdate` 请求 TSF 刷新。

## 错误处理

COM、VARIANT 与 GDI 调用返回错误时记录到现有文件日志。激活中的语言栏
失败不影响按键输入；停用清理尽可能执行全部步骤。生产代码不使用
`unwrap()` 或 `expect()`。

## 测试

先为不依赖 TSF 运行环境的纯逻辑写失败测试：

- `bool` 模式映射到“中/英”显示信息
- compartment 变化只在实际状态变化时要求清理和刷新

随后实现最小逻辑使测试通过。COM 注册、接口签名和 GDI 代码通过
`cargo test -p im_engine`、`cargo clippy -p im_engine --all-targets
-- -D warnings` 与 Windows 现场验证覆盖。
