//! Management operations run on settings/background workers, never key dispatch.
use crate::{
    exchange::{self, ImportMode},
    learning,
    store::Store,
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
#[derive(Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Command {
    Status,
    Export {
        path: PathBuf,
        password: Option<String>,
    },
    Preview {
        path: PathBuf,
        password: Option<String>,
        mode: ImportMode,
    },
    Import {
        path: PathBuf,
        password: Option<String>,
        mode: ImportMode,
        revision: String,
    },
    Clear {
        models: bool,
    },
    Train,
    Rollback,
}
fn read_package(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((exchange::PACKAGE_LIMIT + 54) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > exchange::PACKAGE_LIMIT + 53 {
        return Err(Error::Invalid("个人数据包超过上限"));
    }
    Ok(bytes)
}
pub fn execute(config: &Path, command: Command) -> Result<serde_json::Value> {
    let store = Store::beside_config(config);
    match command {
        Command::Status => {
            let model = learning::load(&store).ok();
            Ok(
                serde_json::json!({"data":store.stats()?,"model":model.as_ref().map(|m| serde_json::json!({"version":m.version,"samples":m.sample_count,"updated":m.created_ms,"algorithm":m.algorithm})),"quality":"experimental"}),
            )
        }
        Command::Export { path, password } => {
            let bytes = exchange::export(&store, password.as_deref())?;
            core_engine::data::atomic_write(&path, &bytes)?;
            Ok(serde_json::json!({"message":"个人数据已导出"}))
        }
        Command::Preview {
            path,
            password,
            mode,
        } => {
            let events = exchange::decode(&read_package(&path)?, password.as_deref())?;
            Ok(serde_json::to_value(exchange::preview(
                &store, &events, mode,
            )?)?)
        }
        Command::Import {
            path,
            password,
            mode,
            revision,
        } => {
            let events = exchange::decode(&read_package(&path)?, password.as_deref())?;
            let limit = core_engine::Config::load(config)
                .map_err(|_| Error::Invalid("配置读取失败"))?
                .personal
                .max_mb;
            Ok(serde_json::to_value(exchange::import(
                &store, events, mode, &revision, limit,
            )?)?)
        }
        Command::Clear { models } => {
            if core_engine::Config::load(config)
                .map_err(|_| Error::Invalid("配置读取失败"))?
                .personal
                .collect
            {
                return Err(Error::Invalid("请先关闭并保存个人输入数据收集，再清除记录"));
            }
            store.clear(models)?;
            Ok(
                serde_json::json!({"message":if models {"记录、训练缓存与个人模型已清除"} else {"记录与训练缓存已清除；已有模型知识保留"}}),
            )
        }
        Command::Train => {
            let config =
                core_engine::Config::load(config).map_err(|_| Error::Invalid("配置读取失败"))?;
            if !config.personal.learn {
                return Err(Error::Invalid("后台学习已暂停，请开启并保存后更新"));
            }
            let model = learning::train(&store, &AtomicBool::new(false))?;
            Ok(serde_json::json!({"message":"个人模型已更新（实验性）","version":model.version}))
        }
        Command::Rollback => {
            learning::rollback(&store)?;
            Ok(serde_json::json!({"message":"已回退上一可用个人模型"}))
        }
    }
}
#[derive(Deserialize)]
struct Request {
    config: PathBuf,
    #[serde(flatten)]
    command: Command,
}
pub fn exchange(bytes: &[u8]) -> Vec<u8> {
    let result = (|| -> Result<serde_json::Value> {
        if bytes.len() > 65536 {
            return Err(Error::Invalid("请求过大"));
        }
        let request: Request = serde_json::from_slice(bytes)?;
        execute(&request.config, request.command)
    })();
    let reply = match result {
        Ok(value) => serde_json::json!({"ok":true,"value":value}),
        Err(error) => serde_json::json!({"ok":false,"error":error.to_string()}),
    };
    serde_json::to_vec(&reply).unwrap_or_default()
}
