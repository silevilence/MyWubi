//! Editable templates use the same package validator as imported author layouts.
use super::{keyboard::*, Capabilities, Component, Error, Package};
use std::collections::BTreeMap;

pub fn add(package: &mut Package, id: &str) -> Result<(), Error> {
    let (sets, columns, modes): (Vec<&str>, usize, Vec<Mode>) = match id {
        "shape14" => (vec!["qw","er","ty","ui","op","as","df","gh","jk","l","zx","cv","bn","m"],5,vec![Mode::Main]),
        _ => return Err(Error::Invalid("未知键盘模板".into())),
    };
    let mapping_id=format!("{id}.mapping");
    let charset="abcdefghijklmnopqrstuvwxyz".to_owned();
    let groups:BTreeMap<_,_>=sets.iter().enumerate().map(|(i,letters)|(format!("g{i}"),(*letters).to_owned())).collect();
    let mapping=Mapping{format:1,charset:charset.clone(),modes:modes.clone(),groups,extensions:Default::default()};
    let mut keys:Vec<_>=sets.iter().enumerate().map(|(index,letters)| {
        let directions=letters.chars().zip(["up_left+up+up_right","down_left+down+down_right","left","right"])
            .map(|(c,d)|(d.into(),KeyAction::Character(c))).collect();
        Key{id:format!("key{index}"),label:letters.to_uppercase(),x:(index%columns) as f32,y:(index/columns) as f32,
            width:1.,height:1.,role:"key".into(),action:KeyAction::Group(format!("g{index}")),long_press:None,
            directions,disabled_directions:Vec::new(),extensions:Default::default()}
    }).collect();
    keys.push(Key{id:"backspace".into(),label:"退格".into(),x:(sets.len()%columns) as f32,y:(sets.len()/columns) as f32,
        width:1.,height:1.,role:"function".into(),action:KeyAction::Backspace,long_press:None,directions:Default::default(),
        disabled_directions:Vec::new(),extensions:Default::default()});
    let layout=Layout{format:1,modes,charset,width:columns as f32,height:keys.len().div_ceil(columns) as f32,
        mapping:Some(mapping_id.clone()),recommended_appearance:None,fallback:"builtin.26".into(),required_styles:Vec::new(),
        optional_styles:Default::default(),directions:Default::default(),keys,landscape:None,extensions:Default::default()};
    layout.validate_geometry(Some(&mapping),&[KeyAction::Space,KeyAction::Enter,KeyAction::Cancel])?;
    let components=&mut package.platforms.get_mut("android").ok_or_else(||Error::Invalid("缺少 Android 平台".into()))?.components;
    if components.iter().any(|c|c.id==id || c.id==mapping_id) {return Err(Error::Invalid("模板组件已存在".into()));}
    package.files.insert(format!("android/layouts/{id}.toml"),toml::to_string(&layout)?.into_bytes());
    package.files.insert(format!("android/mappings/{id}.toml"),toml::to_string(&mapping)?.into_bytes());
    components.push(Component{id:mapping_id.clone(),version:"1".into(),kind:"mapping".into(),path:format!("mappings/{id}.toml"),
        capabilities:Capabilities{required:vec![GROUP_CAPABILITY.into()],..Default::default()},dependencies:Default::default(),extensions:Default::default()});
    components.push(Component{id:id.into(),version:"1".into(),kind:"layout".into(),path:format!("layouts/{id}.toml"),
        capabilities:Capabilities{required:vec![LAYOUT_CAPABILITY.into(),GROUP_CAPABILITY.into(),GESTURE_CAPABILITY.into(),MERGE_CAPABILITY.into()],..Default::default()},
        dependencies:[(mapping_id,"1".into())].into(),extensions:Default::default()});
    Ok(())
}
pub fn package() -> Result<Package,Error> {
    let mut package=super::parameters::builtin_package("jade")?;
    package.metadata.id="mywubi.keyboards".into();package.metadata.version="1".into();package.metadata.name="内置键盘作者模板".into();
    add(&mut package,"shape14")?;
    Ok(package)
}
