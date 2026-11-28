use super::{docs, tool};
use crate::{
    skin_editor::{resolve_view, SkinEditor},
    skin_image_editor,
};
use core_engine::skin::{self, editor::Draft, images, parameters::Appearance};
use serde_json::{json, Value};
use std::{
    io::{Cursor, Read},
    path::Path,
};

pub fn tools() -> Vec<Value> {
    let string = json!({"type":"string"});
    let lease = json!({"type":"string","description":"Token returned by acquire"});
    vec![
        tool("create_skin","Create an unsaved copy of a bundled template",json!({"lease":lease,"template":{"enum":["jade","plain","contrast","bamboo","parameters"]}}),&["lease","template"]),
        tool("open_skin","Open an existing local .mywubi-skin file",json!({"lease":lease,"path":string}),&["lease","path"]),
        tool("inspect","Read package identity, resources and components; supply all three target fields for effective appearance",json!({"platform":string,"component":string,"variant":{"enum":["light","dark"]}}),&[]),
        tool("set_identity","Set the id, version and display name of the authored copy",json!({"lease":lease,"id":string,"version":string,"name":string}),&["lease","id","version","name"]),
        tool("edit_appearance","Atomically update scalar fields and/or image roles. An image style has resource, mode, opacity and optional slice/content; null removes a role.",json!({"lease":lease,"platform":{"enum":["windows","android"]},"component":string,"variant":{"enum":["light","dark"]},"fields":{"type":"object"},"images":{"type":"object"}}),&["lease","platform","component","variant"]),
        tool("import_png","Import a local PNG under a resource id in shared or platform scope",json!({"lease":lease,"scope":{"enum":["shared","windows","android"]},"id":string,"path":string}),&["lease","scope","id","path"]),
        tool("validate","Validate the draft for export, including image decode and keyboards",json!({}),&[]),
        tool("export","Export to a new local .mywubi-skin file",json!({"lease":lease,"path":string}),&["lease","path"]),
    ]
}
pub(super) fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("缺少字符串字段 {key}"))
}
fn path<'a>(args: &'a Value, extension: &str) -> Result<&'a Path, String> {
    let path = Path::new(string(args, "path")?);
    if !path.is_absolute()
        || !path
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case(extension))
    {
        return Err(format!("需要 .{extension} 文件的绝对路径"));
    }
    Ok(path)
}
pub(super) fn target(args: &Value) -> Result<(&str, &str, bool), String> {
    let variant = string(args, "variant")?;
    if !matches!(variant, "light" | "dark") {
        return Err("variant 须为 light 或 dark".into());
    }
    Ok((
        string(args, "platform")?,
        string(args, "component")?,
        variant == "dark",
    ))
}
fn summary(d: &Draft) -> Value {
    json!({"metadata":d.package.metadata,"platforms":d.package.platforms,"dirty":d.dirty})
}
pub fn read(name: &str, args: &Value, editor: &SkinEditor) -> Result<Value, String> {
    let d = editor.draft()?;
    match name {
        "inspect"
            if ["platform", "component", "variant"]
                .iter()
                .any(|k| args.get(k).is_some()) =>
        {
            let (p, c, dark) = target(args)?;
            let (view, _) = resolve_view(&d.package, p, c, dark)?;
            let roles: serde_json::Map<String, Value> = view
                .images
                .iter()
                .map(|(k, v)| (k.clone(), json!(v.style)))
                .collect();
            Ok(
                json!({"variant":view.variant,"fields":view.parameters,"images":roles,"warnings":view.warnings}),
            )
        }
        "inspect" => Ok(summary(d)),
        "validate" => {
            validate(d)?;
            d.check_source()
                .and_then(|_| d.package.encoded())
                .map_err(|e| e.to_string())?;
            Ok(json!({"valid":true}))
        }
        _ => Err(format!("未知工具 {name}")),
    }
}
fn validate(d: &Draft) -> Result<(), String> {
    skin_image_editor::validate(d)
        .and_then(|_| d.validate_keyboards())
        .map_err(|e| e.to_string())
}
pub fn write(name: &str, args: &Value, editor: &mut SkinEditor) -> Result<Value, String> {
    match name {
        "create_skin" => match string(args, "template")? {
            name @ ("bamboo" | "parameters") => {
                if editor.is_dirty() {
                    return Err("请先导出当前草稿".into());
                }
                let bytes = if name == "bamboo" {
                    docs::BAMBOO
                } else {
                    docs::PARAMETER_TEMPLATE
                };
                let package = skin::Package::read(Cursor::new(bytes)).map_err(|e| e.to_string())?;
                editor.replace_draft(Draft::from_template(package));
            }
            preset => editor.create(preset)?,
        },
        "open_skin" => editor.open_file(path(args, "mywubi-skin")?)?,
        "export" => {
            let path = path(args, "mywubi-skin")?;
            editor.export_file(path)?;
            return Ok(
                json!({"path":path,"id":editor.draft()?.package.metadata.id,"version":editor.draft()?.package.metadata.version}),
            );
        }
        "edit_appearance" | "import_png" | "set_identity" => {
            let mut d = editor.draft()?.clone();
            match name {
                "edit_appearance" => {
                    let (p, c, dark) = target(args)?;
                    edit(&mut d, p, c, if dark { "dark" } else { "light" }, args)
                        .map_err(|e| format!("{p}/{c}/{}: {e}", args["variant"]))?;
                }
                "import_png" => {
                    let scope = string(args, "scope")?;
                    let mut bytes = Vec::new();
                    std::fs::File::open(path(args, "png")?)
                        .and_then(|f| f.take(skin::archive::MAX_ENTRY + 1).read_to_end(&mut bytes))
                        .map_err(|e| e.to_string())?;
                    if bytes.len() as u64 > skin::archive::MAX_ENTRY {
                        return Err("PNG 超过资源限额".into());
                    }
                    d.import_png(
                        (scope != "shared").then_some(scope),
                        string(args, "id")?,
                        bytes,
                    )
                    .map_err(|e| e.to_string())?;
                }
                _ => {
                    d.package.metadata.id = string(args, "id")?.into();
                    d.package.metadata.name = string(args, "name")?.into();
                    d.package.metadata.version = string(args, "version")?.into();
                    d.package.encoded().map_err(|e| e.to_string())?;
                }
            }
            validate(&d)?;
            editor.publish_edit(d);
        }
        _ => return Err(format!("未知工具 {name}")),
    }
    Ok(summary(editor.draft()?))
}
fn edit(d: &mut Draft, p: &str, c: &str, variant: &str, args: &Value) -> Result<(), String> {
    let component = d
        .package
        .platforms
        .get(p)
        .and_then(|v| {
            v.components
                .iter()
                .find(|v| v.id == c && v.kind == "appearance")
        })
        .ok_or("外观组件不存在")?;
    let file = format!("{p}/{}", component.path);
    let bytes = d.package.files.get(&file).ok_or("外观文件不存在")?;
    let mut appearance: Appearance =
        toml::from_str(std::str::from_utf8(bytes).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let patch = appearance.variants.entry(variant.to_owned()).or_default();
    if let Some(fields) = args.get("fields") {
        let values: skin::Extensions =
            serde_json::from_value(fields.clone()).map_err(|e| e.to_string())?;
        skin::parameters::validate_patch(&values).map_err(|e| e.to_string())?;
        patch.extend(values);
    }
    if let Some(bindings) = args.get("images") {
        let mut variants = images::variants(&appearance).map_err(|e| e.to_string())?;
        let roles = variants.entry(variant.into()).or_default();
        for (role, value) in bindings.as_object().ok_or("images 必须是角色表")? {
            if value.is_null() {
                roles.remove(role);
                continue;
            }
            let style: images::Style = serde_json::from_value(value.clone())
                .map_err(|e| format!("images.{variant}.{role}: {e}"))?;
            images::resource_path(&d.package, p, &style.resource)
                .map_err(|e| format!("images.{variant}.{role}.resource: {e}"))?;
            roles.insert(role.clone(), style);
        }
        variants.retain(|_, r| !r.is_empty());
        if !variants.is_empty() {
            appearance.extensions.insert(
                "images".into(),
                toml::Value::try_from(variants).map_err(|e| e.to_string())?,
            );
            let component = d
                .package
                .platforms
                .get_mut(p)
                .and_then(|v| v.components.iter_mut().find(|v| v.id == c))
                .ok_or("组件不存在")?;
            if !component
                .capabilities
                .required
                .iter()
                .any(|k| k == images::CAPABILITY)
                && !component
                    .capabilities
                    .optional
                    .contains_key(images::CAPABILITY)
            {
                component
                    .capabilities
                    .required
                    .push(images::CAPABILITY.into());
            }
        } else {
            appearance.extensions.remove("images");
        }
    }
    d.package.files.insert(
        file,
        toml::to_string(&appearance)
            .map_err(|e| e.to_string())?
            .into_bytes(),
    );
    resolve_view(&d.package, p, c, variant == "dark")?;
    Ok(())
}
