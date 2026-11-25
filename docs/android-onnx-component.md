# Android 预测组件

标准 APK 在 Android 8.0（API 26）及以上提供 ARM64 和 x86_64 两种架构。GGUF 通过静态链接的 llama.cpp 运行。ONNX 适配器使用 ort 2.0.0-rc.15 的 C API 31，运行库固定为微软发布到 Maven Central 的 onnxruntime-android 1.31.1。词典预测可独立使用。模型权重由用户另行选择并下载或导入。

应用根据已加载的 JNI 库架构从固定 AAR 选择 libonnxruntime.so。其 ELF 依赖为系统提供的 libdl、liblog、libandroid、libm 和 libc。AAR 的 SHA-256 与两种架构运行库的大小、摘要见 OnnxComponent.kt。原生库采用 MIT 许可证，完整许可与第三方声明随 APK 放在 assets/licenses/models。升级运行库需要随应用更新可信清单和兼容性检查。

在 x86_64 设备上强制以 ARM64 转译方式启动时，预测设置会提示改用本机架构。按系统默认方式覆盖安装双架构完整包即可使用内置的 x86_64 运行库，已有配置、词库和模型会保留。

开发构建使用 `android/build-models.ps1`，默认生成标准包。`-PbundleOnnx=true` 可生成内置同版本 ONNX Runtime 的完整包。`-PmodelBackends=false` 仅用于基础版体积对照。三种构建均沿用相同的原生库压缩策略。

覆盖安装保留应用私有数据。旧模型包内置的 ONNX 库随旧 APK 替换，模型选择和预测开关照原配置保留。标准包从设置的「预测」页获取固定版本组件，组件下载完成后再选择模型、保存并启用。完整包优先使用由 Android 包签名保护的内置库。已加载的可选库可在切换并保存其他模型后请求移除，下次进程启动完成移除。

发布流程使用同一签名、包名和版本生成 Android.apk 和 Android-full.apk。可从 GitHub 正式发布页手动覆盖安装同版本完整包，随后完整包读取 android-update-full.json。普通包继续读取 android-update.json。已缓存的其他类型更新在安装前会被拒绝。
