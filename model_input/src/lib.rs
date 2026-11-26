//! Optional model adapters and offline evaluation. Never linked into the TSF DLL.
#[cfg(feature = "android-nnapi")]
pub mod android_npu;
mod auto_device;
pub mod catalog;
pub mod compatibility;
pub mod components;
pub mod device;
pub mod download;
pub mod download_manager;
pub mod evaluation;
pub mod generation;
#[cfg(feature = "llama-cpp-2")]
pub mod llama;
#[cfg(feature = "ort")]
pub mod onnx;
pub mod package;
pub mod prediction_policy;
pub mod scheduler;
pub mod service;
pub mod trial;
pub mod worker;
