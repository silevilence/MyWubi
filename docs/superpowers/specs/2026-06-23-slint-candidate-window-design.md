# Slint 轻量化候选框 UI 设计规格

- **日期**: 2026-06-23
- **范围**: MyWubi Windows 输入法候选框子系统
- **状态**: 待审核

---

## 1. 目标与非目标

### 目标

- 在 `im_engine.dll` 中实现基于 Slint 声明式 UI + Win32 透明分层窗口的无焦点候选框
- 候选框跟随系统光标定位，自动避让屏幕边缘
- 支持 `config.toml` 中定义的外观主题（字体大小、配色）
- 将候选数据通过 wait-free 通道（`ArcSwap`）从 TSF 线程传递给渲染线程

### 非目标

- 本次不包含 `config.toml` 热重载（后续独立迭代）
- 不包含鼠标点击候选词上屏（仅键盘选择）
- 不包含候选框动画/过渡效果
- 不包含 Android 候选框（Kotlin/Compose 独立实现）

---

## 2. 架构概览

```
┌─────────────────── ctfmon.exe 进程空间 ────────────────────┐
│                                                              │
│  TSF 线程 (STA)                   候选框窗口线程              │
│  ┌─────────────┐                  ┌──────────────────────┐  │
│  │ TextService │──ArcSwap───────→│ CandidateWindow       │  │
│  │ (按键/焦点) │  CandidateData  │ (Win32 + Slint 软渲)  │  │
│  │             │←──SetWindowPos──│                      │  │
│  │ ITfContext  │   光标位置       │  ┌────────────────┐  │  │
│  │ GetStatus   │                  │  │ Slint Component│  │  │
│  └─────────────┘                  │  │ .slint UI 定义 │  │  │
│       │                           │  │ software渲染器 │  │  │
│       │ 插入文本                   │  └────────────────┘  │  │
│       ▼                           │         │             │  │
│  ITfEditSession                    │  UpdateLayeredWindow  │  │
│  (上屏文字)                        │  (贴到屏幕)           │  │
│                                    └──────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

**核心原则**：数据单向流动。CandidateWindow 线程拥有自己的消息泵，从 `ArcSwap<CandidateData>` 只读最新数据并渲染，绝不回调 TSF 线程。

---

## 3. 模块分解

### 3.1 文件结构

```
windows/im_engine/
├── Cargo.toml                          # 新增 slint, fontdb 依赖
├── build.rs                            # 【新增】slint-build 编译 .slint → Rust
├── ui/
│   └── candidate_window.slint          # 【新增】Slint 声明式 UI 定义
├── src/
│   ├── lib.rs                          # (已有) COM 入口
│   ├── factory.rs                      # (已有)
│   ├── guids.rs                        # (已有)
│   ├── key_filter.rs                   # (已有)
│   ├── registrar.rs                    # (已有)
│   ├── text_service.rs                 # (已有，新增 candidate_data 写入)
│   ├── candidate_data.rs               # 【新增】共享数据结构
│   ├── candidate_window.rs             # 【新增】Win32 窗口创建/消息泵
│   ├── candidate_renderer.rs           # 【新增】Slint 初始化 + 软件渲染
│   └── screen_geometry.rs              # 【新增】光标定位 + 避让算法
```

### 3.2 各模块职责

#### `candidate_data.rs` — 共享数据结构

```rust
pub struct CandidateData {
    pub visible: bool,
    pub spelling: String,
    pub items: Vec<CandidateItem>,
    pub highlighted: usize,
    pub page: usize,
    pub total_pages: usize,
    pub anchor: Option<ScreenPoint>,
    pub theme: ThemeSnapshot,
}

pub struct CandidateItem {
    pub text: String,
    pub label: String,
}

pub struct ScreenPoint { pub x: i32, pub y: i32 }

pub struct ThemeSnapshot {
    pub font_size: u16,
    pub primary_color: u32,
    pub background_color: u32,
    pub highlight_color: u32,
}
```

- 数据由 TSF 线程在每次 `StateMachine::handle()` 后构造
- 通过 `ArcSwap<CandidateData>` 发布（单写多读，wait-free）
- `visible` 字段控制窗口显示/隐藏

#### `candidate_window.slint` — 声明式 UI

```slint
component CandidateItem {
    in property <string> label;
    in property <string> text;
    in property <bool> highlighted;
    in property <color> primary;
    in property <color> highlight;
    width: 80px;  // 单项固定宽度
    // 高亮时反色
}

component CandidateWindow {
    in property <[CandidateItemData]> candidates;
    in property <int> page;
    in property <int> total-pages;
    in property <color> background;
    in property <length> font-size;

    Rectangle {
        background: root.background;
        HorizontalLayout {
            for item in root.candidates: CandidateItem { ... }
        }
        if root.total-pages > 1: Text { text: "\{page}/\{total-pages}"; }
    }
}
```

- 候选词水平排列，单项固定宽度 80px
- 仅多页时显示翻页指示
- 所有颜色/字号由外部属性注入

#### `candidate_window.rs` — Win32 窗口管理

- 创建 `WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TOOLWINDOW` 窗口
- 独立线程运行 `GetMessage` / `DispatchMessage` 消息泵
- `WM_TIMER` 每 16ms 轮询 `ArcSwap::load()`，触发渲染
- `WM_QUIT` 时优雅退出（500ms 超时后 `TerminateThread`）
- 窗口创建失败则降级为"无声候选"模式

#### `candidate_renderer.rs` — Slint 渲染桥接

- 初始化 `slint::software::SoftwareRenderer`，绑定 `.slint` 组件
- 字体加载抽象为 `FontProvider` trait：
  - **方案 A（当前）**: `EmbeddedFontProvider` — 编译期内嵌思源黑体
  - **方案 B（预留）**: `GdiFontProvider` — 通过 Win32 GDI 查询系统字体
- 每帧：Slint 属性更新 → `render()` 生成 ARGB buffer → `UpdateLayeredWindow` 贴图

```rust
pub trait FontProvider: Send + Sync {
    fn load(&self) -> Vec<u8>;
}

