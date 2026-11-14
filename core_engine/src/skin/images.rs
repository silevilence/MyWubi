//! Portable static-image contract. Native shells decode only the selected platform.
use super::{parameters::Appearance, Component, Error, Package};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CAPABILITY: &str = "appearance.images/1";
pub const MAX_DIMENSION: u32 = 2048;
pub const MAX_DECODED: u64 = 32 * 1024 * 1024;
pub const ROLES: &[&str] = &[
    "background",
    "candidate",
    "selected",
    "key",
    "function",
    "pressed",
    "disabled",
    "bubble",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Stretch,
    Tile,
    NineSlice,
}

/// Insets are left, top, right, bottom; slice is source pixels, content is DIP/dp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    pub resource: String,
    pub mode: Mode,
    #[serde(default)]
    pub slice: [u32; 4],
    #[serde(default)]
    pub content: [u32; 4],
    pub opacity: f32,
}

#[derive(Debug, Clone)]
pub struct Image {
    pub style: Style,
    pub path: String,
    pub width: u32,
    pub height: u32,
}
pub type Images = BTreeMap<String, Image>;
pub type Variants = BTreeMap<String, BTreeMap<String, Style>>;

pub fn variants(appearance: &Appearance) -> Result<Variants, Error> {
    appearance.extensions.get("images").map_or_else(
        || Ok(BTreeMap::new()),
        |value| value.clone().try_into().map_err(Error::from),
    )
}

/// Header limits precede allocation; the native decoder must also reject damaged streams.
pub fn dimensions(bytes: &[u8]) -> Result<(u32, u32), Error> {
    if bytes.len() < 33
        || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || bytes[8..12] != 13u32.to_be_bytes()
        || &bytes[12..16] != b"IHDR"
    {
        return Err(Error::Invalid("图片必须为静态 PNG".into()));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(Error::Invalid("PNG 尺寸必须为 1～2048 像素".into()));
    }
    let mut cursor = 8usize;
    let mut ended = false;
    while cursor.checked_add(12).is_some_and(|end| end <= bytes.len()) {
        let length = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
        let end = cursor
            .checked_add(12)
            .and_then(|v| v.checked_add(length))
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| Error::Invalid("PNG 数据截断".into()))?;
        let kind = &bytes[cursor + 4..cursor + 8];
        let checksum = u32::from_be_bytes(bytes[end-4..end].try_into().unwrap());
        if crc32fast::hash(&bytes[cursor+4..end-4]) != checksum {
            return Err(Error::Invalid("PNG 块校验失败".into()));
        }
        if kind == b"acTL" {
            return Err(Error::Invalid("不支持动态 PNG".into()));
        }
        if kind == b"IEND" {
            ended = length == 0 && end == bytes.len();
            break;
        }
        cursor = end;
    }
    if !ended {
        return Err(Error::Invalid("PNG 缺少完整结束块".into()));
    }
    Ok((width, height))
}

pub fn resource_path(package: &Package, platform: &str, reference: &str) -> Result<String, Error> {
    let path = if let Some(id) = reference.strip_prefix("shared:") {
        package.metadata.resources.get(id).cloned()
    } else if let Some(id) = reference.strip_prefix("platform:") {
        package
            .platforms
            .get(platform)
            .and_then(|p| p.resources.get(id))
            .map(|p| format!("{platform}/{p}"))
    } else {
        None
    };
    path.filter(|p| package.files.contains_key(p))
        .ok_or_else(|| Error::Invalid(format!("图片资源引用不存在：{reference}")))
}

