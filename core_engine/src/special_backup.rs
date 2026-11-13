//! Portable settings-side backup. Restore stages private resources and returns a draft;
//! it never activates a runtime, writes config, or persists environment availability.
use crate::{
    expressions::{ExpressionKind, ExpressionLibrary, ExpressionsConfig},
    shortcuts::{self, ShortcutConfig},
    Config,
};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path};

pub const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAGIC: &str = "MYWUBI-SPECIAL-INPUT-1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    magic: String,
    shortcuts: ShortcutConfig,
    resources: Vec<Resource>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Resource {
    kind: ExpressionKind,
    append: bool,
    search: bool,
    library: String,
}

/// All six selected resources are included, even if their switches are off.
pub fn export(path: &Path, config: &Config, target: &Path) -> Result<(), String> {
    let rules =
        shortcuts::validate_structure(&config.shortcuts.rules).map_err(|e| e.to_string())?;
    let resources = ExpressionKind::ALL
        .into_iter()
        .map(|kind| {
            let selected = config.expressions.selection(kind);
            let library = ExpressionLibrary::load(&selected.path(path, kind), kind)
                .map_err(|e| format!("{}无法备份：{e}", kind.label()))?;
            Ok(Resource {
                kind,
                append: selected.append,
                search: selected.search,
                library: library.encode().map_err(|e| e.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bytes = serde_json::to_vec(&Archive {
        magic: MAGIC.into(),
        shortcuts: ShortcutConfig { rules },
        resources,
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("特殊输入备份超过 512 MiB".into());
    }
    crate::data::atomic_write(target, &bytes).map_err(|e| e.to_string())
}

pub fn restore(path: &Path, config: &Config, source: &Path) -> Result<Config, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(source)
        .and_then(|file| file.take(MAX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("特殊输入备份超过 512 MiB".into());
    }
    let archive: Archive =
        serde_json::from_slice(&bytes).map_err(|e| format!("备份格式无效：{e}"))?;
    if archive.magic != MAGIC || archive.resources.len() != ExpressionKind::ALL.len() {
        return Err("不是完整的 MyWubi 特殊输入备份（版本 1）".into());
    }
    let rules =
        shortcuts::validate_structure(&archive.shortcuts.rules).map_err(|e| e.to_string())?;
    // Validate every entry before creating any import generation.
    let libraries = ExpressionKind::ALL
        .into_iter()
        .map(|kind| {
            let matches: Vec<_> = archive
                .resources
                .iter()
                .filter(|r| r.kind == kind)
                .collect();
            if matches.len() != 1 {
                return Err(format!("备份中的{}缺失或重复", kind.label()));
            }
            let resource = matches[0];
            let library =
                ExpressionLibrary::parse(&resource.library, kind).map_err(|e| e.to_string())?;
            Ok((kind, resource, library))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut expressions = ExpressionsConfig::default();
    for (kind, resource, library) in libraries {
        let imported = crate::expression_store::import_library(path, kind, &library)?;
        let selected = expressions.selection_mut(kind);
        selected.append = resource.append;
        selected.search = resource.search;
        selected.resource = Some(
            imported
                .path
                .strip_prefix(path.parent().unwrap_or(Path::new(".")))
                .unwrap_or(&imported.path)
                .to_owned(),
        );
    }
    let mut draft = config.clone();
    draft.expressions = expressions;
    draft.shortcuts = ShortcutConfig { rules };
    Ok(draft)
}
