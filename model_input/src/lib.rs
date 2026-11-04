//! Optional model adapters and offline evaluation. Never linked into the TSF DLL.
pub mod evaluation;
#[cfg(feature = "llama-cpp-2")]
pub mod llama;
#[cfg(feature = "ort")]
pub mod onnx;
pub mod package;
pub mod scheduler;
