# Android 预测组件

标准 APK 在 Android 8.0（API 26）及以上提供 ARM64 和 x86_64 两种架构。GGUF 通过静态链接的 llama.cpp 运行。ONNX 适配器使用 ort 2.0.0-rc.15 的 C API 31，运行库固定为微软发布到 Maven Central 的 onnxruntime-android 1.31.1。词典预测可独立使用。模型权重由用户另行选择并下载或导入。

应用根据当前架构从固定 AAR 选择 libonnxruntime.so。其 ELF 依赖为系统提供的 libdl、liblog、libandroid、libm 和 libc。AAR 的 SHA-256 与两种架构运行库的大小、摘要见 OnnxComponent.kt。原生库采用 MIT 许可证，完整许可与第三方声明随 APK 放在 assets/licenses/models。升级运行库需要随应用更新可信清单和兼容性检查。

开发构建使用 `android/build-models.ps1`，默认生成标准包。`-PbundleOnnx=true` 可生成内置同版本 ONNX Runtime 的完整包。`-PmodelBackends=false` 仅用于基础版体积对照。三种构建均沿用相同的原生库压缩策略。
