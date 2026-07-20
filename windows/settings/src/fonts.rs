//! 内嵌中文字体加载，防止 egui 默认字体导致豆腐块乱码。

use eframe::egui::{FontData, FontDefinitions, FontFamily};

/// 内嵌的 Noto Sans SC 子集（GB2312 常用字 + ASCII）。
const NOTO_SANS_SC: &[u8] = include_bytes!("../assets/fonts/noto_sans_sc_subset.ttf");

/// 将内嵌中文字体注入 egui 的 FontDefinitions，设为最高优先级。
pub fn load_chinese_fonts(ctx: &eframe::egui::Context) {
    // 占位字体为空时跳过注入，release 前需替换为真实子集。
    if NOTO_SANS_SC.len() < 1000 {
        log::warn!("字体为空占位，中文可能显示为豆腐块");
        return;
    }
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "noto_sans_sc".to_owned(),
        FontData::from_static(NOTO_SANS_SC),
    );
    // 插入到 Proportional 和 Monospace 的最高优先级
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "noto_sans_sc".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "noto_sans_sc".to_owned());
    ctx.set_fonts(fonts);
}
