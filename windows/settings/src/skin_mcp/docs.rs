use base64::{engine::general_purpose::STANDARD, Engine};
use core_engine::skin::parameters;
use serde_json::{json, Value};
use std::borrow::Cow;

pub const BAMBOO: &[u8] = include_bytes!("../../../../examples/skins/bamboo-dual.mywubi-skin");
pub const PARAMETER_TEMPLATE: &[u8] =
    include_bytes!("../../../../examples/skins/parameters-dual.mywubi-skin");

struct Resource {
    key: &'static str,
    name: &'static str,
    mime: &'static str,
    data: &'static [u8],
}
const SPACE_EXAMPLE: &str =
    "[images.dark.space]\nresource = \"shared:bamboo\"\nmode = \"stretch\"\nopacity = 0.8";
const RESOURCES: &[Resource] = &[
    Resource {
        key: "mcp",
        name: "MCP 使用说明与真实宿主环境",
        mime: "text/markdown",
        data: include_bytes!("../../../../docs/skin-mcp.md"),
    },
    Resource {
        key: "guide",
        name: "制作指南 Author guide",
        mime: "text/markdown",
        data: include_bytes!("../../../../docs/skin-author-guide.md"),
    },
    Resource {
        key: "format",
        name: "包格式、能力与继承 Package contract",
        mime: "text/markdown",
        data: include_bytes!("../../../../docs/skin-package-format.md"),
    },
    Resource {
        key: "fields",
        name: "外观字段字典 Appearance fields",
        mime: "application/json",
        data: &[],
    },
    Resource {
        key: "android-space",
        name: "Android 空格键背景 space key background",
        mime: "application/json",
        data: &[],
    },
    Resource {
        key: "templates",
        name: "模板与素材许可 Templates and licensing",
        mime: "text/markdown",
        data: include_bytes!("../../../../examples/skins/README.md"),
    },
    Resource {
        key: "bamboo",
        name: "竹纸双端模板 Bamboo dual",
        mime: "application/zip",
        data: BAMBOO,
    },
    Resource {
        key: "parameters",
        name: "双端参数模板 Parameters",
        mime: "application/zip",
        data: PARAMETER_TEMPLATE,
    },
    Resource {
        key: "bamboo-paper",
        name: "竹纸 PNG 素材 (MIT)",
        mime: "image/png",
        data: include_bytes!("../../../../examples/skins/assets/bamboo-paper.png"),
    },
    Resource {
        key: "bamboo-preview",
        name: "竹纸 Windows 原生预览",
        mime: "image/png",
        data: include_bytes!("../../../../docs/validation/skins/windows-bamboo-native.png"),
    },
    Resource {
        key: "scenes",
        name: "固定预览状态与测试数据 Preview scenes",
        mime: "application/json",
        data: include_bytes!("../../../../docs/validation/skins/states.json"),
    },
    Resource {
        key: "device-tests",
        name: "真实宿主测试环境与设备要求",
        mime: "text/markdown",
        data: include_bytes!("../../../../docs/skin-validation.md"),
    },
];

