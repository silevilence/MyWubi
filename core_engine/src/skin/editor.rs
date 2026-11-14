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
        self.validate_images()?;
        self.validate_keyboards()?;
        self.package.export_new(path)
    }
    pub fn validate_images(&self) -> Result<(), Error> {
        for (platform, descriptor) in &self.package.platforms {
            for component in descriptor
                .components
                .iter()
                .filter(|c| c.kind == "appearance")
            {
                let appearance: parameters::Appearance = super::parse(
                    &self.package.files,
                    &format!("{platform}/{}", component.path),
                )?;
                // Unknown image versions remain losslessly read-only in the editor.
                if appearance.extensions.contains_key("images")
                    && (component
                        .capabilities
                        .required
                        .iter()
                        .any(|c| c == super::images::CAPABILITY)
                        || component
                            .capabilities
                            .optional
                            .contains_key(super::images::CAPABILITY))
                {
                    for variant in appearance.variants.keys() {
                        super::images::resolve(
                            &self.package,
                            platform,
                            component,
                            &appearance,
                            variant,
                            &[super::images::CAPABILITY.into()].into(),
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
    /// Stage one layout and its mapping, validating before replacing any package bytes.
    pub fn update_keyboard(
        &mut self,
        id: &str,
        layout: &super::keyboard::Layout,
        mapping: Option<&super::keyboard::Mapping>,
    ) -> Result<(), Error> {
        let mut next = self.package.clone();
        let component = next
            .platforms
            .get("android")
            .and_then(|p| {
                p.components
                    .iter()
                    .find(|c| c.id == id && c.kind == "layout")
            })
            .cloned()
            .ok_or_else(|| Error::Invalid("布局组件不存在".into()))?;
        next.files.insert(
            format!("android/{}", component.path),
            toml::to_string(layout)?.into_bytes(),
        );
        if let Some(mapping) = mapping {
            let id = layout
                .mapping
                .as_ref()
                .ok_or_else(|| Error::Invalid("布局未声明映射".into()))?;
            let declaration = next.platforms["android"]
                .components
                .iter()
                .find(|c| &c.id == id && c.kind == "mapping")
                .ok_or_else(|| Error::Invalid("映射组件不存在".into()))?;
            next.files.insert(
                format!("android/{}", declaration.path),
                toml::to_string(mapping)?.into_bytes(),
            );
        }
        let (checked, mapping) = super::keyboard::layout(&next, &component)?;
        super::combination::validate_layout(&checked, mapping.as_ref())?;
        if let Some(id) = &checked.recommended_appearance {
            super::combination::validate_layout_appearance(&next, id)?;
        }
        let states = super::keyboard::compatibility(&next, &super::combination::supported())?;
        if !states[id].enabled {
            return Err(Error::Invalid(states[id].reasons.join("；")));
        }
        self.package = next;
        self.dirty = true;
        Ok(())
    }
    pub fn validate_keyboards(&self) -> Result<(), Error> {
        if let Some(platform) = self.package.platforms.get("android") {
            super::keyboard::compatibility(&self.package, &super::combination::supported())?;
            for c in platform.components.iter().filter(|c| {
                c.kind == "layout"
                    && c.capabilities
                        .required
                        .iter()
                        .all(|cap| super::combination::supported().contains(cap))
            }) {
                let (layout, mapping) = super::keyboard::layout(&self.package, c)?;
                super::combination::validate_layout(&layout, mapping.as_ref())?;
            }
        }
        Ok(())
    }
    pub fn import_png(
        &mut self,
        platform: Option<&str>,
        id: &str,
        bytes: Vec<u8>,
    ) -> Result<(), Error> {
        if !super::valid_id(id) {
            return Err(Error::Invalid("资源 ID 无效".into()));
        }
        super::images::dimensions(&bytes)?;
        let path = if let Some(platform) = platform {
            let descriptor = self
                .package
                .platforms
                .get_mut(platform)
                .ok_or_else(|| Error::Invalid("平台不存在".into()))?;
            let path = format!("assets/{id}.png");
            descriptor.resources.insert(id.into(), path.clone());
            format!("{platform}/{path}")
        } else {
            let path = format!("shared/assets/{id}.png");
            self.package
                .metadata
                .resources
                .insert(id.into(), path.clone());
            path
        };
        self.package.files.insert(path, bytes);
        self.dirty = true;
        Ok(())
    }
    pub fn remove_resource(&mut self, platform: Option<&str>, id: &str) -> Result<(), Error> {
        let reference = format!(
            "{}:{id}",
            if platform.is_some() {
                "platform"
            } else {
                "shared"
            }
        );
        let scope = platform.map(|p| format!("{p}/"));
        if self.package.files.iter().any(|(path, bytes)| {
            scope.as_ref().is_none_or(|scope| path.starts_with(scope))
                && std::str::from_utf8(bytes).is_ok_and(|s| s.contains(&reference))
        }) {
            return Err(Error::Invalid(
                "资源仍被引用，请先在相关平台/变体重新绑定".into(),
            ));
        }
        let path = if let Some(platform) = platform {
            self.package
                .platforms
                .get_mut(platform)
                .and_then(|p| p.resources.remove(id))
                .map(|p| format!("{platform}/{p}"))
        } else {
            self.package.metadata.resources.remove(id)
        };
        if let Some(path) = path {
            // A second resource ID may intentionally alias the same file.
            let shared = self.package.metadata.resources.values().any(|p| p == &path);
            let private = self.package.platforms.iter().any(|(platform, p)| {
                p.resources
                    .values()
                    .any(|v| format!("{platform}/{v}") == path)
            });
            if !shared && !private {
                self.package.files.remove(&path);
            }
            self.dirty = true;
        }
        Ok(())
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
