//! Package authoring stays separate from the currently applied Windows preference.
use core_engine::skin::{
    self,
    editor::Draft,
    parameters::{self, Appearance, Parameters},
    preferences::Selection,
    Extensions,
};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
};

enum Picked {
    Open(Draft),
    Exported(PathBuf),
}
pub struct SkinEditor {
    android_preview: crate::android_skin_preview::AndroidPreview,
    draft: Option<Draft>,
    pending: Option<mpsc::Receiver<Result<Picked, String>>>,
    platform: String,
    component: String,
    dark: bool,
    sample: usize,
    remove_confirm: bool,
    message: String,
    renderer: Option<text_render::TextSystem>,
    texture: Option<egui::TextureHandle>,
    preview_key: String,
    catalog: Vec<PathBuf>,
    catalog_loaded: bool,
    active_key: String,
    active_parameters: Option<Parameters>,
    active_legacy: bool,
}
impl Default for SkinEditor {
    fn default() -> Self {
        Self {
            android_preview: Default::default(),
            draft: None,
            pending: None,
            platform: "windows".into(),
            component: String::new(),
            dark: false,
            sample: 0,
            remove_confirm: false,
            message: String::new(),
            renderer: None,
            texture: None,
            preview_key: String::new(),
            catalog: Vec::new(),
            catalog_loaded: false,
            active_key: String::new(),
            active_parameters: None,
            active_legacy: false,
        }
    }
}
impl SkinEditor {
    pub fn is_dirty(&self) -> bool {
        self.draft.as_ref().is_some_and(|d| d.dirty)
    }
    fn refresh_catalog(&mut self, path: &Path) {
        self.catalog = std::fs::read_dir(skin::runtime::store_path(path))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "mywubi-skin"))
            .collect();
        self.catalog.sort();
        self.catalog_loaded = true;
    }
    fn open(&mut self, path: Option<PathBuf>, config_path: PathBuf, ctx: egui::Context) {
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let selected = path.or_else(|| {
                rfd::FileDialog::new()
                    .add_filter("MyWubi 皮肤", &["mywubi-skin", "zip"])
                    .pick_file()
            });
            if let Some(path) = selected {
                let result = (|| {
                    let draft = Draft::open(&path).map_err(|e| e.to_string())?;
                    let bytes = draft.package.encoded().map_err(|e| e.to_string())?;
                    skin::archive::import(
                        &skin::runtime::store_path(&config_path),
                        std::io::Cursor::new(bytes),
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(Picked::Open(draft))
                })();
                let _ = tx.send(result);
            }
            ctx.request_repaint();
        });
    }
    pub fn ui(&mut self, ui: &mut egui::Ui, config: &mut core_engine::Config, path: &Path) -> bool {
        let mut applied = false;
        if !self.catalog_loaded {
            self.refresh_catalog(path);
        }
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(Picked::Open(draft))) => {
                    self.platform = if draft.package.platforms.contains_key("windows") {
                        "windows"
                    } else {
                        "android"
                    }
                    .into();
                    self.component.clear();
                    self.draft = Some(draft);
                    self.pending = None;
                    self.message = "已导入并打开；当前输入法外观未改变".into();
                    self.refresh_catalog(path);
                }
                Ok(Ok(Picked::Exported(target))) => {
                    self.pending = None;
                    self.message = format!("已导出 {}", target.display());
                    match Draft::open(&target) {
                        Ok(d) => self.draft = Some(d),
                        Err(e) => self.message = e.to_string(),
                    }
                }
                Ok(Err(error)) => {
                    self.pending = None;
                    self.message = error;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        ui.separator();
        ui.heading("皮肤管理与制作");
        ui.label(if let Some(s) = &config.skins.windows.selection {
            format!("Windows 已选择：{} @ {} / {}", s.id, s.version, s.component)
        } else {
            format!("Windows 方案：{}", config.skins.windows.preset)
        });
        ui.horizontal_wrapped(|ui| {
            for (id, label) in [
                ("jade", "墨绿副本"),
                ("plain", "素简副本"),
                ("contrast", "高对比副本"),
            ] {
                if ui
                    .add_enabled(
                        self.pending.is_none() && !self.is_dirty(),
                        egui::Button::new(label),
                    )
                    .clicked()
                {
                    match Draft::builtin(id) {
                        Ok(d) => {
                            self.draft = Some(d);
                            self.component = id.into();
                            self.message = "正在编辑自定义副本".into();
                        }
                        Err(e) => self.message = e.to_string(),
                    }
                }
            }
            if ui
                .add_enabled(
                    self.pending.is_none() && !self.is_dirty(),
                    egui::Button::new("导入并编辑…"),
                )
                .clicked()
            {
                self.open(None, path.to_owned(), ui.ctx().clone());
            }
            if ui.button("刷新列表").clicked() {
                self.refresh_catalog(path);
            }
        });
        egui::ComboBox::from_label("已导入皮肤")
            .selected_text("选择包进行编辑")
            .show_ui(ui, |ui| {
                for file in self.catalog.clone() {
                    if ui
                        .add_enabled(
                            !self.is_dirty() && self.pending.is_none(),
                            egui::Button::new(
                                file.file_stem().unwrap_or_default().to_string_lossy(),
                            ),
                        )
                        .clicked()
                    {
                        self.open(Some(file), path.to_owned(), ui.ctx().clone());
                    }
                }
            });
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
        let Some(mut draft) = self.draft.take() else {
            let key = format!(
                "{:?}-{:?}-{}",
                config.appearance,
                config.skins.windows,
                ui.visuals().dark_mode
            );
            if key != self.active_key {
                self.active_key = key;
                match skin::runtime::load(path, config, "windows", ui.visuals().dark_mode, false) {
                    Ok(skin) => {
                        self.active_legacy = skin.legacy;
                        self.active_parameters = Some((*skin.parameters).clone());
                    }
                    Err(e) => self.message = e.to_string(),
                }
            }
            if let Some(p) = self.active_parameters.clone() {
                self.preview(ui, &p, self.active_legacy);
            }
            return false;
        };
        let busy = self.pending.is_some();
        ui.label(if draft.dirty {
            "草稿有修改 · 导出或放弃后可打开其他包"
        } else {
            "包已保存 · 编辑目标不等于本机应用目标"
        });
        let mut discard = false;
        ui.add_enabled_ui(!busy,|ui| {
            ui.collapsing("包信息",|ui| {
                for (label,value) in [("包 ID",&mut draft.package.metadata.id),("版本",&mut draft.package.metadata.version),
                    ("名称",&mut draft.package.metadata.name),("作者",&mut draft.package.metadata.author),("许可证",&mut draft.package.metadata.license)] {
                    ui.horizontal(|ui| {ui.label(label);draft.dirty|=ui.text_edit_singleline(value).changed();});
                }
            });
            ui.horizontal(|ui| {
                for platform in ["windows","android"] {
                    if ui.selectable_label(self.platform==platform,platform).clicked() {self.platform=platform.into();self.component.clear();self.remove_confirm=false;}
                }
            });
            if !draft.package.platforms.contains_key(&self.platform) {
                if ui.button("添加此平台入口").clicked() {if let Err(e)=draft.add_platform(&self.platform){self.message=e.to_string();}}
            } else {
                ui.horizontal(|ui| {
                    if ui.button("移除此平台…").clicked() {self.remove_confirm=true;}
                    if self.remove_confirm {
                        ui.label("移除会删除此平台草稿，保留共享资源。");
                        if ui.button("确认移除").clicked() {if let Err(e)=draft.remove_platform(&self.platform){self.message=e.to_string();}self.remove_confirm=false;}
                        if ui.button("取消移除").clicked(){self.remove_confirm=false;}
                    }
                });
            }
            ui.collapsing("共享颜色与资源",|ui| {
                ui.label("共享修改可能影响 Windows 和 Android；组件覆盖值仍优先。私有副本不会自动重绑定已有引用。");
                if let Ok(base)=parameters::builtin("jade",self.dark) {draft.dirty|=patch_form(ui,&mut draft.package.metadata.defaults,&base,true,false);}
                for (id,resource) in draft.package.metadata.resources.clone() {
                    ui.horizontal(|ui| {ui.label(format!("shared:{id} → {resource}"));
                        if ui.button("复制为当前平台资源").clicked() {
                            self.message=match draft.copy_shared_resource(&id,&self.platform){Ok(id)=>format!("已复制：{id}"),Err(e)=>e.to_string()};
                        }
                    });
                }
            });
            if let Some(platform)=draft.package.platforms.get(&self.platform) {
                if !platform.components.iter().any(|c|c.id==self.component) {self.component=platform.components.first().map(|c|c.id.clone()).unwrap_or_default();}
                egui::ComboBox::from_label("组件").selected_text(&self.component).show_ui(ui,|ui| {
                    for c in &platform.components {ui.selectable_value(&mut self.component,c.id.clone(),format!("{} · {}",c.id,c.kind));}
                });
                ui.checkbox(&mut self.dark,"深色变体");
                let supported=["container.zip/1".into(),format!("{}.base/1",self.platform),"appearance.parameters/1".into()].into();
                let resolved=parameters::resolve(&draft.package,&self.platform,&self.component,self.dark,&Extensions::new(),false,&supported);
                match resolved {
                    Ok(resolved)=>{
                        for warning in &resolved.warnings {ui.label(warning);}
                        let entry=draft.package.platforms[&self.platform].components.iter().find(|c|c.id==self.component).cloned();
                        if let Some(entry)=entry {
                            let entry_path=format!("{}/{}",self.platform,entry.path);
                            let parsed=draft.package.files.get(&entry_path).and_then(|b|std::str::from_utf8(b).ok()).and_then(|s|toml::from_str::<Appearance>(s).ok());
                            if let Some(mut appearance)=parsed {
                                if matches!(self.platform.as_str(),"windows"|"android") {
                                    let variant=if self.dark {"dark"} else {"light"};
                                    if !appearance.variants.contains_key(variant) && ui.button("创建所选变体").clicked() {
                                        appearance.variants.insert(variant.into(),resolved.parameters.0.clone());draft.dirty=true;
                                    }
                                    let mut changed=false;
                                    if let Some(patch)=appearance.variants.get_mut(variant) {ui.collapsing(if self.platform=="android" {"Android 候选栏、按键与状态参数"} else {"Windows 外观参数"},|ui| {changed=patch_form(ui,patch,&resolved.parameters,false,self.platform=="android");});}
                                    if changed || (draft.dirty && appearance.variants.contains_key(variant)) {
                                        match toml::to_string(&appearance) {Ok(s)=>{draft.package.files.insert(entry_path,s.into_bytes());draft.dirty|=changed;},Err(e)=>self.message=e.to_string()}
                                    }
                                }
                            }
                        }
                        if self.platform=="windows" {self.preview(ui,&resolved.parameters,false);} else {self.android_preview.ui(ui,&resolved.parameters);}
                    },
                    Err(e)=>{ui.label(format!("此范围只读，保留原始数据：{e}"));}
                }
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("导出新文件…").clicked() {
                    let copy=draft.clone();let ctx=ui.ctx().clone();let (tx,rx)=mpsc::channel();self.pending=Some(rx);
                    std::thread::spawn(move || {
                        if let Some(path)=rfd::FileDialog::new().set_file_name(format!("{}-{}.mywubi-skin",copy.package.metadata.id,copy.package.metadata.version)).save_file() {
                            let _=tx.send(copy.export_new(&path).map(|_|Picked::Exported(path)).map_err(|e|e.to_string()));
                        }ctx.request_repaint();
                    });
                }
                let can_apply=self.platform=="windows" && draft.package.platforms.contains_key("windows");
                if ui.add_enabled(can_apply,egui::Button::new("应用到 Windows")).clicked() {
                    let result=(|| ->Result<(),String> {
                        draft.check_source().map_err(|e|e.to_string())?;
                        let mut candidate=core_engine::Config::load(path).map_err(|e|e.to_string())?;
                        let bytes=draft.package.encoded().map_err(|e|e.to_string())?;
                        skin::archive::import(&skin::runtime::store_path(path),std::io::Cursor::new(bytes)).map_err(|e|e.to_string())?;
                        candidate.skins.windows.selection=Some(Selection{id:draft.package.metadata.id.clone(),version:draft.package.metadata.version.clone(),component:self.component.clone()});
                        for dark in [false,true] {skin::runtime::load(path,&candidate,"windows",dark,false).map_err(|e|e.to_string())?;}
                        candidate.save(path).map_err(|e|e.to_string())?;
                        config.skins.windows=candidate.skins.windows;
                        Ok(())
                    })();
                    match result {Ok(())=>{applied=true;self.message="已应用 Windows 皮肤；其他设置草稿保留".into();self.refresh_catalog(path);},Err(e)=>self.message=e}
                }
                if ui.button("放弃并关闭草稿").clicked(){discard=true;}
            });
        });
        if !discard {
            self.draft = Some(draft);
        }
        applied
    }

    fn preview(&mut self, ui: &mut egui::Ui, p: &Parameters, legacy: bool) {
        ui.horizontal_wrapped(|ui| {
            for (index, name) in ["普通候选", "长词与反查", "表情", "不可用状态"]
                .into_iter()
                .enumerate()
            {
                ui.selectable_value(&mut self.sample, index, name);
            }
        });
        let key = format!("{:?}-{}-{legacy}", p, self.sample);
        if key != self.preview_key {
            self.preview_key = key;
            self.texture = None;
            let result = (|| -> Result<(), String> {
                if self.renderer.is_none() {
                    self.renderer =
                        Some(text_render::TextSystem::new().map_err(|e| e.to_string())?);
                }
                let system = self.renderer.as_mut().ok_or("预览初始化失败")?;
                let mut data = text_render::candidate_data::CandidateData {
                    visible: true,
                    spelling: "a".into(),
                    theme: text_render::candidate_data::ThemeSnapshot::from_parameters(Arc::new(
                        p.clone(),
                    )),
                    ..Default::default()
                };
                if legacy {
                    data.theme.parameters = None;
                }
                let texts = match self.sample {
                    1 => vec!["跨平台形码输入法的候选与长文本", "internationalization"],
                    2 => vec!["😀", "👨‍👩‍👧‍👦", "👩🏽‍💻"],
                    3 => vec!["拼音码表不可用"],
                    _ => vec!["工", "戈", "或"],
                };
                data.items = texts
                    .into_iter()
                    .enumerate()
                    .map(|(i, text)| text_render::candidate_data::CandidateItem {
                        label: format!("{}. ", i + 1),
                        text: text.into(),
                        hint: if self.sample == 1 {
                            "wqvb · 反查".into()
                        } else {
                            String::new()
                        },
                        disabled: self.sample == 3,
                        expression_kind: (self.sample == 2)
                            .then_some(core_engine::expressions::ExpressionKind::Emoji),
                    })
                    .collect();
                let bitmap = text_render::preview::candidate(system, &data, 600.0)
                    .map_err(|e| e.to_string())?;
                let pixels = bitmap
                    .pixels()
                    .chunks_exact(4)
                    .map(|p| egui::Color32::from_rgba_premultiplied(p[2], p[1], p[0], p[3]))
                    .collect();
                self.texture = Some(ui.ctx().load_texture(
                    "skin-native-preview",
                    egui::ColorImage {
                        size: [bitmap.width as usize, bitmap.height as usize],
                        pixels,
                    },
                    egui::TextureOptions::LINEAR,
                ));
                Ok(())
            })();
            if let Err(e) = result {
                self.message = e;
            }
        }
        ui.label("Windows 原生候选预览");
        if let Some(texture) = &self.texture {
            egui::ScrollArea::both().max_height(280.0).show(ui, |ui| {
                ui.image((texture.id(), texture.size_vec2()));
            });
        }
    }
}

