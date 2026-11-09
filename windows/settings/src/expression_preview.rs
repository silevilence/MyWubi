//! System-font preview: egui's built-in font atlas cannot shape color Emoji sequences.
use eframe::egui;
use text_render::{coverage::inspect, Draw, TextSystem};

pub struct ExpressionPreview {
    text: String,
    emoji: bool,
    renderer: Option<TextSystem>,
    texture: Option<egui::TextureHandle>,
    key: Option<(String, bool, u32, bool)>,
    message: String,
}
impl Default for ExpressionPreview {
    fn default() -> Self {
        Self {
            text: "🚳".into(),
            emoji: true,
            renderer: None,
            texture: None,
            key: None,
            message: String::new(),
        }
    }
}
impl ExpressionPreview {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("表情显示预览", |ui| {
            ui.small("使用与候选框相同的系统字体渲染。粘贴单个 Emoji 或一条颜文字检查显示；预览不修改词库。宿主应用可能使用不同字体。");
            ui.horizontal_wrapped(|ui| {
                for (label, text, emoji) in [("禁止自行车", "🚳", true), ("家庭", "👨‍👩‍👧‍👦", true),
                    ("旗帜", "🇨🇳", true), ("肤色与性别", "👩🏽‍⚕️", true), ("键帽", "1️⃣", true),
                    ("颜文字", "(╯°□°）╯︵ ┻━┻", false), ("组合附加符", "(๑•̀ㅂ•́)و✧", false)] {
                    if ui.button(label).clicked() { self.text = text.into(); self.emoji = emoji; }
                }
            });
            ui.checkbox(&mut self.emoji, "按单个 Emoji 组合检查");
            ui.add(egui::TextEdit::singleline(&mut self.text).char_limit(1024).desired_width(f32::INFINITY));
            let scale = ui.ctx().pixels_per_point();
            let dark = ui.visuals().dark_mode;
            let key = (self.text.clone(), self.emoji, scale.to_bits(), dark);
            if self.key.as_ref() != Some(&key) {
                self.texture = None;
                match self.render(ui.ctx(), scale, dark) {
                    Ok(()) => {},
                    Err(error) => self.message = format!("系统字体预览不可用：{error}"),
                }
                self.key = Some(key);
            }
            if let Some(texture) = &self.texture {
                egui::ScrollArea::both().max_height(260.0).show(ui, |ui| {
                    ui.add(egui::Image::new((texture.id(), texture.size_vec2() / scale)))
                        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label,
                            format!("表情预览：{}。{}", self.text, self.message)));
                });
            }
            ui.label(&self.message);
        });
    }

    fn render(&mut self, ctx: &egui::Context, scale: f32, dark: bool) -> windows::core::Result<()> {
        if self.renderer.is_none() {
            self.renderer = Some(TextSystem::new()?);
        }
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };
        let family = if self.emoji {
            "Segoe UI Emoji"
        } else {
            "Segoe UI"
        };
        let raw = renderer.layout(&self.text, family, 28.0 * scale, 700.0 * scale)?;
        let coverage = inspect(&raw)?;
        let fallback = coverage.needs_fallback(self.emoji);
        let layout = if fallback {
            let codes = self
                .text
                .chars()
                .map(|c| format!("U+{:04X}", c as u32))
                .collect::<Vec<_>>()
                .join(" ");
            renderer.layout(
                &format!("字体不支持完整显示：{codes}"),
                "Segoe UI",
                18.0 * scale,
                700.0 * scale,
            )?
        } else {
            raw
        };
        let bitmap = renderer.render(
            (layout.width + 16.0).ceil() as u32,
            (layout.height + 16.0).ceil() as u32,
            if dark { 0xff202020 } else { 0xffffffff },
            &[Draw::Text {
                layout: &layout,
                x: 8.0,
                y: 8.0,
                color: if dark { 0xfffafafa } else { 0xff202020 },
            }],
        )?;
        let rgba: Vec<_> = bitmap
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        self.texture = Some(ctx.load_texture(
            "expression-system-preview",
            egui::ColorImage::from_rgba_unmultiplied(
                [bitmap.width as usize, bitmap.height as usize],
                &rgba,
            ),
            egui::TextureOptions::LINEAR,
        ));
        self.message = format!(
            "{}；字体：{}",
            if fallback {
                "使用码点回退，输入时仍提交原始完整字符串"
            } else {
                "字形覆盖检查通过；请检查实际预览"
            },
            coverage.families.into_iter().collect::<Vec<_>>().join("、")
        );
        Ok(())
    }
}
