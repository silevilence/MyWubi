//! Reproducible author templates. Output must be a new directory or have no same filenames.
use core_engine::skin::{
    editor::Draft,
    images::{Mode, Style},
    parameters::Appearance,
    Package,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: skin_templates OUTPUT_DIRECTORY")?;
    std::fs::create_dir_all(&output)?;
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/skins/assets/bamboo-paper.png");
    let mut draft = Draft::builtin("jade")?;
    draft.package.metadata.id = "mywubi.bamboo".into();
    draft.package.metadata.name = "竹纸".into();
    draft.import_png(None, "bamboo", std::fs::read(source)?)?;
    for platform in ["windows", "android"] {
        draft
            .package
            .platforms
            .get_mut(platform)
            .unwrap()
            .components[0]
            .capabilities
            .required
            .push("appearance.images/1".into());
        let path = format!("{platform}/appearance.toml");
        let mut appearance: Appearance =
            toml::from_str(std::str::from_utf8(&draft.package.files[&path])?)?;
        let variants: BTreeMap<_, _> = [("light", 0.48), ("dark", 0.04)]
            .into_iter()
            .map(|(name, opacity)| {
                (
                    name.to_string(),
                    BTreeMap::from([(
                        "background".to_string(),
                        Style {
                            resource: "shared:bamboo".into(),
                            mode: Mode::NineSlice,
                            slice: [160; 4],
                            content: if platform == "windows" {
                                [12, 10, 12, 10]
                            } else {
                                [4; 4]
                            },
                            opacity,
                        },
                    )]),
                )
            })
            .collect();
        appearance
            .extensions
            .insert("images".into(), toml::Value::try_from(variants)?);
        draft
            .package
            .files
            .insert(path, toml::to_string(&appearance)?.into_bytes());
    }
    draft.export_new(&output.join("bamboo-dual.mywubi-skin"))?;
    for platform in ["windows", "android"] {
        let mut single = draft.clone();
        single.remove_platform(if platform == "windows" {
            "android"
        } else {
            "windows"
        })?;
        single.package.metadata.id = format!("mywubi.bamboo.{platform}");
        single.export_new(&output.join(format!("bamboo-{platform}.mywubi-skin")))?;
    }
    core_engine::skin::parameters::builtin_package("plain")?
        .export_new(&output.join("parameters-dual.mywubi-skin"))?;
    let mut future = draft.package.clone();
    future.metadata.id = "example.future-required".into();
    future.platforms.get_mut("android").unwrap().components[0]
        .capabilities
        .required
        .push("appearance.future/9".into());
    future.export_new(&output.join("compat-future-required.mywubi-skin"))?;
    let mut optional = draft.package.clone();
    optional.metadata.id = "example.optional-fallback".into();
    for platform in optional.platforms.values_mut() {
        let caps = &mut platform.components[0].capabilities;
        caps.required.retain(|v| v != "appearance.images/1");
        caps.optional
            .insert("appearance.images/1".into(), "plain".into());
    }
    optional.export_new(&output.join("compat-optional-plain.mywubi-skin"))?;
    let mut damaged = draft.package;
    damaged.metadata.id = "example.damaged-png".into();
    damaged.files.get_mut("shared/assets/bamboo.png").unwrap()[45] ^= 0xff;
    damaged.export_new(&output.join("invalid-damaged-png.mywubi-skin"))?;
    // Exercise the same parser as importers for all structurally valid outputs.
    for entry in std::fs::read_dir(&output)? {
        let path = entry?.path();
        if path.extension().is_some_and(|v| v == "mywubi-skin") {
            Package::open(&path)?;
        }
    }
    Ok(())
}
