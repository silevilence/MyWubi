//! Explicit, file-based automation using the same sessions as the desktop UI.
//! No TIP registration, worker startup, network access or native file dialogs.
use crate::{
    skin_editor::SkinEditor,
    state::{AppState, Panel},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Application {
    Settings,
    SkinEditor,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub config: PathBuf,
    pub actions: Vec<Action>,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Inspect,
    Navigate {
        page: String,
    },
    Set {
        field: String,
        value: serde_json::Value,
    },
    Save,
    Reload {
        #[serde(default)]
        discard: bool,
    },
    SelectTable {
        path: PathBuf,
    },
    SkinNew {
        preset: String,
    },
    SkinOpen {
        path: PathBuf,
    },
    SkinSet {
        platform: String,
        component: String,
        #[serde(default)]
        dark: bool,
        field: String,
        value: serde_json::Value,
    },
    SkinExport {
        path: PathBuf,
    },
    SkinApply,
    SkinDiscard,
}

#[derive(Serialize)]
pub struct Report {
    pub ok: bool,
    pub results: Vec<serde_json::Value>,
    pub error: Option<String>,
}

pub fn panel(name: &str) -> Result<Panel, String> {
    match name {
        "basic" => Ok(Panel::Basic),
        "appearance" => Ok(Panel::Appearance),
        "dictionary" => Ok(Panel::Dictionary),
        "special_input" => Ok(Panel::SpecialInput),
        "intelligence" => Ok(Panel::Intelligence),
        "tip_manager" => Ok(Panel::TipManager),
        "about" => Ok(Panel::About),
        _ => Err(format!("未知页面：{name}")),
    }
}

/// Set an existing scalar field, rejecting misspellings and invalid values before mutation.
pub fn set_field(
    state: &mut AppState,
    field: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let mut config = toml::Value::try_from(&state.config).map_err(|e| e.to_string())?;
    let mut target = &mut config;
    for key in field.split('.') {
        target = target
            .get_mut(key)
            .ok_or_else(|| format!("配置字段不存在：{field}"))?;
    }
    if target.is_table() || target.is_array() {
        return Err("请指定具体配置字段".into());
    }
    *target = toml::Value::try_from(value).map_err(|e| e.to_string())?;
    let config =
        core_engine::Config::from_str(&toml::to_string(&config).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if field == "dictionary.system_table" {
        state
            .tables
            .select(config.dictionary.system_table.clone(), &mut state.config)?;
    } else {
        state.config = config;
    }
    state.mark_dirty();
    Ok(())
}

pub fn execute(request: Request, application: Application) -> Report {
    let mut report = Report {
        ok: true,
        results: Vec::new(),
        error: None,
    };
    if !request.config.is_absolute() {
        report.ok = false;
        report.error = Some("config 必须是明确的绝对路径".into());
        return report;
    }
    let mut state = AppState::load_isolated(request.config);
    if let Some(error) = &state.load_error {
        report.ok = false;
        report.error = Some(error.message.clone());
        return report;
    }
    let mut skin = SkinEditor::default();
    for (index, action) in request.actions.into_iter().enumerate() {
        let result = (|| -> Result<(), String> {
            let skin_action = matches!(
                action,
                Action::SkinNew { .. }
                    | Action::SkinOpen { .. }
                    | Action::SkinSet { .. }
                    | Action::SkinExport { .. }
                    | Action::SkinApply
                    | Action::SkinDiscard
            );
            if !matches!(action, Action::Inspect)
                && skin_action != (application == Application::SkinEditor)
            {
                return Err("操作与当前程序不匹配".into());
            }
            match action {
                Action::Inspect => {}
                Action::Navigate { page } => state.active_panel = panel(&page)?,
                Action::Set { field, value } => set_field(&mut state, &field, value)?,
                Action::Save => {
                    if !crate::save::save(&mut state) {
                        return Err(state
                            .status_msg
                            .clone()
                            .unwrap_or_else(|| "保存失败".into()));
                    }
                }
                Action::Reload { discard } => state.reload(discard)?,
                Action::SelectTable { path } => {
                    state.tables.select(path, &mut state.config)?;
                    state.mark_dirty();
                }
                Action::SkinNew { preset } => skin.create(&preset)?,
                Action::SkinOpen { path } => skin.open_file(&path)?,
                Action::SkinSet {
                    platform,
                    component,
                    dark,
                    field,
                    value,
                } => skin.set_parameter(
                    &platform,
                    &component,
                    dark,
                    &field,
                    toml::Value::try_from(value).map_err(|e| e.to_string())?,
                )?,
                Action::SkinExport { path } => skin.export_file(&path)?,
                Action::SkinApply => {
                    skin.apply(&state.config_path)?;
                    state.reload(false)?;
                }
                Action::SkinDiscard => skin.discard(),
            }
            Ok(())
        })();
        report.results.push(serde_json::json!({ "index": index, "ok": result.is_ok(), "page": format!("{:?}",state.active_panel), "dirty": state.has_unsaved_changes(), "config": state.config, "skin_dirty": skin.is_dirty(), "skin_title": skin.title() }));
        if let Err(error) = result {
            report.ok = false;
            report.error = Some(error);
            break;
        }
    }
    report
}

/// `--automation request.json --output report.json` runs before any desktop initialization.
pub fn run_cli(application: Application) -> Option<i32> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !args.iter().any(|a| a == "--automation") {
        return None;
    }
    let result = (|| -> Result<bool, String> {
        if args.len() != 4 || args[0] != "--automation" || args[2] != "--output" {
            return Err("用法：--automation request.json --output report.json".into());
        }
        let input = Path::new(&args[1]);
        let output = Path::new(&args[3]);
        let request: Request =
            serde_json::from_slice(&std::fs::read(input).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        // Reports must be new files: never overwrite a config, source package or prior evidence.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|e| e.to_string())?;
        let report = execute(request, application);
        serde_json::to_writer_pretty(&mut file, &report).map_err(|e| e.to_string())?;
        Ok(report.ok)
    })();
    Some(match result {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(error) => {
            eprintln!("{error}");
            2
        }
    })
}
