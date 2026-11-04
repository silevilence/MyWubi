//! Optional model adapters and offline evaluation. Never linked into the TSF DLL.
pub mod evaluation;
#[cfg(feature = "llama-cpp-2")]
pub mod llama;
