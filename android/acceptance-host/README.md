# 外部 IME 验收宿主

仅通过 `-PacceptanceHost=true` 加入构建。独立包与进程中的原生 `EditText`，没有 JNI、模型、网络或输入连接替身；不打入输入法 APK，也不用于发布。

```powershell
./android/build-models.ps1 -Tasks @('-PacceptanceHost=true', ':acceptance-host:assembleDebug', ':app:assembleDebug', ':app:assembleDebugAndroidTest')
adb install -r android/acceptance-host/build/outputs/apk/debug/acceptance-host-debug.apk
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
adb install -r android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
adb shell am instrument -w -r -e model_threads 4 -e prediction_host editor -e class org.mywubi.PredictionHostTest org.mywubi.test/androidx.test.runner.AndroidJUnitRunner
```

先按模型验证文档准备固定许可模型包。缺少生成包会跳过生成用例，跳过不能算验收通过。省略 `prediction_host` 时仍运行系统设置搜索框，二者使用相同的正文、候选、外部编辑、连续选择与静默回退断言。轻量编辑器通过不能证明模型在所有宿主负载下都能按时返回；设置搜索框的超时必须单独保留。
