//! Device preferences are configuration, never bundled into a distributed skin.
use super::{parameters, valid_id, Error, Extensions};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkinConfig {
    pub windows: Preference,
    pub android: Preference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preference {
    pub preset: String,
    pub mode: Mode,
    pub selection: Option<Selection>,
    pub use_skin_metrics: bool,
    pub overrides: Extensions,
}

impl Default for Preference {
    fn default() -> Self {
        Self {
            preset: "legacy".into(),
            mode: Mode::System,
            selection: None,
            use_skin_metrics: false,
            overrides: Extensions::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub id: String,
    pub version: String,
    pub component: String,
}

impl Preference {
    pub fn validate(&self) -> Result<(), Error> {
        if !matches!(
            self.preset.as_str(),
            "legacy" | "jade" | "plain" | "contrast"
        ) || self
            .selection
            .as_ref()
            .is_some_and(|s| !valid_id(&s.id) || !valid_id(&s.version) || !valid_id(&s.component))
        {
            return Err(Error::Invalid("皮肤选择身份无效".into()));
        }
        parameters::validate_patch(&self.overrides)
    }

    pub fn dark(&self, system_dark: bool) -> bool {
        match self.mode {
            Mode::System => system_dark,
            Mode::Light => false,
            Mode::Dark => true,
        }
    }
}
