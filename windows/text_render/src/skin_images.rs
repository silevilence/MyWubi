//! CPU image cache prepared before publication; no native handles cross threads.
use core_engine::skin::{
    images::{self, Image, Images},
    Error, Package,
};
use std::{collections::BTreeMap, io::Cursor, sync::Arc};

#[derive(Debug)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}
#[derive(Debug)]
pub struct Cached {
    pub image: Image,
    pub pixels: Arc<Pixels>,
}
pub type Cache = BTreeMap<String, Cached>;

pub fn decode(package: &Package, images: &Images) -> Result<Cache, Error> {
    let mut pixels = BTreeMap::<String, Arc<Pixels>>::new();
    let mut cache = Cache::new();
    for (role, image) in images {
        let data = if let Some(data) = pixels.get(&image.path) {
            data.clone()
        } else {
            let bytes = &package.files[&image.path];
            images::dimensions(bytes)?;
            let mut decoder = png::Decoder::new(Cursor::new(bytes));
            decoder.set_limits(png::Limits {
                bytes: images::MAX_DECODED as usize,
            });
            decoder
                .set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
            let mut reader = decoder
                .read_info()
                .map_err(|e| Error::Invalid(format!("PNG 解码失败：{e}")))?;
            if reader.output_buffer_size() > images::MAX_DECODED as usize {
                return Err(Error::Invalid("PNG 解码超限".into()));
            }
            let mut output = vec![0; reader.output_buffer_size()];
            let info = reader
                .next_frame(&mut output)
                .map_err(|e| Error::Invalid(format!("PNG 解码失败：{e}")))?;
            let channels = info.color_type.samples();
            let mut bgra = Vec::with_capacity((info.width * info.height * 4) as usize);
            for pixel in output[..info.buffer_size()].chunks_exact(channels) {
                let (r, g, b, a) = match info.color_type {
                    png::ColorType::Rgba => (pixel[0], pixel[1], pixel[2], pixel[3]),
                    png::ColorType::Rgb => (pixel[0], pixel[1], pixel[2], 255),
                    png::ColorType::Grayscale => (pixel[0], pixel[0], pixel[0], 255),
                    png::ColorType::GrayscaleAlpha => (pixel[0], pixel[0], pixel[0], pixel[1]),
                    _ => return Err(Error::Invalid("PNG 像素格式无法归一化".into())),
                };
                bgra.extend([b, g, r].map(|c| ((u16::from(c) * u16::from(a) + 127) / 255) as u8));
                bgra.push(a);
            }
            let data = Arc::new(Pixels {
                width: info.width,
                height: info.height,
                bgra,
            });
            pixels.insert(image.path.clone(), data.clone());
            data
        };
        cache.insert(
            role.clone(),
            Cached {
                image: image.clone(),
                pixels: data,
            },
        );
    }
    Ok(cache)
}

impl super::Bitmap {
    /// Composite image layers behind transparent native text in premultiplied BGRA.
    pub fn skin_background(
        &self,
        cache: &Cache,
        scale: f32,
        background: u32,
        selected: &[[f32; 4]],
    ) -> windows::core::Result<Self> {
        let mut output = Self::new(self.width, self.height)?;
        let [a, r, g, b] = background.to_be_bytes();
        let fill = [b, g, r].map(|c| ((u16::from(c) * u16::from(a) + 127) / 255) as u8);
        for pixel in output.pixels_mut().as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&[fill[0], fill[1], fill[2], a]);
        }
        if let Some(image) = cache.get("background") {
            paint(
                &mut output,
                image,
                [0.0, 0.0, self.width as f32, self.height as f32],
                scale,
            );
        }
        if let Some(image) = cache.get("selected") {
            for rect in selected {
                paint(&mut output, image, *rect, scale);
            }
        }
        for (destination, source) in output
            .pixels_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(self.pixels().as_chunks::<4>().0)
        {
            over(destination, *source);
        }
        Ok(output)
    }
}
fn over(destination: &mut [u8; 4], source: [u8; 4]) {
    let inverse = 255 - u16::from(source[3]);
    for i in 0..4 {
        destination[i] = (u16::from(source[i]) + (u16::from(destination[i]) * inverse + 127) / 255)
            .min(255) as u8;
    }
}
fn paint(bitmap: &mut super::Bitmap, cached: &Cached, rect: [f32; 4], scale: f32) {
    let data = &cached.pixels;
    let style = &cached.image.style;
    let width = bitmap.width;
    let height = bitmap.height;
    for (source, dest) in images::patches(
        style,
        [data.width as f32, data.height as f32],
        [rect[2] - rect[0], rect[3] - rect[1]],
        scale,
    ) {
        let dx = dest[0] + rect[0];
        let dy = dest[1] + rect[1];
        let dw = dest[2] - dest[0];
        let dh = dest[3] - dest[1];
        for y in (dy.ceil().max(0.0) as u32)..((dy + dh).ceil().min(height as f32) as u32) {
            for x in (dx.ceil().max(0.0) as u32)..((dx + dw).ceil().min(width as f32) as u32) {
                let sx = (source[0] + ((x as f32 + 0.5 - dx) / dw) * (source[2] - source[0]))
                    .clamp(0.0, data.width as f32 - 1.0) as u32;
                let sy = (source[1] + ((y as f32 + 0.5 - dy) / dh) * (source[3] - source[1]))
                    .clamp(0.0, data.height as f32 - 1.0) as u32;
                let offset = ((sy * data.width + sx) * 4) as usize;
                let source: [u8; 4] = data.bgra[offset..offset + 4].try_into().unwrap();
                let source = source.map(|v| (f32::from(v) * style.opacity).round() as u8);
                let offset = ((y * width + x) * 4) as usize;
                over(
                    (&mut bitmap.pixels_mut()[offset..offset + 4])
                        .try_into()
                        .unwrap(),
                    source,
                );
            }
        }
    }
}
