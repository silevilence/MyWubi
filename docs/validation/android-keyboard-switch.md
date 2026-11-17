# Android 形码布局切换与独立皮肤页验收

日期：2026-11-17。本记录用于 V0.10.2 发布准备；功能验收使用版本号更新前的 0.10.1 Debug 产物，下列哈希对应验收构建，不是正式发布包。

## 行为

- 「皮肤」独立为设置分类，包含外观、导入分享和键盘组合管理。分类切换与 Activity 重建保留外观草稿。
- 当前皮肤包有多个形码布局时，空闲工具栏显示「切换形码键盘」。纯外观皮肤沿用当前形码键盘包；包 ID 和版本必须相同，不混入其他包或拼音布局。
- 选择只保存 Main 布局，使用目标布局推荐映射，保留当前外观。拼音、英文、数字和符号仍走原功能入口。
- 当前项有文字与勾选语义；列表可滚动，不可用项禁用。布局发布继续使用无触摸、无未完成编码的门槛。

## 环境与资源

Windows，JDK 21.0.8，Android SDK 35，NDK 27.2.12479018；本地 Pixel 5 API 34 x86_64 AVD，使用无窗口软件渲染。
Gradle 使用 `--offline`，Cargo 使用 `CARGO_NET_OFFLINE=true`；复用已有 SDK、Gradle/Cargo 缓存、AVD、皮肤测试包和独立宿主 APK。
本次没有验证在线下载，也没有下载模型、皮肤或发布资产。

| 产物 | SHA-256 |
| --- | --- |
| `android/app/build/outputs/apk/debug/app-debug.apk` | `DA937A0B0F40982F7D6F482AB292C6936ACA7E0156B0BD5A82447543D07C83B3` |
| `android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk` | `8DA4B7DB948D8E48E0AD16F6518256764D54B3486C8FB057BBCFD5FFFD0D2308` |
| `android/acceptance-host/build/outputs/apk/debug/acceptance-host-debug.apk` | `EB9340F2C742AB18976C291FDDA6A3A6C93B906062FF0940650706DC0AE89ED6` |

## 命令与结果

```powershell
$env:ANDROID_HOME='C:/Program Files (x86)/Android/android-sdk'
$env:JAVA_HOME='C:/Program Files/Android/openjdk/jdk-21.0.8'
$env:CARGO_NET_OFFLINE='true'
./android/gradlew.bat -p android :app:testDebugUnitTest :app:assembleDebug :app:assembleDebugAndroidTest :app:lintDebug --offline --console=plain
```

两个 ABI 构建通过；12 项 JVM 单元测试通过。Lint 0 错误、10 警告（备份规则、启动图标等），最终日志为 `android/build/keyboard-switch-lint-final.log`。`git diff --check` 通过。

设备命令前缀：`adb -s emulator-5554 shell am instrument -w`；runner：`org.mywubi.test/androidx.test.runner.AndroidJUnitRunner`。以 `-e class` 选择下表测试；复测只执行失败或新增变体。

| 测试 | 最终结果与覆盖 |
| --- | --- |
| `MainKeyboardMenuTest` | 通过：当前项与不可用状态、选当前项、返回取消、10 项列表末项、准确派发布局选择、切换中禁用、单布局及其他模式不显示入口 |
| `CustomKeyboardHostTest` | 通过：真实独立 EditText 中全键、14 键、9 键上屏“王”，英文、拼音上屏；菜单切换后焦点、保存选择、外观保持；数字/符号往返；密码、数字、网址、邮箱隔离；重新进入宿主保留选择；恢复全键 |
| `SettingsNavigationTest`（5 项） | 通过：独立皮肤页、草稿与重建、全部分类滚动访问、末项搜索、横屏保存区、词典预测入口 |
| `SkinSettingsUiTest` | 通过：系统文件选择器导入本地包、选择产生草稿、原生预览、重建保留、保存回读确认、打开系统分享面板（未发送给他人） |
| 大字体与明暗变体（3 次） | 通过：浅色竖屏 200% 菜单；深色横屏 200% 菜单；系统 150% 字体深色皮肤页与草稿重建 |

上述 8 项不同设备测试及 3 次显示变体均已得到通过结果，分布在以下日志：

- `android/build/keyboard-switch-device-verified.log`：皮肤页与其余导航测试通过；保留当时未修复的脚本失败记录。
- `android/build/keyboard-switch-device-focused.log`：真实宿主、导航旋转、皮肤导入分享通过；菜单时序断言随后单独复测。
- `android/build/keyboard-switch-menu-pass.log`：菜单完整通过。
- `android/build/keyboard-switch-large-light.log`、`keyboard-switch-large-dark.log`：显示变体通过。

## 失败分类与证据

初次冷启动出现 System UI ANR，系统弹窗挡住目标界面；恢复同一个模拟器后复测。测试脚本还修正了：从屏幕边缘横滑触发系统返回、把描述子节点当作可操作父节点读取状态、菜单/重组尚未完成就断言、JSON 字段顺序比较、控件缓存导致的过早断言。修正保留了状态和真实结果验证，没有改为仅凭截图判定。

中途一次替换测试 APK 打断了尚未结束的测试，记录为 `Process crashed`，属于测试操作问题；随后完整的皮肤导入分享流程已通过。相关旧日志保留，未计作通过。

截图、控件树和宿主编辑日志保存在 `android/build/keyboard-switch-evidence/`：

- `ak14-real-host.png`、`ak9-pinyin-real-host.png`、`ak-host-edits.jsonl`：真实输入结果。
- `keyboard-switch-host-menu.xml`：真实皮肤包菜单与当前布局语义。
- `keyboard-menu-1080-1.0.*`、`keyboard-menu-1080-2.0.*`、`keyboard-menu-2340-2.0.*`：普通、大字体和横屏菜单。
- `settings-skin-page.*`：深色 150% 字体皮肤页。
- `skin-selected-component.*`、`skin-selected-state.json`、`skin-system-share.png`：草稿状态与分享面板。

已检查菜单和皮肤页截图。验收后恢复系统字体为 1.0、夜间模式为 no，停止三个测试应用并关闭本次模拟器；`adb devices -l` 无设备，模拟器启动进程已退出。保留本地构建和验收资源供复用。

## 未覆盖

未连接 ARM64 真机；ARM64 仅构建通过。未做 TalkBack 人工朗读、16 KB 设备与极限并发压力验收。异步准备布局期间立即继续输入的安全发布门槛有单元测试，本次未强制制造该设备端竞态。
