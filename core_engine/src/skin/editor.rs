//! Lossless package drafts. Export never overwrites a source or an installed version.
use super::{parameters, Error, Package};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Draft {
    pub package: Package,
    pub dirty: bool,
    source: Option<(PathBuf, Vec<u8>)>,
}
impl Draft {
    pub fn builtin(name: &str) -> Result<Self, Error> {
        let mut package = parameters::builtin_package(name)?;
        package.metadata.id.push_str(".copy");
        package.metadata.name.push_str(" 自定义副本");
        Ok(Self {
            package,
            dirty: true,
            source: None,
        })
    }
    pub fn open(path: &Path) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(super::archive::MAX_ARCHIVE + 1)
            .read_to_end(&mut bytes)?;
        let package = Package::read(Cursor::new(&bytes))?;
        Ok(Self {
            package,
            dirty: false,
            source: Some((path.to_owned(), bytes)),
        })
    }
    pub fn check_source(&self) -> Result<(), Error> {
        if let Some((path, bytes)) = &self.source {
            if std::fs::metadata(path)?.len() != bytes.len() as u64
                || std::fs::read(path)? != *bytes
            {
                return Err(Error::Invalid(
                    "源包被外部修改，请重新打开或另建副本后导出".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn export_new(&self, path: &Path) -> Result<(), Error> {
        self.check_source()?;
        self.package.export_new(path)
    }
    pub fn add_platform(&mut self, platform: &str) -> Result<(), Error> {
        if !matches!(platform, "windows" | "android") {
            return Err(Error::Invalid("未知平台".into()));
        }
        if self.package.platforms.contains_key(platform) {
            return Ok(());
        }
        let built = parameters::builtin_package("jade")?;
        self.package
            .metadata
            .platforms
            .insert(platform.into(), format!("{platform}/metadata.toml"));
        self.package
            .platforms
            .insert(platform.into(), built.platforms[platform].clone());
        self.package.files.extend(
            built
                .files
                .into_iter()
                .filter(|(p, _)| p.starts_with(&format!("{platform}/"))),
        );
        self.dirty = true;
        Ok(())
    }
    pub fn remove_platform(&mut self, platform: &str) -> Result<(), Error> {
        if self.package.platforms.len() <= 1 {
            return Err(Error::Invalid("至少保留一个平台入口".into()));
        }
        self.package.platforms.remove(platform);
        self.package.metadata.platforms.remove(platform);
        self.package
            .files
            .retain(|p, _| !p.starts_with(&format!("{platform}/")));
        self.dirty = true;
        Ok(())
    }
    pub fn copy_shared_resource(&mut self, id: &str, platform: &str) -> Result<String, Error> {
        let path = self
            .package
            .metadata
            .resources
            .get(id)
            .ok_or_else(|| Error::Invalid("共享资源不存在".into()))?;
        let bytes = self
            .package
            .files
            .get(path)
            .ok_or_else(|| Error::Invalid("资源文件不存在".into()))?
            .clone();
        let extension = Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin");
        let target = format!("assets/{id}.{extension}");
        let descriptor = self
            .package
            .platforms
            .get_mut(platform)
            .ok_or_else(|| Error::Invalid("平台不存在".into()))?;
        if descriptor.resources.contains_key(id)
            || self
                .package
                .files
                .contains_key(&format!("{platform}/{target}"))
        {
            return Err(Error::Invalid("平台资源副本已存在".into()));
        }
        descriptor.resources.insert(id.into(), target.clone());
        self.package
            .files
            .insert(format!("{platform}/{target}"), bytes);
        self.dirty = true;
        Ok(format!("platform:{id}"))
    }
}
