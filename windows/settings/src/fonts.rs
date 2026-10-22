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
    // Cargo 中保留 default_fonts：中文字体缺少的省略号和状态图标需要这些后备字体。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_labels_have_glyphs() {
        let ctx = eframe::egui::Context::default();
        load_chinese_fonts(&ctx);
        ctx.begin_frame(Default::default());
        let labels = "管理当前码表…管理自造词…全拼固定a～z，无万能键；分隔符为单引号，不计入字母长度。一次性临时输入主码表反查拼音英文⚠✅❌🆕";
        let missing: Vec<_> = ctx.fonts(|fonts| {
            [FontFamily::Proportional, FontFamily::Monospace]
                .into_iter()
                .flat_map(|family| {
                    let id = eframe::egui::FontId::new(14.0, family);
                    labels
                        .chars()
                        .filter(move |c| !c.is_whitespace() && !fonts.has_glyph(&id, *c))
                })
                .collect()
        });
        let _ = ctx.end_frame();
        assert!(missing.is_empty(), "配置界面缺失字形: {missing:?}");
    }
}
