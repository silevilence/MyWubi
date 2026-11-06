# Optional inference runtime notices

These notices accompany the optional native runtimes, not model weights. `sources.json`
records versions and SHA-256 of the unmodified notice files. No weight is included in
the base installer or APK. Each imported model supplies its own source and license.

* ONNX Runtime 1.31.1: `LICENSE` and `ThirdPartyNotices.txt` from the official
  `onnxruntime==1.31.1` distribution; Android uses the matching official
  `com.microsoft.onnxruntime:onnxruntime-android:1.31.1` Maven AAR.
  Upstream: https://github.com/microsoft/onnxruntime/tree/v1.31.1
* ort 2.0.0-rc.15: `LICENSE-MIT` and `LICENSE-APACHE` from the pinned Cargo package.
  Upstream: https://github.com/pykeio/ort
* llama.cpp: license from the vendored source in `llama-cpp-sys-2==0.1.161`.
  The exact source revision is recorded by Cargo.lock and that crate's metadata.
* llama-cpp-rs: license files at revision
  `e7af607ebd5ec2932709ef918c17cca2f9b0fce4`, corresponding to version 0.1.161.
  Upstream: https://github.com/utilityai/llama-cpp-rs
* Android NDK 27.2.12479018: original `NOTICE.toolchain`, including the static C++
  runtime linked into the Android native library.
* Optional Windows Vulkan component: Vulkan-Headers 1.4.363 (Khronos, MIT or
  Apache-2.0), including the generated Vulkan-Hpp bindings. The checked-in MIT
  notice and copyright inventory accompany the statically linked llama.cpp
  backend. Vulkan SDK 1.4.363.1 / glslc are build tools, not redistributed.
  The GPU vendor supplies `vulkan-1.dll` and the driver; no SDK, driver or model
  weight is included in the component. Source:
  https://github.com/KhronosGroup/Vulkan-Headers/tree/v1.4.363

Run `python scripts/check_model_artifacts.py` to check the notice hashes; add
`--apk PATH` or `--windows-dir PATH` to check a staged optional-model artifact.
