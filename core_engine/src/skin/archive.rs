//! Bounded ZIP IO and immutable on-disk versions. No extraction or activation occurs.
use super::{valid_path, Error, Package};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read, Seek, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

pub const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
pub const MAX_EXPANDED: u64 = 128 * 1024 * 1024;
pub const MAX_ENTRY: u64 = 32 * 1024 * 1024;
pub const MAX_METADATA: u64 = 1024 * 1024;
pub const MAX_ENTRIES: usize = 512;

impl Package {
    pub fn read(reader: impl Read + Seek) -> Result<Self, Error> {
        let mut reader = reader;
        if reader.seek(std::io::SeekFrom::End(0))? > MAX_ARCHIVE {
            return Err(Error::Invalid("压缩包超过 64 MiB".into()));
        }
        reader.rewind()?;
        let mut zip = ZipArchive::new(reader)?;
        if zip.len() > MAX_ENTRIES {
            return Err(Error::Invalid("ZIP 条目超过 512".into()));
        }
        let mut files = BTreeMap::new();
        let mut names = BTreeSet::new();
        let mut directories = Vec::new();
        let mut total = 0;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            let name = entry.name().to_owned();
            let path = name.strip_suffix('/').unwrap_or(&name);
            let kind = entry.unix_mode().unwrap_or(0) & 0o170000;
            if !valid_path(path)
                || !names.insert(path.to_lowercase())
                || (kind != 0 && kind != 0o100000 && kind != 0o040000)
                || (kind == 0o040000 && !entry.is_dir())
                || (kind == 0o100000 && entry.is_dir())
            {
                return Err(Error::Invalid(format!(
                    "ZIP 路径重复、链接或类型非法：{name}"
                )));
            }
            if entry.is_dir() {
                directories.push(path.to_owned());
                continue;
            }
            let limit = if name.ends_with(".toml") {
                MAX_METADATA
            } else {
                MAX_ENTRY
            };
            if entry.size() > limit {
                return Err(Error::Invalid(format!("条目超限：{name}")));
            }
            let mut bytes = Vec::new();
            (&mut entry).take(limit + 1).read_to_end(&mut bytes)?;
            total += bytes.len() as u64;
            if bytes.len() as u64 > limit || total > MAX_EXPANDED {
                return Err(Error::Invalid("皮肤解压大小超限".into()));
            }
            files.insert(name, bytes);
        }
        let package = Self::from_files(files)?;
        package.validate_archive_files()?;
        let root = package
            .metadata
            .capabilities
            .check(&["container.zip/1".into()].into());
        if !root.enabled {
            return Err(Error::Invalid(root.reasons.join("；")));
        }
        for directory in directories {
            let allowed = directory == "shared"
                || directory == "shared/assets"
                || directory.starts_with("shared/assets/")
                || package
                    .metadata
                    .platforms
                    .keys()
                    .any(|p| directory == *p || directory.starts_with(&format!("{p}/")));
            if !allowed
                || package.files.keys().any(|p| {
                    directory
                        .to_lowercase()
                        .starts_with(&format!("{}/", p.to_lowercase()))
                })
            {
                return Err(Error::Invalid(format!("非法 ZIP 目录：{directory}")));
            }
        }
        Ok(package)
    }

    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::read(fs::File::open(path)?)
    }

    /// Regenerate only known metadata documents. Unknown component files remain byte-identical.
    pub fn encoded(&self) -> Result<Vec<u8>, Error> {
        let mut files = self.files.clone();
        files.insert(
            "metadata.toml".into(),
            toml::to_string(&self.metadata)?.into_bytes(),
        );
        for (name, platform) in &self.platforms {
            files.insert(
                format!("{name}/metadata.toml"),
                toml::to_string(platform)?.into_bytes(),
            );
        }
        let validated = Self::from_files(files)?;
        validated.validate_archive_files()?;
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        for (name, bytes) in &validated.files {
            writer.start_file(name, options)?;
            writer.write_all(bytes)?;
        }
        let result = writer.finish()?.into_inner();
        if result.len() as u64 > MAX_ARCHIVE {
            return Err(Error::Invalid("压缩包超过 64 MiB".into()));
        }
        Ok(result)
    }

    /// Export to a new file. Existing files require an explicit caller-side save workflow.
    pub fn export_new(&self, target: &Path) -> Result<(), Error> {
        let bytes = self.encoded()?;
        let parent = target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        temp.persist_noclobber(target)
            .map_err(|e| Error::Io(e.error))?;
        Ok(())
    }

    fn validate_archive_files(&self) -> Result<(), Error> {
        if self.files.len() > MAX_ENTRIES {
            return Err(Error::Invalid("ZIP 文件超过 512".into()));
        }
        let mut total = 0;
        for (path, bytes) in &self.files {
            let allowed = path == "metadata.toml"
                || path.starts_with("shared/assets/")
                || self
                    .metadata
                    .platforms
                    .keys()
                    .any(|p| path.starts_with(&format!("{p}/")));
            let limit = if path.ends_with(".toml") {
                MAX_METADATA
            } else {
                MAX_ENTRY
            };
            total += bytes.len() as u64;
            if !allowed || bytes.len() as u64 > limit || total > MAX_EXPANDED {
                return Err(Error::Invalid(format!("目录结构或大小非法：{path}")));
            }
        }
        Ok(())
    }
}

/// Import result is a version location, not an active skin selection.
pub struct Imported {
    pub path: PathBuf,
    pub already_present: bool,
}

/// Keeps every installed version immutable, so active readers and rollback remain valid.
pub fn import(store: &Path, reader: impl Read + Seek) -> Result<Imported, Error> {
    let package = Package::read(reader)?;
    fs::create_dir_all(store)?;
    let base = fs::canonicalize(store)?;
    // One flat filename avoids following package-controlled subdirectory links.
    let filename = format!(
        "{}@{}.mywubi-skin",
        package.metadata.id, package.metadata.version
    );
    let target = base.join(filename);
    if target.exists() {
        return existing(&package, &target);
    }
    match package.export_new(&target) {
        Ok(()) => Ok(Imported {
            path: target,
            already_present: false,
        }),
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            existing(&package, &target)
        }
        Err(e) => Err(e),
    }
}

fn existing(package: &Package, target: &Path) -> Result<Imported, Error> {
    if fs::symlink_metadata(target)?.file_type().is_symlink() {
        return Err(Error::Invalid("版本库目标是链接".into()));
    }
    let old = Package::open(target)?;
    if old.encoded()? != package.encoded()? {
        return Err(Error::Conflict(
            package.metadata.id.clone(),
            package.metadata.version.clone(),
        ));
    }
    Ok(Imported {
        path: target.to_owned(),
        already_present: true,
    })
}
