# 候选框右侧闪烁回归（2026-11-04）

## 现象与根因

用户视频中，静止的预测列表「节 / 期间 / 长假 / 个 / 下」最右侧文字间歇缺失。
候选渲染线程每 16ms 先通过 `SetWindowPos` 使用字符数估算的宽度，随后
`UpdateLayeredWindow` 又使用 GDI 实测宽度。两个尺寸不一致，形成短暂裁切。

回归测试创建真实分层窗口，调用生产 `handle_timer`，同时记录
`WM_WINDOWPOSCHANGING` 中间尺寸与更新完成后的 `GetWindowRect` 尺寸。
修复前执行：

```text
cargo test -p im_engine --lib unchanged_candidates_never_resize -- --nocapture
candidate width oscillates: [(260, 34), (281, 34), (260, 34),
                            (281, 34), (260, 34), (281, 34)]
FAILED
```

修复删除估算尺寸的预先调整。GDI 真实测量结果同时用于屏幕避让、位图大小，
并通过一次 `UpdateLayeredWindow` 提交位置、尺寸和图像。

## 验证

- 原回归测试通过；`cargo test -p im_engine`：73 个单元测试、3 个集成测试通过。
- `cargo fmt --all --check`、`git diff --check` 通过。
- `cargo build --release -p im_engine` 通过，仍有已有的 LNK4104 导出警告。
- 在已有沙盒中注册到 `C:\MyWubiTrialFix2`，新 Edge 进程加载测试页。
  实际按 `g`、空格、空格产生「五一」及视频同组预测；观察右侧文字完整稳定。
  原有测试窗口保留，新页面标题为「MyWubi 闪烁修复测试」。
- DLL SHA-256：`9da0002f422f34b436fe560ab94f10afc6450a029bf8ffe026727b5e29dbc95a`。

## 连续首选的质量限制

已检查沙盒实际配置：预测开启，未选模型。当前 `Lexicon::recall` 按前文末尾的
最多八种后缀匹配词库，再加少量人工联想对，以后缀长度和权重排列候选。
这不能保证句子语义、分词边界或语法连续；例如「分钟 → 钟头 → 头发 → 发展」
可以形成重叠词链。反复接受首选会放大这一限制。

本次只修复窗口显示，不将上述质量问题视为通过验收，也不自动切换到尚未达标的
实验模型。完整前文约束、合理停止及预测质量评测仍需后续推进。
