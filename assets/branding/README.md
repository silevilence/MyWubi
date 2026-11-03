# MyWubi 图标

`mywubi.png` 是通过内置 image_gen 生成的原始图标。`export-icons.ps1` 仅使用
System.Drawing 缩放、封装平台资源，不重新绘制图案。运行后更新：

- Android 各密度启动图标、自适应图标前景及设置页/键盘工具入口。
- Windows EXE/DLL 图标、设置窗口、关于面板和 Velopack 安装包图标。

Windows 语言栏的“中／英”图标是输入状态指示，继续使用动态文字。

生成提示词（内置工具模式）：

> Use case: logo-brand. Create a single production app icon for MyWubi, a Chinese shape-code keyboard/input method for Android and Windows. A precise geometric white monogram inspired by the Chinese character 五 and interlocking keyboard strokes, centered on a solid deep jade square (#245D51). Flat vector-like edges, no gradient, no shadow, no texture, no words, no watermarks, no mockup. Full bleed square jade background, with the white symbol entirely within the central 56 percent of the canvas to survive Android adaptive circular cropping. Distinct silhouette readable at 24 pixels. Output one square icon.
