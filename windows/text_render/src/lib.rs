//! System text shaping and color-font rasterization shared by the candidate window
//! and settings previews. No GUI runtime or downloaded fonts are involved.
#![cfg(windows)]

use std::{collections::HashMap, ffi::c_void, ptr};
use windows::{
    core::{w, Result, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dxgi::Common::*,
            Gdi::*,
        },
    },
};
use windows_numerics::Vector2;

pub mod candidate_data;
pub mod candidate_layout;
pub mod coverage;
pub mod preview;
pub mod skin_images;

/// One shaped run, including ink overhang so accents are not cropped.
#[derive(Clone)]
pub struct TextLayout {
    pub(crate) raw: IDWriteTextLayout,
    pub width: f32,
    pub height: f32,
    inset_x: f32,
    inset_y: f32,
}

/// A UTF-16 range styled within one shaped/wrapped layout.
pub struct TextSpan {
    pub start: u32,
    pub length: u32,
    pub color: u32,
    pub size: f32,
}

/// Thread-local resources. Retain this object across frames, never across threads.
pub struct TextSystem {
    write: IDWriteFactory,
    target: ID2D1DCRenderTarget,
    formats: HashMap<(String, u32), IDWriteTextFormat>,
}

impl TextSystem {
    pub fn new() -> Result<Self> {
        unsafe {
            let write = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let target = factory.CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            })?;
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            Ok(Self {
                write,
                target,
                formats: HashMap::new(),
            })
        }
    }

    /// Shapes the complete string with system fallback; width is in physical pixels.
    /// Emergency wrapping uses DirectWrite cluster boundaries, not UTF-16 units.
    pub fn layout(
        &mut self,
        text: &str,
        family: &str,
        size: f32,
        width: f32,
    ) -> Result<TextLayout> {
        self.layout_styled(text, family, size, width, &[])
    }

    pub fn layout_styled(
        &mut self,
        text: &str,
        family: &str,
        size: f32,
        width: f32,
        spans: &[TextSpan],
    ) -> Result<TextLayout> {
        let key = (family.to_owned(), size.to_bits());
        if !self.formats.contains_key(&key) {
            let name: Vec<u16> = family.encode_utf16().chain(Some(0)).collect();
            let format = unsafe {
                self.write.CreateTextFormat(
                    PCWSTR(name.as_ptr()),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("zh-CN"),
                )?
            };
            unsafe {
                format.SetWordWrapping(DWRITE_WORD_WRAPPING_EMERGENCY_BREAK)?;
            }
            if self.formats.len() >= 32 {
                self.formats.clear();
            }
            self.formats.insert(key.clone(), format);
        }
        let format = &self.formats[&key];
        let utf16: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let layout_width = width.max(1.0);
            let layout_height = 100_000.0;
            let raw = self
                .write
                .CreateTextLayout(&utf16, format, layout_width, layout_height)?;
            for span in spans {
                if span.start as usize + span.length as usize > utf16.len()
                    || !span.size.is_finite()
                    || span.size <= 0.0
                {
                    return Err(windows::core::Error::new(
                        E_INVALIDARG,
                        "Invalid text style range",
                    ));
                }
                let range = DWRITE_TEXT_RANGE {
                    startPosition: span.start,
                    length: span.length,
                };
                let brush = self.target.CreateSolidColorBrush(&rgba(span.color), None)?;
                raw.SetDrawingEffect(&brush, range)?;
                raw.SetFontSize(span.size, range)?;
            }
            let mut metrics = DWRITE_TEXT_METRICS::default();
            raw.GetMetrics(&mut metrics)?;
            let ink = raw.GetOverhangMetrics()?;
            let inset_x = ink.left.max(0.0).ceil() + 1.0;
            let inset_y = ink.top.max(0.0).ceil() + 1.0;
            Ok(TextLayout {
                width: (metrics.widthIncludingTrailingWhitespace
                    + inset_x
                    + (layout_width + ink.right - metrics.widthIncludingTrailingWhitespace)
                        .max(0.0)
                    + 1.0)
                    .ceil(),
                height: (metrics.height
                    + inset_y
                    + (layout_height + ink.bottom - metrics.height).max(0.0)
                    + 1.0)
                    .ceil(),
                raw,
                inset_x,
                inset_y,
            })
        }
    }

    pub fn render(
        &self,
        width: u32,
        height: u32,
        background: u32,
        draws: &[Draw<'_>],
    ) -> Result<Bitmap> {
        self.render_internal(width, height, background, draws, false)
    }

    /// Transparent premultiplied BGRA for UpdateLayeredWindow and native previews.
    pub fn render_alpha(
        &self,
        width: u32,
        height: u32,
        background: u32,
        draws: &[Draw<'_>],
    ) -> Result<Bitmap> {
        self.render_internal(width, height, background, draws, true)
    }

    fn render_internal(
        &self,
        width: u32,
        height: u32,
        background: u32,
        draws: &[Draw<'_>],
        alpha: bool,
    ) -> Result<Bitmap> {
        let mut bitmap = Bitmap::new(width, height)?;
        unsafe {
            self.target.BindDC(
                bitmap.dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: width as i32,
                    bottom: height as i32,
                },
            )?;
            // Create brushes before BeginDraw so errors cannot leave an open batch.
            let mut brushes = HashMap::new();
            for draw in draws {
                let color = match draw {
                    Draw::Text { color, .. }
                    | Draw::Fill { color, .. }
                    | Draw::RoundFill { color, .. } => *color,
                };
                if let std::collections::hash_map::Entry::Vacant(entry) = brushes.entry(color) {
                    entry.insert(self.target.CreateSolidColorBrush(
                        &rgba(if alpha { color } else { color | 0xff000000 }),
                        None,
                    )?);
                }
            }
            self.target.BeginDraw();
            self.target.Clear(Some(&rgba(if alpha {
                background
            } else {
                background | 0xff000000
            })));
            for draw in draws {
                match draw {
                    Draw::Text {
                        layout,
                        x,
                        y,
                        color,
                    } => self.target.DrawTextLayout(
                        Vector2 {
                            X: x + layout.inset_x,
                            Y: y + layout.inset_y,
                        },
                        &layout.raw,
                        &brushes[color],
                        D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                    ),
                    Draw::Fill {
                        left,
                        top,
                        right,
                        bottom,
                        color,
                    } => self.target.FillRectangle(
                        &D2D_RECT_F {
                            left: *left,
                            top: *top,
                            right: *right,
                            bottom: *bottom,
                        },
                        &brushes[color],
                    ),
                    Draw::RoundFill {
                        left,
                        top,
                        right,
                        bottom,
                        radius,
                        color,
                    } => self.target.FillRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: D2D_RECT_F {
                                left: *left,
                                top: *top,
                                right: *right,
                                bottom: *bottom,
                            },
                            radiusX: *radius,
                            radiusY: *radius,
                        },
                        &brushes[color],
                    ),
                }
            }
            self.target.EndDraw(None, None)?;
            GdiFlush().ok()?;
        }
        if !alpha {
            for pixel in bitmap.pixels_mut().as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
        }
        Ok(bitmap)
    }
}

