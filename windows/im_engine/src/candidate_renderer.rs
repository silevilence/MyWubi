//! Cached DirectWrite/Direct2D raster, presented through the existing layered HWND.
use crate::{
    candidate_data::CandidateData,
    candidate_layout::{foreground, Scene},
    screen_geometry::compute_window_rect,
};
use std::sync::Arc;
use text_render::{Draw, TextSystem};
use windows::{
    core::Result,
    Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*},
};

pub(crate) const SCROLL_UP: usize = usize::MAX - 10;
pub(crate) const SCROLL_DOWN: usize = usize::MAX - 11;

#[derive(Default)]
pub(crate) struct Renderer {
    original: Option<Arc<CandidateData>>,
    appearance: Option<Arc<core_engine::skin::runtime::Snapshot>>,
    themed: Option<Arc<CandidateData>>,
    system: Option<TextSystem>,
    scene: Option<Scene>,
    last: Option<Arc<CandidateData>>,
    geometry: Option<(i32, i32, i32, i32, i32)>,
    position: Option<(i32, i32)>,
    scroll: f32,
    dirty: bool,
}

impl Renderer {
    pub fn scroll(&mut self, delta: f32) {
        self.scroll = (self.scroll + delta).max(0.0);
        self.dirty = true;
    }
    pub fn reset(&mut self) {
        self.last = None;
        self.scene = None;
        self.scroll = 0.0;
    }

