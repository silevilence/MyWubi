# SK-01～20 实施与验收记录

2026-11-14。本次只在本地提交，没有推送或发布 Release。任务状态以 ROADMAP 的逐项勾选为准；实现通过不等于全部设备验收通过。

## 环境与资源

Windows 11 Pro 26200、Intel Core Ultra 7 165U；Windows 参数阶段使用同一 Sandbox/Edge，图片编辑器使用仓库 `target/skin-acceptance/editor` 的独立便携配置，未操作主机 TIP 安装。
Android 使用已有 Pixel 5 API 34 x86_64 AVD，沿用现有应用/测试宿主与数据，未擦除模拟器。用户明确选择先用模拟器，真机项保留未完成。
构建复用本地 JDK 21.0.8、SDK、NDK 27.2.12479018、Gradle/Cargo 缓存、字体及码表，均离线。没有重新下载模型、组件或安装包，也没有发布后回下载验。
原创图片通过内置 imagegen 生成一次，原图已保存到 [模板素材](../examples/skins/assets/bamboo-paper.png)，生成提示见 [来源记录](../examples/skins/provenance.md)。

## 代码与产物检查

- 最终工作区 `cargo test --workspace --offline --locked`：541 项通过。随后增加的竹纸模板原生绘制检查通过；图片定向组共 3 项通过，其中 2 项已包含在工作区结果中。
- `core_engine` 与 `text_render` 全目标严格 Clippy 通过。扩大到全部 Windows 前端时，既有 COM 裸指针接口及 settings/model 等警告仍使 `-D warnings` 失败，未记作整体 Clippy 通过。
- Windows settings/im_engine Release 离线构建通过。链接器仍有既有 COM 导出 PRIVATE 提示。
- Android ARM64/x86_64 JNI、Debug APK、设备测试 APK 及 3 项 JVM 单元测试离线构建通过。SDK 只读 package.xml 警告未影响构建，未改变其权限。
- 包路径、ZIP 限额、三级能力降级、未知扩展保存、参数覆盖、原子快照、输入会话隔离、图片 CRC/尺寸/预算、九宫格、预乘 alpha、资源共享和释放均有定向测试。

摘要见 [测试结果](validation/skins/test-results.json)，最终 DLL/EXE/APK 及模板的大小和 SHA-256 见 [产物清单](validation/skins/artifacts.json)。原始运行日志保存在本地 `target/skin-acceptance/logs`，不把临时构建日志加入版本库。

## Windows 实际证据

参数阶段 Release DLL 在 Sandbox/Edge 中完成浅深色候选显示、空格上屏、编码中换肤后数字选词。初次重验发现配置文件占用时走入完整运行时重建，修复后同一输入编码保持，数字 2 上屏「式」；9 项重载测试通过。[实际输入截图](validation/skins/windows-hot-switch.png)

最终图片编辑器实际打开竹纸双端模板，查看 Windows 原生候选及 Android 示意图片预览，将 Android 浅色 background 图片透明度从 0.48 改为约 0.22，导出新文件并自动重新打开。导出前后只有 `android/appearance.toml` 改变；Windows 外观、两端描述文件、根 metadata 及共享 PNG 均字节一致。[逐文件核对](validation/skins/editor-image-roundtrip.json)、[编辑界面](validation/skins/windows-image-editor.jpg)、[导出后界面](validation/skins/windows-image-export.jpg)

该实际导出文件被原样加入 Android 测试资产；没有用手工契约包替代图片编辑器闭环。Windows 原生离屏绘制也使用交付模板并覆盖长候选和彩色表情；它不替代 TSF 现场或真实可见延迟测量。

图片阶段最终 DLL 未重新完成完整多宿主 TSF 矩阵。记事本、多屏/多 DPI、安装/AppData 路径、屏幕边缘与真实可见 p95、千次换肤多宿主资源曲线仍未覆盖。不能把参数阶段 DLL 的现场结果标为最终图片 DLL 的全部验收。

## Android 证据边界

最终 APK 在现有模拟器上验证实际 Windows 参数/图片导出包的双变体解析和完整资源往返，三套参数皮肤与图片皮肤的原生预览、1×/2× 字体、候选位置及触摸派发。图片角色和双变体共享同一位图；损坏数据不能进入活动外观。
SAF 文件选择、草稿选择、Activity 重建恢复、保存和系统分享使用 Android 系统组件。重复运行测试先建立内置外观基线；导入返回后等待目标组件按钮启用，再断言产生草稿。
按键释放气泡设计为短暂保留；手势验收等待其消失，不把 Compose 暂时空闲误当作延迟清除已完成。
截图复核发现横屏 2× 字体时底排裁切，已按窗口剩余高度协调候选与按键字号，保留完整键行；测试增加第三排和空格排可见高度断言。字号在短窗口受可用空间限制，竖屏保留系统缩放；可访问性节点仍保留完整名称。
多套包可以含同名组件，选用与分享按钮增加皮肤名称的可访问性描述。设备测试按具体皮肤定位，避免把另一套 jade 误认为导入的目标包。

模拟器图片证据：[竖屏 2× 字体](validation/skins/android-bamboo-portrait-2x.png)、[深色横屏 2× 字体](validation/skins/android-bamboo-dark-landscape-2x.png)、[外部宿主输入](validation/skins/android-image-real-host.png)、[系统分享](validation/skins/android-system-share.png)。Windows 原生绘制证据见 [长候选与表情](validation/skins/windows-bamboo-native.png)。

最终完整运行 21 项中 20 项通过；修复测试的后台导入等待与预览滚动定位后，剩余 SAF 流程定向通过，新增横屏展开候选检查也通过。共 21 个不同设备测试取得通过结果；没有把多轮结果描述成单次全绿运行。明细写入上述测试 JSON。真机、ARM64 实际运行、16 KB 页设备、TalkBack 人工体验、真实进程回收、OEM 导航差异及硬件延迟/内存长测保持未完成。Activity 重建不等于进程被系统回收。

## 联合交付限制

SK-20 的 [48 格组合矩阵](validation/skins/keyboard-combination-matrix.csv) 已固定：参数/图片 × 26/14/9 键 × 浅/深 × 竖/横 × 八向独立/合并。
AK-17 尚未完成且不属于本次 SK 范围，全部联合组合保留 `not_run`。已有普通 26 键和手势回归不等于自定义布局、十四/九键或八向组合通过，也未擅自实施 AK 系列。
SK-01 的可见延迟/资源基线、SK-11/14 完整设备矩阵以及 SK-16/17/19 的未覆盖验收项继续保留未完成。
