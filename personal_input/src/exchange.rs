//! A bounded single-file container: no archive paths or executable payloads.
use crate::{
    store::{digest, event_digest, Snapshot, Store},
    Error, Event, Result,
};
use ring::{
    aead, pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU32,
};

const MAGIC: &[u8; 8] = b"MWPERS01";
const ITERATIONS: u32 = 600_000;
pub const PACKAGE_LIMIT: usize = 128 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Package {
    version: u32,
    count: usize,
    sources: BTreeSet<String>,
    sha256: String,
    events: Vec<Event>,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Preview {
    pub added: usize,
    pub duplicates: usize,
    pub conflicts: usize,
    pub replaced: usize,
    pub revision: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ImportMode {
    Merge,
    Replace,
}

fn key(password: &str, salt: &[u8]) -> Result<aead::LessSafeKey> {
    let mut key = [0u8; 32];
    let rounds = NonZeroU32::new(ITERATIONS).ok_or(Error::Invalid("invalid KDF parameters"))?;
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        rounds,
        salt,
        password.as_bytes(),
        &mut key,
    );
    let result = aead::UnboundKey::new(&aead::AES_256_GCM, &key)
        .map(aead::LessSafeKey::new)
        .map_err(|_| Error::Invalid("加密初始化失败"));
    key.fill(0);
    result
}
pub fn export(store: &Store, password: Option<&str>) -> Result<Vec<u8>> {
    let state = store.snapshot()?;
    let events: Vec<_> = state.events.into_values().collect();
    let payload = serde_json::to_vec(&events)?;
    let package = Package {
        version: 1,
        count: events.len(),
        sources: events.iter().map(|e| e.source.clone()).collect(),
        sha256: digest(&payload),
        events,
    };
    let mut data = serde_json::to_vec(&package)?;
    if data.len() > PACKAGE_LIMIT {
        return Err(Error::Invalid("导出数据超过 128 MB"));
    }
    let mut header = MAGIC.to_vec();
    header.push(u8::from(password.is_some()));
    if let Some(password) = password {
        if password.is_empty() {
            return Err(Error::Invalid("加密密码不能为空"));
        }
        let mut salt = [0u8; 16];
        let mut nonce = [0u8; 12];
        SystemRandom::new()
            .fill(&mut salt)
            .map_err(|_| Error::Invalid("系统随机数不可用"))?;
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| Error::Invalid("系统随机数不可用"))?;
        header.extend(salt);
        header.extend(nonce);
        key(password, &salt)?
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(&header),
                &mut data,
            )
            .map_err(|_| Error::Invalid("加密失败"))?;
    }
    header.extend(data);
    Ok(header)
}
pub fn decode(bytes: &[u8], password: Option<&str>) -> Result<Vec<Event>> {
    if bytes.len() > PACKAGE_LIMIT + 53 || bytes.len() < 9 || &bytes[..8] != MAGIC {
        return Err(Error::Invalid("个人数据包版本或大小无效"));
    }
    let plain = match bytes[8] {
        0 => bytes[9..].to_vec(),
        1 if bytes.len() >= 53 => {
            let mut data = bytes[37..].to_vec();
            let mut nonce = [0u8; 12];
            nonce.copy_from_slice(&bytes[25..37]);
            let plain = key(password.ok_or(Error::Invalid("需要密码"))?, &bytes[9..25])?
                .open_in_place(
                    aead::Nonce::assume_unique_for_key(nonce),
                    aead::Aad::from(&bytes[..37]),
                    &mut data,
                )
                .map_err(|_| Error::Invalid("密码错误或数据包损坏"))?;
            plain.to_vec()
        }
        _ => return Err(Error::Invalid("个人数据包截断或加密方式不兼容")),
    };
    let package: Package = serde_json::from_slice(&plain)?;
    if package.version != 1
        || package.count != package.events.len()
        || package.sha256 != digest(&serde_json::to_vec(&package.events)?)
        || package.sources != package.events.iter().map(|e| e.source.clone()).collect()
    {
        return Err(Error::Invalid("个人数据包完整性校验失败"));
    }
    let mut ids = BTreeSet::new();
    for event in &package.events {
        event.validate().map_err(Error::Invalid)?;
        if !ids.insert(event.id.as_str()) {
            return Err(Error::Invalid("数据包中有重复事件身份"));
        }
    }
    for event in &package.events {
        if let crate::Change::Edit {
            related: Some(id), ..
        } = &event.change
        {
            // A pruned source event can be represented explicitly as missing.
            if !ids.contains(id.as_str()) && !event.before.discontinuity {
                return Err(Error::Invalid("编辑引用缺失且未声明断裂"));
            }
        }
    }
    Ok(package.events)
}
pub fn revision(state: &Snapshot) -> Result<String> {
    Ok(digest(&serde_json::to_vec(state)?))
}
fn compare(state: &Snapshot, events: &[Event], mode: ImportMode) -> Result<Preview> {
    let mut preview = Preview {
        revision: revision(state)?,
        replaced: if matches!(mode, ImportMode::Replace) {
            state.events.len()
        } else {
            0
        },
        ..Default::default()
    };
    for event in events {
        let old = state
            .events
            .get(&event.id)
            .map(event_digest)
            .transpose()?
            .or_else(|| state.retired.get(&event.id).cloned());
        match old {
            None => preview.added += 1,
            Some(hash) if hash == event_digest(event)? => preview.duplicates += 1,
            Some(_) => preview.conflicts += 1,
        }
    }
    Ok(preview)
}
pub fn preview(store: &Store, events: &[Event], mode: ImportMode) -> Result<Preview> {
    compare(&store.snapshot()?, events, mode)
}
pub fn import(
    store: &Store,
    events: Vec<Event>,
    mode: ImportMode,
    expected: &str,
    max_mb: u32,
) -> Result<Preview> {
    let _lock = store.lock()?;
    let mut state = store.load_locked()?;
    let preview = compare(&state, &events, mode)?;
    if preview.revision != expected {
        return Err(Error::Invalid(
            "导入预览后数据有变化，请重新预览；新输入已保留",
        ));
    }
    if preview.conflicts != 0 {
        return Err(Error::Invalid("同身份不同内容冲突，未修改任何数据"));
    }
    let changed = matches!(mode, ImportMode::Replace) || preview.added > 0;
    if matches!(mode, ImportMode::Replace) {
        state.data_epoch += 1;
        state.events = BTreeMap::new();
        state.retired.clear();
    }
    for event in events {
        if !state.retired.contains_key(&event.id) {
            state.events.insert(event.id.clone(), event);
        }
    }
    if serde_json::to_vec(&state)?.len() as u64 > u64::from(max_mb) * 1024 * 1024 {
        return Err(Error::Invalid("导入后数据超过配置容量"));
    }
    if changed {
        store.replace_locked(state)?;
    }
    // Models include a data fingerprint and are rejected after a replacement;
    // additions keep the last good model until a validated update is published.
    Ok(preview)
}
