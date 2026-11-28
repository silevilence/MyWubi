use super::{author::target, tool};
#[cfg(test)]
use crate::skin_editor::SkinEditor;
use crate::{android_skin_preview::AndroidPreview, skin_editor::resolve_view};
use base64::{engine::general_purpose::STANDARD, Engine};
use eframe::{egui, egui_wgpu};
use image::RgbaImage;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::Cursor,
    sync::{Arc, OnceLock},
};
use text_render::candidate_data::{CandidateData, CandidateItem, ThemeSnapshot};

pub fn definition() -> Value {
    tool("preview","Render a batch of explicitly targeted PNGs. Windows uses native candidates; Android uses the desktop schematic.",json!({"platform":{"enum":["windows","android"]},"component":{"type":"string"},"variant":{"enum":["light","dark"]},"scenes":{"type":"array","minItems":1,"maxItems":6,"items":{"type":"object","properties":{"state":{"enum":["ordinary","long","pressed","disabled","bubble","symbols"]},"scale":{"type":"number","minimum":1,"maximum":2},"landscape":{"type":"boolean"}},"additionalProperties":false}}}),&["platform","component","variant","scenes"])
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scene {
    #[serde(default = "ordinary")]
    state: String,
    #[serde(default = "one")]
    scale: f32,
    #[serde(default)]
    landscape: bool,
}
fn one() -> f32 {
    1.
}
fn ordinary() -> String {
    "ordinary".into()
}

#[cfg(test)]
pub fn run(args: &Value, editor: &SkinEditor) -> Result<Value, String> {
    render(args, &editor.draft()?.package)
}
pub fn render(args: &Value, package: &core_engine::skin::Package) -> Result<Value, String> {
    let (platform, component, dark) = target(args)?;
    let scenes: Vec<Scene> =
        serde_json::from_value(args["scenes"].clone()).map_err(|e| e.to_string())?;
    if !(1..=6).contains(&scenes.len()) {
        return Err("scenes 需指定一至六个场景".into());
    }
    let (view, images) = resolve_view(package, platform, component, dark)?;
    let images = Arc::new(images);
    let mut results = Vec::new();
    for scene in scenes {
        if !(1.0..=2.0).contains(&scene.scale) {
            return Err("scale 须在 1～2 之间".into());
        }
        let (image, source) = match platform {
            "windows"
                if matches!(scene.state.as_str(), "ordinary" | "long" | "disabled")
                    && !scene.landscape =>
            {
                let mut data = CandidateData {
                    visible: true,
                    spelling: "a".into(),
                    theme: ThemeSnapshot::from_parameters(Arc::new(view.parameters.clone())),
                    ..Default::default()
                };
                data.theme.images = images.clone();
                let words: &[&str] = match scene.state.as_str() {
                    "long" => &[
                        "跨平台形码输入法候选显示与长文本自动换行",
                        "internationalization and accessibility",
                    ],
                    "disabled" => &["拼音码表不可用"],
                    _ => &["工", "戈", "或"],
                };
                data.items = words
                    .iter()
                    .enumerate()
                    .map(|(i, t)| CandidateItem {
                        label: format!("{}. ", i + 1),
                        text: (*t).into(),
                        hint: String::new(),
                        disabled: scene.state == "disabled",
                        expression_kind: None,
                    })
                    .collect();
                let bitmap = text_render::TextSystem::new()
                    .and_then(|mut s| {
                        text_render::preview::candidate_scaled(
                            &mut s,
                            &data,
                            600. * scene.scale,
                            scene.scale,
                        )
                    })
                    .map_err(|e| e.to_string())?;
                let pixels = bitmap
                    .pixels()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| {
                        egui::Color32::from_rgba_premultiplied(p[2], p[1], p[0], p[3])
                            .to_srgba_unmultiplied()
                    })
                    .collect();
                (
                    RgbaImage::from_raw(bitmap.width, bitmap.height, pixels)
                        .ok_or("无效的原生位图")?,
                    "windows-native",
                )
            }
            "android"
                if matches!(
                    scene.state.as_str(),
                    "ordinary" | "pressed" | "disabled" | "symbols" | "bubble"
                ) =>
            {
                let width = if scene.landscape { 720 } else { 393 };
                let mut preview =
                    AndroidPreview::scene(width, scene.landscape, scene.scale, &scene.state);
                (
                    offscreen(width, |ui| preview.paint(ui, &view.parameters, &images))?,
                    "android-desktop-schematic",
                )
            }
            _ => return Err(format!("{platform} 不支持场景 {}", scene.state)),
        };
        let mut png = Cursor::new(Vec::new());
        image
            .write_to(&mut png, image::ImageOutputFormat::Png)
            .map_err(|e| e.to_string())?;
        results.push(json!({"type":"text","text":json!({"source":source,"platform":platform,"variant":view.variant,"state":scene.state,"scale":scene.scale,"landscape":scene.landscape,"width":image.width(),"height":image.height()}).to_string()}));
        results.push(
            json!({"type":"image","mimeType":"image/png","data":STANDARD.encode(png.into_inner())}),
        );
    }
    Ok(results.into())
}

fn gpu() -> Result<&'static (wgpu::Device, wgpu::Queue), String> {
    static GPU: OnceLock<(wgpu::Device, wgpu::Queue)> = OnceLock::new();
    if let Some(gpu) = GPU.get() {
        return Ok(gpu);
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .ok_or("预览找不到 DX12 适配器")?;
    let pair = pollster::block_on(adapter.request_device(&Default::default(), None))
        .map_err(|e| e.to_string())?;
    Ok(GPU.get_or_init(|| pair))
}
/// Paint the same keyboard used by the interactive editor into an offscreen texture.
fn offscreen(
    width: u32,
    paint: impl FnOnce(&mut egui::Ui) -> egui::Rect,
) -> Result<RgbaImage, String> {
    let (device, queue) = gpu()?;
    let ctx = egui::Context::default();
    crate::fonts::load_chinese_fonts(&ctx);
    ctx.begin_frame(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width as f32, 1200.),
        )),
        ..Default::default()
    });
    let rect = egui::CentralPanel::default()
        .frame(egui::Frame::none())
        .show(&ctx, paint)
        .inner;
    let output = ctx.end_frame();
    let height = rect.max.y.ceil() as u32;
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("skin preview"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let stride = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(stride) * u64::from(height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut renderer = egui_wgpu::Renderer::new(device, format, None, 1);
    for (id, delta) in &output.textures_delta.set {
        renderer.update_texture(device, queue, *id, delta);
    }
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [width, height],
        pixels_per_point: output.pixels_per_point,
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    let callbacks = renderer.update_buffers(device, queue, &mut encoder, &primitives, &screen);
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.render(&mut pass, &primitives, &screen);
    }
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::ImageCopyBuffer {
            buffer: &buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
        },
        size,
    );
    queue.submit(callbacks.into_iter().chain(Some(encoder.finish())));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let pixels = slice
        .get_mapped_range()
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..width as usize * 4].iter().copied())
        .collect();
    RgbaImage::from_raw(width, height, pixels).ok_or_else(|| "预览位图尺寸无效".into())
}