fn patch_form(
    ui: &mut egui::Ui,
    patch: &mut Extensions,
    effective: &Parameters,
    colors_only: bool,
    android: bool,
) -> bool {
    let mut changed = false;
    for (key, default) in &effective.0 {
        if colors_only && !parameters::COLORS.contains(&key.as_str()) {
            continue;
        }
        if !colors_only
            && !android
            && (key.starts_with("key_")
                || key.starts_with("function_")
                || key.starts_with("pressed_")
                || key.starts_with("bubble_")
                || key == "keyboard_height")
        {
            continue;
        }
        if android
            && matches!(
                key.as_str(),
                "font_family" | "layout" | "density" | "max_width"
            )
        {
            continue;
        }
        let mut value = patch.get(key).unwrap_or(default).clone();
        ui.horizontal(|ui| {
            ui.label(if android {
                field_label(key).replace("DIP", "dp").replace("pt", "sp")
            } else {
                field_label(key).into()
            });
            let edit = match &mut value {
                toml::Value::String(text) if parameters::COLORS.contains(&key.as_str()) => {
                    let argb = parameters::parse_color(text).unwrap_or(0xff000000);
                    let [a, r, g, b] = argb.to_be_bytes();
                    let mut color = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
                    let response = ui.color_edit_button_srgba(&mut color);
                    if response.changed() {
                        let [r, g, b, a] = color.to_srgba_unmultiplied();
                        *text = format!("#{r:02X}{g:02X}{b:02X}{a:02X}");
                    }
                    response.changed()
                }
                toml::Value::String(text) => ui.text_edit_singleline(text).changed(),
                toml::Value::Float(n) => ui.add(egui::DragValue::new(n).speed(0.5)).changed(),
                toml::Value::Integer(n) => ui.add(egui::DragValue::new(n)).changed(),
                _ => false,
            };
            if edit {
                let single = [(key.clone(), value.clone())].into();
                match parameters::validate_patch(&single) {
                    Ok(()) => {
                        patch.insert(key.clone(), value);
                        changed = true;
                    }
                    Err(_) => {
                        ui.colored_label(egui::Color32::RED, "格式或数值超出范围");
                    }
                }
            }
            if ui
                .add_enabled(patch.contains_key(key), egui::Button::new("继承"))
                .clicked()
            {
                patch.remove(key);
                changed = true;
            }
        });
    }
    changed
}
fn field_label(key: &str) -> &str {
    match key {
        "background" => "背景",
        "text" => "正文",
        "label" => "序号",
        "hint" => "提示",
        "code" => "编码",
        "selected_background" => "选中背景",
        "selected_text" => "选中正文",
        "selected_hint" => "选中提示",
        "disabled" => "禁用文字",
        "status" => "状态提示",
        "border" => "边框颜色",
        "font_family" => "字体",
        "font_size" => "字号 pt",
        "hint_size" => "提示字号 pt",
        "layout" => "布局 auto / horizontal / vertical",
        "density" => "密度 compact / comfortable",
        "padding_x" => "左右留白 DIP",
        "padding_y" => "上下留白 DIP",
        "corner_radius" => "外框圆角 DIP",
        "selected_radius" => "选中圆角 DIP",
        "border_width" => "边框宽度 DIP",
        "shadow" => "阴影 DIP",
        "max_width" => "最大宽度 DIP",
        "key_background" => "普通键背景",
        "key_text" => "普通键文字",
        "function_background" => "功能键背景",
        "function_text" => "功能键文字",
        "pressed_background" => "按下背景",
        "pressed_text" => "按下文字",
        "bubble_background" => "气泡背景",
        "bubble_text" => "气泡文字",
        "key_text_size" => "按键字号 sp",
        "bubble_text_size" => "气泡字号 sp",
        "key_radius" => "按键圆角 dp",
        "key_gap" => "按键间距 dp",
        "keyboard_height" => "推荐键盘高度 dp",
        _ => key,
    }
}
