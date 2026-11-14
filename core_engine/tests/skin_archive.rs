use core_engine::skin::{
    archive::{self, MAX_METADATA},
    Error, Package,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};
use zip::{write::SimpleFileOptions, ZipWriter};

fn package(platforms: &[&str]) -> Package {
    let mut root = "format=1\nid='fixture'\nversion='1'\nname='Fixture'\nauthor='MyWubi'\nlicense='MIT'\n[platforms]\n".to_owned();
    let mut files = BTreeMap::new();
    for platform in platforms {
        root.push_str(&format!("{platform}='{platform}/metadata.toml'\n"));
        files.insert(format!("{platform}/metadata.toml"), format!("platform='{platform}'\n[[components]]\nid='future'\nversion='1'\nkind='unknown'\npath='future.toml'\n").into_bytes());
        files.insert(
            format!("{platform}/future.toml"),
            b"# keep this comment\nresource='shared:image'\n".to_vec(),
        );
    }
    root.push_str("[resources]\nimage='shared/assets/image.bin'\n");
    files.insert("metadata.toml".into(), root.into_bytes());
    files.insert("shared/assets/image.bin".into(), vec![0, 1, 2, 3]);
    Package::from_files(files).unwrap()
}

fn zip_with(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn single_and_dual_platform_packages_preserve_unknown_bytes_and_share_one_resource() {
    for platforms in [vec!["windows"], vec!["android"], vec!["windows", "android"]] {
        let original = package(&platforms);
        let bytes = original.encoded().unwrap();
        let loaded = Package::read(Cursor::new(bytes)).unwrap();
        for platform in &platforms {
            let key = format!("{platform}/future.toml");
            assert_eq!(loaded.files[&key], original.files[&key]);
        }
        assert_eq!(
            loaded
                .files
                .keys()
                .filter(|p| p.starts_with("shared/"))
                .count(),
            1
        );
        assert_eq!(loaded.encoded().unwrap(), original.encoded().unwrap());
    }
}

#[test]
fn import_is_immutable_idempotent_and_failed_import_keeps_installed_version() {
    let store = tempfile::tempdir().unwrap();
    let mut package = package(&["android"]);
    let bytes = package.encoded().unwrap();
    let imported = archive::import(store.path(), Cursor::new(&bytes)).unwrap();
    assert!(!imported.already_present);
    assert!(
        archive::import(store.path(), Cursor::new(&bytes))
            .unwrap()
            .already_present
    );
    package.metadata.name = "changed".into();
    assert!(matches!(
        archive::import(store.path(), Cursor::new(package.encoded().unwrap())),
        Err(Error::Conflict(_, _))
    ));
    assert!(archive::import(store.path(), Cursor::new(b"broken")).is_err());
    assert_eq!(std::fs::read(&imported.path).unwrap(), bytes);
    package.metadata.version = "2".into();
    archive::import(store.path(), Cursor::new(package.encoded().unwrap())).unwrap();
    assert!(imported.path.exists());
    assert_eq!(std::fs::read_dir(store.path()).unwrap().count(), 2);
}

#[test]
fn traversal_case_alias_directory_conflicts_and_metadata_limit_are_rejected() {
    for name in ["../escape", "metadata.TOML", "android", "other/file"] {
        let p = package(&["android"]);
        let mut entries: Vec<_> = p
            .files
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect();
        entries.push((name, vec![1]));
        assert!(
            Package::read(Cursor::new(zip_with(&entries))).is_err(),
            "{name}"
        );
    }
    let bytes = zip_with(&[("metadata.toml", vec![b' '; MAX_METADATA as usize + 1])]);
    assert!(Package::read(Cursor::new(bytes)).is_err());
}

#[test]
fn symlinks_are_rejected_without_extracting() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink(
            "shared/assets/link",
            "C:/private",
            SimpleFileOptions::default(),
        )
        .unwrap();
    assert!(Package::read(Cursor::new(writer.finish().unwrap().into_inner())).is_err());
}

#[test]
fn export_never_overwrites_existing_target() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.zip");
    std::fs::write(&path, b"original").unwrap();
    assert!(package(&["windows"]).export_new(&path).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