fn uri(r: &Resource) -> String {
    format!("mywubi://skin/{}", r.key)
}
pub fn list() -> Vec<Value> {
    RESOURCES
        .iter()
        .map(|r| json!({"uri":uri(r),"name":r.name,"mimeType":r.mime}))
        .collect()
}
fn fields() -> Result<Value, String> {
    let light = parameters::builtin("jade", false).map_err(|e| e.to_string())?;
    let dark = parameters::builtin("jade", true).map_err(|e| e.to_string())?;
    let mut fields = serde_json::Map::new();
    for (key, value) in &light.0 {
        let k = key.as_str();
        let (kind, domain, unit) = match k {
            "font_family" => ("string", json!("1..256 UTF-8 bytes"), ""),
            "layout" => ("enum", json!(["auto", "horizontal", "vertical"]), ""),
            "density" => ("enum", json!(["compact", "comfortable"]), ""),
            "font_size" | "hint_size" | "key_text_size" | "bubble_text_size" => {
                ("number", json!([8, 96]), "pt / sp")
            }
            "border_width" => ("number", json!([0, 8]), "DIP / dp"),
            "max_width" => ("number", json!([100, 2000]), "DIP"),
            "keyboard_height" => ("number", json!([160, 600]), "dp"),
            color if parameters::COLORS.contains(&color) => {
                ("string", json!(["#RRGGBB", "#RRGGBBAA"]), "RGBA")
            }
            _ => ("number", json!([0, 64]), "DIP / dp"),
        };
        let platform = if ["key", "function", "pressed", "bubble", "keyboard"]
            .iter()
            .any(|p| k.starts_with(p))
        {
            "android"
        } else if matches!(k, "layout" | "density" | "max_width") {
            "windows"
        } else {
            "windows/android"
        };
        fields.insert(key.clone(),json!({"type":kind,"range":domain,"unit":unit,"platform":platform,"jadeLight":value,"jadeDark":dark.0[k]}));
    }
    Ok(
        json!({"fields":fields,"inheritance":["builtin","root","platform","component","variant","user","accessibility"],"imageRoles":core_engine::skin::images::ROLES}),
    )
}
fn data(r: &Resource) -> Result<Cow<'static, [u8]>, String> {
    let value = match r.key {
        "fields" => fields()?,
        "android-space" => {
            json!({"platform":"android","field":"images.dark.space","requires":"appearance.images/1","fallback":"key","template":"mywubi://skin/bamboo","example":SPACE_EXAMPLE})
        }
        _ => return Ok(Cow::Borrowed(r.data)),
    };
    Ok(Cow::Owned(value.to_string().into_bytes()))
}
fn content(r: &Resource) -> Result<Value, String> {
    let mut v = json!({"uri":uri(r),"mimeType":r.mime});
    let bytes = data(r)?;
    if r.mime.starts_with("text/") || r.mime == "application/json" {
        v["text"] = Value::from(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?);
    } else {
        v["blob"] = STANDARD.encode(bytes).into();
    }
    Ok(v)
}
pub fn read(request: &str) -> Result<Value, String> {
    content(
        RESOURCES
            .iter()
            .find(|r| uri(r) == request)
            .ok_or("未知资料 URI")?,
    )
}
pub fn lookup(query: &str) -> Result<Value, String> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Err("请输入查询词或资源 URI".into());
    }
    let mut found = Vec::new();
    for r in RESOURCES {
        let title = format!("{} {}", uri(r), r.name).to_lowercase();
        let text = r.mime.starts_with("text/") || r.mime == "application/json";
        let entry = if text { Some(content(r)?) } else { None };
        let lines = entry
            .as_ref()
            .and_then(|e| e["text"].as_str())
            .unwrap_or_default();
        if title.contains(&query) || lines.to_lowercase().contains(&query) {
            let excerpt = if r.mime == "application/json" {
                lines.to_owned()
            } else {
                lines
                    .lines()
                    .filter(|s| s.to_lowercase().contains(&query))
                    .take(4)
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            found.push(json!({"uri":uri(r),"name":r.name,"text":if excerpt.is_empty() && text {lines.lines().take(5).collect::<Vec<_>>().join("\n")} else {excerpt}}));
        }
    }
    found.sort_by_key(|entry| {
        !entry["name"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase()
            .contains(&query)
    });
    Ok(json!({"matches":found}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn space_key_example_from_both_routes_resolves_on_the_shipped_package() {
        let resource = read("mywubi://skin/android-space").unwrap();
        let matches = lookup("Android 空格键背景").unwrap();
        assert_eq!(matches["matches"][0]["text"], resource["text"]);
        let mut package = core_engine::skin::Package::read(std::io::Cursor::new(BAMBOO)).unwrap();
        let bytes = package.files.get_mut("android/appearance.toml").unwrap();
        let mut appearance: toml::Table =
            toml::from_str(std::str::from_utf8(bytes).unwrap()).unwrap();
        let sample: toml::Table = toml::from_str(SPACE_EXAMPLE).unwrap();
        appearance["images"]["dark"]
            .as_table_mut()
            .unwrap()
            .insert("space".into(), sample["images"]["dark"]["space"].clone());
        *bytes = toml::to_string(&appearance).unwrap().into_bytes();
        let (view, _) =
            crate::skin_editor::resolve_view(&package, "android", "jade", true).unwrap();
        assert!(view.images.contains_key("space"));
    }
}
