//! Local personal input storage, exchange and learning. No network clients.
pub mod capture;
pub mod capture_endpoint;
pub mod exchange;
pub mod recorder;
pub mod store;
pub use core_engine::personal::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("个人数据读写失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("个人数据格式损坏")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(&'static str),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn identity() -> Result<String> {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 16];
    ring::rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| Error::Invalid("系统随机数不可用"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub mod samples;
