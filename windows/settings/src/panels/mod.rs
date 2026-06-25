//! 配置面板分发。

pub mod about;
pub mod appearance;
pub mod basic;
pub mod dictionary;
pub mod tip_manager;

use crate::state::{AppState, Panel};
use eframe::egui::Ui;

/// 渲染当前激活面板。
pub fn show_active(ui: &mut Ui, state: &mut AppState) {
    match state.active_panel {
        Panel::Basic => basic::show(ui, state),
        Panel::Appearance => appearance::show(ui, state),
        Panel::Dictionary => dictionary::show(ui, state),
        Panel::TipManager => tip_manager::show(ui, state),
        Panel::About => about::show(ui, state),
    }
}