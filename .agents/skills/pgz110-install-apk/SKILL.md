---
name: pgz110-install-apk
description: 在 PGZ110 真机安装调试 APK 时，处理 ColorOS 开发者验证和锁屏密码确认。
---

# PGZ110 真机安装调试 APK

本机测试设备的 ADB 序列号、测试密码和使用授权记录在同目录的 `device.local.md`。仅在已授权设备的安装确认页使用该密码。其他电脑首次使用时，由设备持有人提供本机信息。

1. **核对设备。** 从本机记录取得设备序列号，在 PowerShell 中赋给 `$serial`。用 `adb devices -l` 确认该序列号状态为 `device`，再用 `adb -s $serial shell getprop ro.product.model` 确认型号为 `PGZ110`。
2. **启动安装。** 从待装 APK 核对包名和应用名，执行 `adb -s $serial install -r "$apk"`（`$apk` 为本地 APK 路径，标记为 testOnly 的包另加 `-t`）。若直接返回 `Success`，跳到第 5 步。若安装等待确认，保留安装进程，在另一终端设置同一序列号并操作手机。
3. **处理开发者验证。** 用 `adb -s $serial shell dumpsys window | Select-String mCurrentFocus` 查看当前页面，用 `adb -s $serial exec-out uiautomator dump /dev/tty` 读取控件。在 `com.oplus.appdetail` 的 `InstallGuideActivity` 核对待装应用，点说明末尾的「继续安装」链接。若控件树只显示整段文字，用当前页面截图定位链接。在「未验证应用提醒」勾选「已知悉应用风险」，等「仍然安装」可用后点击。
4. **输入测试密码。** 安装流程进入系统设置的 `ConfirmLockPassword` 页面时，在控件树的 `com.android.settings:id/keyboard_num` 下按 `content-desc` 找到每一位对应的数字键。按密码顺序用 `adb -s $serial shell input tap X Y` 点击各键 `bounds` 的中心。安全键盘逐键接收输入，第六位后自动提交。若密码被拒绝，停止输入并报告页面提示。
5. **核验结果。** 等原安装进程返回 `Success`，再运行 `adb -s $serial shell pm path $packageName`（`$packageName` 为第 2 步核对的包名），确认返回已安装 APK 的路径。若安装失败，记录原进程的错误信息。
