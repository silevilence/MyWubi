//! Optional model adapters and offline evaluation. Never linked into the TSF DLL.
pub mod catalog;
pub mod evaluation;
#[cfg(feature = "llama-cpp-2")]
pub mod llama;
#[cfg(feature = "ort")]
pub mod onnx;
pub mod package;
pub mod scheduler;
pub mod service;
pub mod worker;
