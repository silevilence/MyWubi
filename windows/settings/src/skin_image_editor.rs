//! Image authoring forms; package paths never come from local source filenames.
use core_engine::skin::{
    self,
    editor::Draft,
    images::{self, Mode, Style},
    parameters::Appearance,
};
use eframe::egui;

pub fn form(
    ui: &mut egui::Ui,
    draft: &mut Draft,
    platform: &str,
    component_id: &str,
    dark: bool,
) -> Result<(), skin::Error> {
    let Some(component) = draft.package.platforms[platform]
        .components
        .iter()
        .find(|c| c.id == component_id)
        .cloned()
    else {
        return Ok(());
    };
    let supported = [
        "container.zip/1".into(),
        format!("{platform}.base/1"),
        "appearance.parameters/1".into(),
        images::CAPABILITY.into(),
    ]
    .into();
    let (base, states) =
        draft
            .package
            .compatibility(platform, &supported, &["appearance".into()].into())?;
    if !base.enabled || !states[component_id].enabled {
        return Ok(());
    }
    let path = format!("{platform}/{}", component.path);
    let mut appearance: Appearance = toml::from_str(
        std::str::from_utf8(&draft.package.files[&path])
            .map_err(|_| skin::Error::Invalid("外观文件不是 UTF-8".into()))?,
    )?;
    let mut variants = images::variants(&appearance)?;
    let variant = if dark { "dark" } else { "light" };
    if !appearance.variants.contains_key(variant) {
        return Ok(());
    }
    let resources: Vec<_> = draft
        .package
        .metadata
        .resources
        .keys()
        .map(|id| format!("shared:{id}"))
        .chain(
            draft.package.platforms[platform]
                .resources
                .keys()
                .map(|id| format!("platform:{id}")),
        )
        .collect();
    let roles = variants.entry(variant.into()).or_default();
    let mut changed = false;
    ui.collapsing("静态 PNG 与九宫格",|ui| {
        ui.label("共享资源修改会影响引用它的两端；仅改单端请先复制为平台私有资源并重新绑定。留白顺序：左、上、右、下。");
        for role in images::ROLES.iter().copied().filter(|role|platform=="android" || matches!(*role,"background"|"selected")) {
            ui.push_id(role,|ui| {
                let mut binding=roles.get(role).map(|s|s.resource.clone()).unwrap_or_default();
                egui::ComboBox::from_label(role).selected_text(if binding.is_empty(){"参数背景"}else{&binding}).show_ui(ui,|ui|{
                    changed|=ui.selectable_value(&mut binding,String::new(),"参数背景").changed();
                    for id in &resources {changed|=ui.selectable_value(&mut binding,id.clone(),id).changed();}
                });
                if binding.is_empty(){roles.remove(role);return;}
                let style=roles.entry(role.into()).or_insert(Style{resource:binding.clone(),mode:Mode::Stretch,slice:[0;4],content:[0;4],opacity:1.0});
                style.resource=binding;
                ui.horizontal(|ui|{
                    for (mode,label) in [(Mode::Stretch,"拉伸"),(Mode::Tile,"平铺"),(Mode::NineSlice,"九宫格")] {changed|=ui.selectable_value(&mut style.mode,mode,label).changed();}
                    changed|=ui.add(egui::Slider::new(&mut style.opacity,0.0..=1.0).text("透明度")).changed();
                });
                if style.mode==Mode::NineSlice {ui.horizontal(|ui|{ui.label("切片 px");for value in &mut style.slice {changed|=ui.add(egui::DragValue::new(value).clamp_range(0..=2047)).changed();}});}
                else {style.slice=[0;4];}
                ui.horizontal(|ui|{ui.label("内容留白");for value in &mut style.content {changed|=ui.add(egui::DragValue::new(value).clamp_range(0..=64)).changed();}});
            });
        }
    });
    if changed {
        variants.retain(|_, roles| !roles.is_empty());
        if variants.is_empty() {
            appearance.extensions.remove("images");
        } else {
            appearance
                .extensions
                .insert("images".into(), toml::Value::try_from(&variants)?);
        }
        draft
            .package
            .files
            .insert(path, toml::to_string(&appearance)?.into_bytes());
        let component = draft
            .package
            .platforms
            .get_mut(platform)
            .unwrap()
            .components
            .iter_mut()
            .find(|c| c.id == component_id)
            .unwrap();
        component
            .capabilities
            .required
            .retain(|c| c != images::CAPABILITY);
        component.capabilities.optional.remove(images::CAPABILITY);
        if !variants.is_empty() {
            component
                .capabilities
                .required
                .push(images::CAPABILITY.into());
        }
        draft.dirty = true;
    }
    Ok(())
}

/// Export validates both platform variants and fully decodes declared images once.
pub fn validate(draft: &Draft) -> Result<(), skin::Error> {
    draft.validate_images()?;
    for (platform, descriptor) in &draft.package.platforms {
        let supported = [
            "container.zip/1".into(),
            format!("{platform}.base/1"),
            "appearance.parameters/1".into(),
            images::CAPABILITY.into(),
        ]
        .into();
        for component in descriptor
            .components
            .iter()
            .filter(|c| c.kind == "appearance")
        {
            let (_, states) =
                draft
                    .package
                    .compatibility(platform, &supported, &["appearance".into()].into())?;
            if !states[&component.id].enabled {
                continue;
            }
            for dark in [false, true] {
                let resolved = skin::parameters::resolve(
                    &draft.package,
                    platform,
                    &component.id,
                    dark,
                    &Default::default(),
                    false,
                    &supported,
                )?;
                text_render::skin_images::decode(&draft.package, &resolved.images)?;
            }
        }
    }
    Ok(())
}