pub struct EmbeddedFontProvider;
impl FontProvider for EmbeddedFontProvider { ... }

// 预留
pub struct GdiFontProvider;
impl FontProvider for GdiFontProvider { ... }
```

#### `screen_geometry.rs` — 光标定位与避让

```rust
/// 从 ITfContext 获取光标屏幕坐标
pub fn get_caret_position(context: &ITfContext) -> Option<ScreenPoint>;

/// 计算候选框最终位置（自动避让屏幕边缘）
pub fn compute_window_rect(
    anchor: ScreenPoint,
    window_size: (i32, i32),
    monitor_rect: (i32, i32, i32, i32),  // left, top, right, bottom
) -> (i32, i32);  // 左上角坐标
```

避让规则：
- 光标在屏幕下半部 → 候选框显示在光标上方
- 光标靠近右边缘 → 候选框向左偏移
- 屏幕边缘留 8px padding

---

## 4. 数据流

```
按键 event → TextService::on_key_down()
    │
    ├─ StateMachine::handle(event) → Transition
    │
    ├─ Transition::Commit(text)
    │     → ITfEditSession 插入文本
    │     → store CandidateData { visible: false }
    │
    ├─ Transition::Candidates { spelling, candidates, page, total_pages }
    │     → screen_geometry::get_caret_position(context)
    │     → 组装 CandidateData { visible: true, anchor, ... }
    │     → ArcSwap::store(Arc::new(data))
    │
    ├─ Transition::SpellingUpdated(s)
    │     → store CandidateData { visible: false }  // 无候选时不显示
    │
    └─ Transition::Cleared / Passthrough
          → store CandidateData { visible: false }

候选框线程 (16ms timer):
    data = ArcSwap::load()
    if data.visible:
        SetWindowPos(hwnd, anchor)
        slint_component.set_candidates(data.items)
        slint_component.set_page(data.page, data.total_pages)
        buffer = software_renderer.render()
        UpdateLayeredWindow(hwnd, buffer)
        ShowWindow(hwnd, SW_SHOWNA)
    else:
        ShowWindow(hwnd, SW_HIDE)
```

---

## 5. 错误处理与边界

| 场景 | 策略 |
|------|------|
| 候选框线程 panic | `catch_unwind` 包裹消息泵，panic 后恢复隐藏，不影响 TSF 线程 |
| 候选框窗口创建失败 | TSF 线程降级为"无声候选"——日志记录，不影响打字 |
| Slint 渲染失败 | 捕获错误，不回写 `UpdateLayeredWindow`，TSF 不受影响 |
| TSF 线程 Deactivate | 写入 `visible=false` + `PostThreadMessage(WM_QUIT)`，500ms 超时后 `TerminateThread` |
| 多次 Activate/Deactivate | `OnceLock` 保证候选框线程单例，重复 Activate 复用 |
| 编码串为空但候选非空 | 防御：`spelling.is_empty() => visible=false` |
| 候选项文本过长 | 最大显示宽度截断（100px），超出显示省略号 |
| `ArcSwap` 竞争 | wait-free——`load()` 总是返回完整快照 |
| 候选数据与光标异步 | 16ms 延迟内光标偏移可忽略 |

---

## 6. 依赖

```toml
[dependencies]
slint = { version = "1", default-features = false, features = ["software-renderer", "compat-1-2"] }
fontdb = { version = "0.21", default-features = false }

[build-dependencies]
slint-build = "1"
```

---

## 7. 测试策略

| 层级 | 内容 | 方式 |
|------|------|------|
| 单元 | `screen_geometry::compute_window_rect` 避让算法 | 给定虚拟屏幕尺寸和光标位置，断言输出坐标 |
| 单元 | `CandidateData` 默认值边界（`spelling=""` → `visible=false`） | 纯函数断言 |
| 集成 | `candidate_renderer` Slint 渲染 | 测试线程创建隐藏窗口，喂候选数据，验证 `render()` 不 panic 且返回非零 buffer |
| 集成 | `candidate_window` 生命周期 | 启动窗口线程 → 发送 `WM_QUIT` → 验证 200ms 内退出 |
| 手动 | 真实 TSF 环境 | 编译 DLL → `regsvr32` 注册 → 记事本打字验证候选框位置和内容 |

---

## 8. 与现有代码的集成点

- **`text_service.rs`**: 在 `apply_transition()` 中，`Transition::Candidates` 分支改为构造 `CandidateData` 并 `ArcSwap::store`；在 `Activate` 中启动候选框线程，`Deactivate` 中停止
- **`factory.rs`**: 在创建 `TextService` 时传入共享的 `Arc<ArcSwap<CandidateData>>`
- **`lib.rs`**: `ENGINE` 单例新增 `ArcSwap<CandidateData>` 字段，供 `im_engine_init` 初始化

---

## 9. 待决项

- 嵌入的具体字体文件：建议 `Noto Sans SC Regular`（思源黑体，~5MB），需确认 license 兼容性
- Slint 版本锁定：初选 `1.x` stable，关注 `software-renderer` feature 稳定性