    pub fn draw(
        &mut self,
        hwnd: HWND,
        data: Arc<CandidateData>,
        hits: &mut Vec<(RECT, u64, usize)>,
    ) -> Result<()> {
        let appearance = crate::skin_runtime::current();
        let appearance_changed = match (&appearance, &self.appearance) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if appearance_changed
            || self
                .original
                .as_ref()
                .is_none_or(|last| !Arc::ptr_eq(last, &data))
        {
            self.original = Some(data.clone());
            self.appearance = appearance;
            self.themed = Some(if let Some(skin) = &self.appearance {
                let mut themed = (*data).clone();
                if !skin.legacy {
                    themed.theme = crate::candidate_data::ThemeSnapshot::from_parameters(
                        skin.parameters.clone(),
                    );
                } else {
                    themed.theme.font_size = skin.parameters.number("font_size") as u16;
                    themed.theme.primary_color = skin.parameters.color("code");
                    themed.theme.background_color = skin.parameters.color("background");
                    themed.theme.highlight_color = skin.parameters.color("selected_background");
                }
                Arc::new(themed)
            } else {
                data.clone()
            });
        }
        let data = self.themed.as_ref().cloned().unwrap_or(data);
        let anchor = data
            .anchor
            .or_else(crate::screen_geometry::get_cursor_position);
        let monitor = crate::candidate_window::get_monitor_rect(hwnd, anchor);
        let size = crate::screen_geometry::pt_to_px_for_window(hwnd, data.theme.font_size).max(12);
        let geometry = (monitor.0, monitor.1, monitor.2, monitor.3, size);
        let scale = size as f32 / 19.0;
        let dip = size as f32 / (f32::from(data.theme.font_size) * 96.0 / 72.0).max(1.0);
        let parameters = data.theme.parameters.as_deref();
        let shadow = parameters.map_or(0.0, |p| p.number("shadow") * dip);
        let margin = (shadow * 2.0).ceil() as i32;
        let preferred_width = data
            .theme
            .parameters
            .as_ref()
            .map_or(900.0 * scale, |p| p.number("max_width") * dip);
        let max_width =
            preferred_width.min((monitor.2 - monitor.0 - 16 - margin * 2).max(80) as f32);
        let max_height =
            (650.0 * scale).min((monitor.3 - monitor.1 - 16 - margin * 2).max(80) as f32);
        if self.system.is_none() {
            self.system = Some(TextSystem::new()?);
        }
        let Some(system) = self.system.as_mut() else {
            return Ok(());
        };
        if self
            .last
            .as_ref()
            .is_none_or(|last| !Arc::ptr_eq(last, &data))
            || self.geometry != Some(geometry)
        {
            self.scene = Some(Scene::build(system, &data, size as f32, max_width)?);
            self.scroll = 0.0;
            self.dirty = true;
            self.last = Some(data.clone());
            self.geometry = Some(geometry);
        }
        let Some(scene) = &self.scene else {
            return Ok(());
        };
        let overflow = scene.height > max_height;
        let footer_height = if overflow {
            size as f32 * 1.8 + 8.0
        } else {
            0.0
        };
        let height = scene.height.min(max_height);
        let visible_height = height - footer_height;
        self.scroll = self
            .scroll
            .clamp(0.0, (scene.height - visible_height).max(0.0));
        let (width, height_px) = (
            scene.width.ceil() as i32 + margin * 2,
            height.ceil() as i32 + margin * 2,
        );
        let position = anchor
            .map(|anchor| compute_window_rect(anchor, (width, height_px), monitor))
            .unwrap_or((monitor.0 + 8, monitor.1 + 8));
        if !self.dirty && self.position == Some(position) {
            return Ok(());
        }
        let mut draws = Vec::new();
        let mut next_hits = Vec::new();
        for block in &scene.blocks {
            let top = block.y - self.scroll;
            let bottom = top + block.layout.height;
            if bottom <= 0.0 || top >= visible_height {
                continue;
            }
            if let Some(color) = block.background {
                draws.push(Draw::RoundFill {
                    left: block.x - 2.0,
                    top,
                    right: block.x + block.layout.width + 2.0,
                    bottom,
                    color,
                    radius: parameters.map_or(0.0, |p| p.number("selected_radius") * dip),
                });
            }
            draws.push(Draw::Text {
                layout: &block.layout,
                x: block.x,
                y: top,
                color: block.color,
            });
            if let (Some(id), Some(token)) = (block.id, data.selection_token) {
                next_hits.push((
                    RECT {
                        left: (block.x - 2.0).floor() as i32,
                        top: top.max(0.0).ceil() as i32,
                        right: (block.x + block.layout.width + 2.0).ceil().min(scene.width) as i32,
                        bottom: bottom.min(visible_height).floor() as i32,
                    },
                    token,
                    id,
                ));
            }
        }
        let scroll_up;
        let scroll_down;
        if overflow {
            // Opaque footer covers scrolled text. Its controls never send host actions.
            draws.push(Draw::Fill {
                left: 0.0,
                top: visible_height,
                right: scene.width,
                bottom: height,
                color: data.theme.background_color,
            });
            scroll_up = system.layout(
                "[ 向上 ]",
                "Segoe UI",
                size as f32,
                scene.width / 2.0 - 12.0,
            )?;
            scroll_down = system.layout(
                "[ 向下 ]",
                "Segoe UI",
                size as f32,
                scene.width / 2.0 - 12.0,
            )?;
            for (layout, x, id) in [
                (&scroll_up, 6.0, SCROLL_UP),
                (&scroll_down, scene.width / 2.0, SCROLL_DOWN),
            ] {
                draws.push(Draw::Text {
                    layout,
                    x,
                    y: visible_height + 4.0,
                    color: foreground(data.theme.background_color),
                });
                next_hits.push((
                    RECT {
                        left: x as i32,
                        top: visible_height.ceil() as i32,
                        right: (x + layout.width).ceil() as i32,
                        bottom: height.ceil() as i32,
                    },
                    0,
                    id,
                ));
            }
        }
        let border = 0xffa0a0a0;
        if parameters.is_none() {
            for (left, top, right, bottom) in [
                (0.0, 0.0, scene.width, 1.0),
                (0.0, height - 1.0, scene.width, height),
                (0.0, 0.0, 1.0, height),
                (scene.width - 1.0, 0.0, scene.width, height),
            ] {
                draws.push(Draw::Fill {
                    left,
                    top,
                    right,
                    bottom,
                    color: border,
                });
            }
        }
        let bitmap = if let Some(p) = parameters {
            for (rect, _, _) in &mut next_hits {
                rect.left += margin;
                rect.right += margin;
                rect.top += margin;
                rect.bottom += margin;
            }
            system
                .render_alpha(
                    scene.width.ceil() as u32,
                    height.ceil() as u32,
                    data.theme.background_color,
                    &draws,
                )?
                .framed(
                    p.number("corner_radius") * dip,
                    p.number("border_width") * dip,
                    p.color("border"),
                    shadow,
                )?
        } else {
            system.render(
                width as u32,
                height_px as u32,
                data.theme.background_color,
                &draws,
            )?
        };
        unsafe {
            UpdateLayeredWindow(
                hwnd,
                None,
                Some(&POINT {
                    x: position.0,
                    y: position.1,
                }),
                Some(&SIZE {
                    cx: width,
                    cy: height_px,
                }),
                Some(bitmap.dc()),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF::default(),
                Some(&BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                }),
                ULW_ALPHA,
            )?;
        }
        *hits = next_hits;
        self.position = Some(position);
        self.dirty = false;
        Ok(())
    }
}
