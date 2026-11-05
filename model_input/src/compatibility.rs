//! Read-only diagnostics shared by package import and explicit compatibility trials.
use crate::package::{self, Backend, Manifest, ValidatedPackage};
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

#[derive(Debug, thiserror::Error)]
pub enum CompatibilityError {
    #[error("模型包损坏或不完整：{0}；请重新准备并校验模型包")]
    Package(String),
    #[error("缺少分词器：character-step-v1 必须包含已校验的 vocabulary.json")]
    Vocabulary,
    #[error("缺少预测策略：请提供与权重绑定的评分准入策略或生成策略；不能用于字词预测")]
    Policy,
    #[error("不支持的 GGUF 架构：{0}；生成适配只支持 qwen2，评分适配只支持已验证的 qwen2/qwen3")]
    Architecture(String),
}

pub fn check(root: &Path) -> Result<ValidatedPackage, CompatibilityError> {
    let bad = |e: package::PackageError| CompatibilityError::Package(e.to_string());
    package::no_links(root, Path::new("manifest.json")).map_err(bad)?;
    let bytes = fs::File::open(root.join("manifest.json"))
        .and_then(|f| {
            let mut bytes = Vec::new();
            f.take(65537).read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .map_err(|e| CompatibilityError::Package(e.to_string()))?;
    if bytes.len() > 65536 {
        return Err(CompatibilityError::Package("清单超过 64 KiB".into()));
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|_| CompatibilityError::Package("清单格式错误".into()))?;
    if manifest.backend == Backend::OnnxRuntime {
        let name = manifest
            .vocabulary
            .as_deref()
            .ok_or(CompatibilityError::Vocabulary)?;
        let relative = package::safe_path(name).map_err(bad)?;
        package::no_links(root, relative).map_err(|e| match e {
            package::PackageError::Io(ref io) if io.kind() == io::ErrorKind::NotFound => {
                CompatibilityError::Vocabulary
            }
            _ => bad(e),
        })?;
        if !root.join(relative).is_file() {
            return Err(CompatibilityError::Vocabulary);
        }
    }
    let policy = if manifest.capabilities.proposes_words {
        &manifest.generation_policy
    } else {
        &manifest.prediction_policy
    };
    let policy = policy.as_deref().ok_or(CompatibilityError::Policy)?;
    let relative = package::safe_path(policy).map_err(bad)?;
    package::no_links(root, relative).map_err(|e| match e {
        package::PackageError::Io(ref io) if io.kind() == io::ErrorKind::NotFound => {
            CompatibilityError::Policy
        }
        _ => bad(e),
    })?;
    if !root.join(relative).is_file() {
        return Err(CompatibilityError::Policy);
    }
    let checked = package::validate(root).map_err(bad)?;
    if manifest.backend == Backend::LlamaCpp {
        let architecture = gguf_architecture(&checked.root.join(&manifest.model))
            .map_err(|_| CompatibilityError::Package("GGUF 元数据不完整或超过限制".into()))?;
        if architecture != "qwen2"
            && (architecture != "qwen3" || manifest.capabilities.proposes_words)
        {
            return Err(CompatibilityError::Architecture(architecture));
        }
    }
    Ok(checked)
}

/// Only data is read. Bound metadata work and allocation before invoking native loaders.
fn gguf_architecture(path: &Path) -> io::Result<String> {
    fn invalid() -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, "GGUF metadata")
    }
    fn u32le(r: &mut impl Read) -> io::Result<u32> {
        let mut b = [0; 4];
        r.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }
    fn u64le(r: &mut impl Read) -> io::Result<u64> {
        let mut b = [0; 8];
        r.read_exact(&mut b)?;
        Ok(u64::from_le_bytes(b))
    }
    fn skip(r: &mut impl Read, n: u64) -> io::Result<()> {
        if n > 16 * 1024 * 1024 || io::copy(&mut r.take(n), &mut io::sink())? != n {
            return Err(invalid());
        }
        Ok(())
    }
    fn string(r: &mut impl Read) -> io::Result<String> {
        let n = u64le(r)?;
        if n > 8192 {
            return Err(invalid());
        }
        let mut b = vec![0; n as usize];
        r.read_exact(&mut b)?;
        String::from_utf8(b).map_err(|_| invalid())
    }
    fn value(r: &mut impl Read, ty: u32, depth: u8) -> io::Result<()> {
        match ty {
            0 | 1 | 7 => skip(r, 1),
            2 | 3 => skip(r, 2),
            4..=6 => skip(r, 4),
            10..=12 => skip(r, 8),
            8 => {
                let n = u64le(r)?;
                skip(r, n)
            }
            9 if depth < 2 => {
                let t = u32le(r)?;
                let n = u64le(r)?;
                if n > 1_000_000 {
                    return Err(invalid());
                }
                for _ in 0..n {
                    value(r, t, depth + 1)?;
                }
                Ok(())
            }
            _ => Err(invalid()),
        }
    }
    let mut r = io::BufReader::new(fs::File::open(path)?).take(16 * 1024 * 1024);
    let mut magic = [0; 4];
    r.read_exact(&mut magic)?;
    if magic != *b"GGUF" || !(2..=3).contains(&u32le(&mut r)?) {
        return Err(invalid());
    }
    let _tensors = u64le(&mut r)?;
    let count = u64le(&mut r)?;
    if count > 65536 {
        return Err(invalid());
    }
    for _ in 0..count {
        let key = string(&mut r)?;
        let ty = u32le(&mut r)?;
        if key == "general.architecture" {
            if ty != 8 {
                return Err(invalid());
            }
            return string(&mut r);
        }
        value(&mut r, ty, 0)?;
    }
    Err(invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_metadata_key_is_rejected_before_allocation() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut bytes = b"GGUF".to_vec();
        bytes.extend(3u32.to_le_bytes());
        bytes.extend(0u64.to_le_bytes());
        bytes.extend(1u64.to_le_bytes());
        bytes.extend(u64::MAX.to_le_bytes());
        fs::write(file.path(), bytes).unwrap();
        assert_eq!(
            gguf_architecture(file.path()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