pub fn resolve(
    package: &Package,
    platform: &str,
    component: &Component,
    appearance: &Appearance,
    variant: &str,
    supported: &BTreeSet<String>,
) -> Result<Images, Error> {
    let variants = variants(appearance)?;
    if variants.is_empty() {
        return Ok(Images::new());
    }
    let declared = component
        .capabilities
        .required
        .iter()
        .any(|s| s == CAPABILITY)
        || component
            .capabilities
            .optional
            .get(CAPABILITY)
            .is_some_and(|v| v == "plain");
    if !declared {
        return Err(Error::Invalid(
            "图片组件须声明 appearance.images/1（可选时回退 plain）".into(),
        ));
    }
    if !supported.contains(CAPABILITY) {
        return Ok(Images::new());
    }
    let mut dimensions_by_path = BTreeMap::new();
    let mut resolved = Images::new();
    for (name, roles) in variants {
        if !appearance.variants.contains_key(&name) {
            return Err(Error::Invalid("图片变体没有对应参数变体".into()));
        }
        for (role, style) in roles {
            if !ROLES.contains(&role.as_str())
                || (platform == "windows" && !matches!(role.as_str(), "background" | "selected"))
            {
                return Err(Error::Invalid(format!("平台不支持图片角色：{role}")));
            }
            if !style.opacity.is_finite()
                || !(0.0..=1.0).contains(&style.opacity)
                || style.content.iter().any(|v| *v > 64)
            {
                return Err(Error::Invalid("图片透明度或内容留白超限".into()));
            }
            let path = resource_path(package, platform, &style.resource)?;
            let (width, height) = if let Some(size) = dimensions_by_path.get(&path) {
                *size
            } else {
                let size = dimensions(&package.files[&path])?;
                dimensions_by_path.insert(path.clone(), size);
                size
            };
            let [left, top, right, bottom] = style.slice;
            if style.mode == Mode::Tile && (width < 8 || height < 8) {
                return Err(Error::Invalid("平铺纹理至少为 8×8 像素".into()));
            }
            if u64::from(left) + u64::from(right) >= u64::from(width)
                || u64::from(top) + u64::from(bottom) >= u64::from(height)
                || (style.mode != Mode::NineSlice && style.slice != [0; 4])
            {
                return Err(Error::Invalid("九宫格切片越界或非九宫格带切片".into()));
            }
            if name == variant {
                resolved.insert(
                    role,
                    Image {
                        style,
                        path,
                        width,
                        height,
                    },
                );
            }
        }
    }
    let bytes: u64 = dimensions_by_path
        .values()
        .map(|(w, h)| u64::from(*w) * u64::from(*h) * 4)
        .sum();
    if bytes > MAX_DECODED {
        return Err(Error::Invalid(
            "单个平台双变体图片解码总量超过 32 MiB".into(),
        ));
    }
    Ok(resolved)
}

/// Shared source/destination patches. Nine-slice borders shrink proportionally if needed.
pub fn patches(
    style: &Style,
    source: [f32; 2],
    target: [f32; 2],
    scale: f32,
) -> Vec<([f32; 4], [f32; 4])> {
    let [sw, sh] = source;
    let [tw, th] = target;
    if sw <= 0.0 || sh <= 0.0 || tw <= 0.0 || th <= 0.0 || scale <= 0.0 {
        return Vec::new();
    }
    if style.mode == Mode::Stretch {
        return vec![([0.0, 0.0, sw, sh], [0.0, 0.0, tw, th])];
    }
    if style.mode == Mode::Tile {
        let mut out = Vec::new();
        let mut y = 0.0;
        while y < th {
            let mut x = 0.0;
            while x < tw {
                let w = (sw * scale).min(tw - x);
                let h = (sh * scale).min(th - y);
                out.push(([0.0, 0.0, w / scale, h / scale], [x, y, x + w, y + h]));
                x += sw * scale;
            }
            y += sh * scale;
        }
        return out;
    }
    let [l, t, r, b] = style.slice.map(|v| v as f32);
    let xs = [0.0, l, sw - r, sw];
    let ys = [0.0, t, sh - b, sh];
    let sx = scale.min(tw / (l + r).max(1.0));
    let sy = scale.min(th / (t + b).max(1.0));
    let dx = [0.0, l * sx, tw - r * sx, tw];
    let dy = [0.0, t * sy, th - b * sy, th];
    let mut out = Vec::with_capacity(9);
    for y in 0..3 {
        for x in 0..3 {
            if dx[x + 1] > dx[x] && dy[y + 1] > dy[y] && xs[x + 1] > xs[x] && ys[y + 1] > ys[y] {
                out.push((
                    [xs[x], ys[y], xs[x + 1], ys[y + 1]],
                    [dx[x], dy[y], dx[x + 1], dy[y + 1]],
                ));
            }
        }
    }
    out
}