pub enum Draw<'a> {
    RoundFill {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        radius: f32,
        color: u32,
    },
    Text {
        layout: &'a TextLayout,
        x: f32,
        y: f32,
        color: u32,
    },
    Fill {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        color: u32,
    },
}

impl Draw<'_> {
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Self::Text { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            Self::Fill {
                left,
                top,
                right,
                bottom,
                ..
            }
            | Self::RoundFill {
                left,
                top,
                right,
                bottom,
                ..
            } => {
                *left += dx;
                *right += dx;
                *top += dy;
                *bottom += dy;
            }
        }
    }
}

fn rgba(argb: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((argb >> 16) & 255) as f32 / 255.0,
        g: ((argb >> 8) & 255) as f32 / 255.0,
        b: (argb & 255) as f32 / 255.0,
        a: ((argb >> 24) & 255) as f32 / 255.0,
    }
}

/// Owns the selected DIB and memory DC. Bytes are BGRA, row-major, top-down.
pub struct Bitmap {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    pub width: u32,
    pub height: u32,
}

impl Bitmap {
    /// Masks content and composites a border and soft shadow in premultiplied space.
    /// The returned margin is visual decoration only; callers offset existing hit boxes.
    pub fn framed(&self, radius: f32, border_width: f32, border: u32, shadow: f32) -> Result<Self> {
        let margin = (shadow * 2.0).ceil() as u32;
        let mut output = Self::new(self.width + margin * 2, self.height + margin * 2)?;
        let width = self.width as f32;
        let height = self.height as f32;
        let radius = radius.clamp(0.0, width.min(height) / 2.0);
        let source = self.pixels();
        let out_width = output.width as usize;
        for (index, pixel) in output
            .pixels_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .enumerate()
        {
            let x = (index % out_width) as f32 - margin as f32 + 0.5;
            let y = (index / out_width) as f32 - margin as f32 + 0.5;
            let qx = (x - width / 2.0).abs() - (width / 2.0 - radius);
            let qy = (y - height / 2.0).abs() - (height / 2.0 - radius);
            let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
            let outer = (0.5 - distance).clamp(0.0, 1.0);
            let inner = (0.5 - distance - border_width).clamp(0.0, 1.0);
            let shadow_alpha = if shadow > 0.0 {
                0.18 * (1.0 - (distance.max(0.0) / (2.0 * shadow)).min(1.0)).powi(2) * (1.0 - outer)
            } else {
                0.0
            };
            let mut rgba = [0.0, 0.0, 0.0, shadow_alpha];
            if x >= 0.0 && y >= 0.0 && x < width && y < height {
                let start = ((y as usize) * self.width as usize + x as usize) * 4;
                let a = source[start + 3] as f32 / 255.0 * outer;
                for channel in 0..3 {
                    rgba[channel] = source[start + channel] as f32 / 255.0 * outer;
                }
                rgba[3] = a + rgba[3] * (1.0 - a);
            }
            let a = ((border >> 24) & 255) as f32 / 255.0 * (outer - inner);
            for (channel, value) in rgba.iter_mut().enumerate().take(3) {
                let c = ((border >> (channel * 8)) & 255) as f32 / 255.0;
                *value = c * a + *value * (1.0 - a);
            }
            rgba[3] = a + rgba[3] * (1.0 - a);
            for channel in 0..4 {
                pixel[channel] = (rgba[channel] * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
        Ok(output)
    }

    fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || u64::from(width) * u64::from(height) > 16_777_216
        {
            return Err(windows::core::Error::new(
                E_INVALIDARG,
                "Invalid raster size",
            ));
        }
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err(windows::core::Error::from_thread());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    let _ = DeleteDC(dc);
                    return Err(error);
                }
            };
            if bits.is_null() {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                let _ = DeleteDC(dc);
                return Err(windows::core::Error::new(
                    E_OUTOFMEMORY,
                    "DIB allocation returned no pixels",
                ));
            }
            let previous = SelectObject(dc, HGDIOBJ(bitmap.0));
            Ok(Self {
                dc,
                bitmap,
                previous,
                bits: bits.cast(),
                width,
                height,
            })
        }
    }

    pub fn dc(&self) -> HDC {
        self.dc
    }
    pub fn pixels(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.bits, self.width as usize * self.height as usize * 4)
        }
    }
    fn pixels_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(
                self.bits,
                self.width as usize * self.height as usize * 4,
            )
        }
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.dc, self.previous);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.dc);
        }
    }
}
