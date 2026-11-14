//! The settings preview uses the candidate scene and native raster, not egui text.
use crate::{candidate_data::CandidateData, candidate_layout::Scene, Bitmap, Draw, TextSystem};

pub fn candidate(
    system: &mut TextSystem,
    data: &CandidateData,
    width: f32,
) -> windows::core::Result<Bitmap> {
    let size = f32::from(data.theme.font_size) * 96.0 / 72.0;
    let scene = Scene::build(system, data, size, width)?;
    let parameters = data.theme.parameters.as_deref();
    let mut draws = Vec::new();
    for block in &scene.blocks {
        if let Some(color) = block.background {
            draws.push(Draw::RoundFill {
                left: block.x - 2.0,
                top: block.y,
                right: block.x + block.layout.width + 2.0,
                bottom: block.y + block.layout.height,
                radius: parameters.map_or(0.0, |p| p.number("selected_radius")),
                color,
            });
        }
        draws.push(Draw::Text {
            layout: &block.layout,
            x: block.x,
            y: block.y,
            color: block.color,
        });
    }
    if let Some(p) = parameters {
        system
            .render_alpha(
                scene.width as u32,
                scene.height as u32,
                data.theme.background_color,
                &draws,
            )?
            .framed(
                p.number("corner_radius"),
                p.number("border_width"),
                p.color("border"),
                p.number("shadow"),
            )
    } else {
        system.render(
            scene.width as u32,
            scene.height as u32,
            data.theme.background_color,
            &draws,
        )
    }
}
