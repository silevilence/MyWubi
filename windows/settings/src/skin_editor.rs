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
    Open(Box<Draft>),
    Exported(PathBuf),
    Image {
        platform: Option<String>,
        id: String,
        bytes: Vec<u8>,
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Appearance,
    Metadata,
    Images,
    Shared,
}
pub struct SkinEditor {
    section: Section,
    field_errors: std::collections::HashMap<egui::Id, String>,
    revision: u64,
    keyboard_editor: crate::keyboard_editor::KeyboardEditor,
    android_preview: crate::android_skin_preview::AndroidPreview,
    draft: Option<Draft>,
    pending: Option<mpsc::Receiver<Result<Picked, String>>>,
    platform: String,
    component: String,
    dark: bool,
    sample: usize,
    remove_confirm: bool,
    discard_confirm: bool,
    preview_parameters: Option<(Parameters, bool, Arc<text_render::skin_images::Cache>)>,
    message: String,
    renderer: Option<text_render::TextSystem>,
    texture: Option<egui::TextureHandle>,
    preview_key: String,
    catalog: Vec<PathBuf>,
    catalog_loaded: bool,
    active_key: String,
    active_parameters: Option<Parameters>,
    active_legacy: bool,
    active_images: Arc<text_render::skin_images::Cache>,
    image_id: String,
    image_key: u64,
    image_cache: Arc<text_render::skin_images::Cache>,
}
impl Default for SkinEditor {
    fn default() -> Self {
        Self {
            section: Section::Appearance,
            field_errors: Default::default(),
            revision: 0,
            keyboard_editor: Default::default(),
            android_preview: Default::default(),
            draft: None,
            pending: None,
            platform: "windows".into(),
            component: String::new(),
            dark: false,
            sample: 0,
            remove_confirm: false,
            discard_confirm: false,
            preview_parameters: None,
            message: String::new(),
            renderer: None,
            texture: None,
            preview_key: String::new(),
            catalog: Vec::new(),
            catalog_loaded: false,
            active_key: String::new(),
            active_parameters: None,
            active_legacy: false,
            active_images: Default::default(),
            image_id: "image".into(),
            image_key: 0,
            image_cache: Default::default(),
        }
    }
}
impl SkinEditor {
    pub fn is_dirty(&self) -> bool {
        self.draft.as_ref().is_some_and(|d| d.dirty)
            || self.keyboard_editor.is_dirty()
            || !self.field_errors.is_empty()
    }
    pub(crate) fn prepare_capture(&mut self, case: &str) {
        match case {
            "skin-light" => {
                self.discard();
                if let Err(e) = self.create("jade") {
                    self.message = e;
                }
            }
            "skin-dark" => self.dark = true,
            "skin-compact" => {
                self.platform = "windows".into();
                self.component = "jade".into();
            }
            "skin-keyboard" => {
                if let Some(draft) = &mut self.draft {
                    if let Err(e) = skin::keyboard_templates::add(&mut draft.package, "full26") {
                        self.message = e.to_string();
                    }
                    self.component = "full26".into();
                }
            }
            "skin-android" => {
                self.platform = "android".into();
                self.component = "jade".into();
                self.dark = false;
            }
            _ => {}
        }
    }
    pub fn is_busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn title(&self) -> &str {
        self.draft.as_ref().map_or("新建或打开皮肤", |d| {
            d.package.metadata.name.as_str()
        })
    }
    pub fn has_draft(&self) -> bool {
        self.draft.is_some()
    }
    pub fn discard(&mut self) {
        self.field_errors.clear();
        self.revision = self.revision.wrapping_add(1);
        self.draft = None;
        self.keyboard_editor = Default::default();
        self.preview_parameters = None;
        self.discard_confirm = false;
    }
    fn replace_draft(&mut self, draft: Draft) {
        self.field_errors.clear();
        self.revision = self.revision.wrapping_add(1);
        self.platform = if draft.package.platforms.contains_key("windows") {
            "windows"
        } else {
            "android"
        }
        .into();
        self.component = draft
            .package
            .platforms
            .get(&self.platform)
            .and_then(|p| p.components.first())
            .map(|c| c.id.clone())
            .unwrap_or_default();
        self.keyboard_editor = Default::default();
        self.section = Section::Appearance;
        self.preview_key.clear();
        self.image_key = 0;
        self.draft = Some(draft);
    }
    pub fn create(&mut self, preset: &str) -> Result<(), String> {
        if self.is_dirty() || self.is_busy() {
            return Err("请先导出或放弃当前草稿".into());
        }
        self.replace_draft(Draft::builtin(preset).map_err(|e| e.to_string())?);
        Ok(())
    }
    pub fn open_file(&mut self, path: &Path) -> Result<(), String> {
        if self.is_dirty() || self.is_busy() {
            return Err("请先导出或放弃当前草稿".into());
        }
        self.replace_draft(Draft::open(path).map_err(|e| e.to_string())?);
        Ok(())
    }
    pub fn export_file(&mut self, path: &Path) -> Result<(), String> {
        if !self.field_errors.is_empty() {
            return Err("请修正外观参数中的无效输入".into());
        }
        if self.keyboard_editor.is_dirty() {
            return Err("请先校验并写入键盘组件草稿".into());
        }
        let draft = self.draft.as_ref().ok_or("请先新建或打开皮肤")?;
        crate::skin_image_editor::validate(draft).map_err(|e| e.to_string())?;
        draft.export_new(path).map_err(|e| e.to_string())?;
        self.replace_draft(Draft::open(path).map_err(|e| e.to_string())?);
        Ok(())
    }
    pub fn set_parameter(
        &mut self,
        platform: &str,
        component: &str,
        dark: bool,
        key: &str,
        value: toml::Value,
    ) -> Result<(), String> {
        let draft = self.draft.as_mut().ok_or("请先新建或打开皮肤")?;
        let patch = [(key.to_owned(), value.clone())].into();
        parameters::validate_patch(&patch).map_err(|e| e.to_string())?;
        let entry = draft
            .package
            .platforms
            .get(platform)
            .and_then(|p| {
                p.components
                    .iter()
                    .find(|c| c.id == component && c.kind == "appearance")
            })
            .ok_or("外观组件不存在")?;
        let path = format!("{platform}/{}", entry.path);
        let bytes = draft.package.files.get(&path).ok_or("组件文件不存在")?;
        let mut appearance: Appearance =
            toml::from_str(std::str::from_utf8(bytes).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        appearance
            .variants
            .entry(if dark { "dark" } else { "light" }.into())
            .or_default()
            .insert(key.into(), value);
        draft.package.files.insert(
            path,
            toml::to_string(&appearance)
                .map_err(|e| e.to_string())?
                .into_bytes(),
        );
        draft.dirty = true;
        Ok(())
    }
    pub fn apply(&mut self, path: &Path) -> Result<(), String> {
        if !self.field_errors.is_empty() {
            return Err("请修正外观参数中的无效输入".into());
        }
        if self.keyboard_editor.is_dirty() {
            return Err("请先校验并写入键盘组件草稿".into());
        }
        let draft = self.draft.as_ref().ok_or("请先新建或打开皮肤")?;
        draft.check_source().map_err(|e| e.to_string())?;
        crate::skin_image_editor::validate(draft).map_err(|e| e.to_string())?;
        let mut candidate = core_engine::Config::load(path).map_err(|e| e.to_string())?;
        let bytes = draft.package.encoded().map_err(|e| e.to_string())?;
        skin::archive::import(
            &skin::runtime::store_path(path),
            std::io::Cursor::new(bytes),
        )
        .map_err(|e| e.to_string())?;
        candidate.skins.windows.selection = Some(Selection {
            id: draft.package.metadata.id.clone(),
            version: draft.package.metadata.version.clone(),
            component: self.component.clone(),
        });
        for dark in [false, true] {
            let skin = skin::runtime::load(path, &candidate, "windows", dark, false)
                .map_err(|e| e.to_string())?;
            if let Some(package) = &skin.package {
                text_render::skin_images::decode(package, &skin.images)
                    .map_err(|e| e.to_string())?;
            }
        }
        candidate.save(path).map_err(|e| e.to_string())?;
        self.refresh_catalog(path);
        self.message = "已应用到 Windows。需要保留皮肤文件时，请另行导出。".into();
        Ok(())
    }
    pub fn show_preview(&mut self, ui: &mut egui::Ui) {
        if let Some((parameters, legacy, images)) = self.preview_parameters.clone() {
            if self.platform == "android" && self.has_draft() {
                self.android_preview.ui(ui, &parameters, &images);
            } else {
                self.preview(ui, &parameters, legacy, images);
            }
        } else {
            ui.label("选择外观组件后，在这里预览候选框。");
        }
    }
    pub fn actions(&mut self, ui: &mut egui::Ui, path: &Path) -> bool {
        let mut applied = false;
        let ready = self.draft.is_some()
            && !self.is_busy()
            && !self.keyboard_editor.is_dirty()
            && self.field_errors.is_empty();
        ui.horizontal_wrapped(|ui| {
            if crate::ui_style::primary(ui, "导出皮肤文件…", ready).clicked() {
                self.export_dialog(ui.ctx().clone());
            }
            let windows = self.platform == "windows" && ready;
            if ui
                .add_enabled(windows, egui::Button::new("应用到 Windows"))
                .clicked()
            {
                match self.apply(path) {
                    Ok(()) => applied = true,
                    Err(e) => self.message = e,
                }
            }
            if ui
                .add_enabled(
                    self.has_draft() && !self.is_busy(),
                    egui::Button::new("关闭草稿"),
                )
                .clicked()
            {
                if self.is_dirty() {
                    self.discard_confirm = true;
                } else {
                    self.discard();
                }
            }
            if self.is_busy() {
                ui.spinner();
                ui.label("正在处理…");
            } else {
                ui.label(if self.is_dirty() {
                    "草稿未导出"
                } else {
                    "没有未导出的修改"
                });
            }
        });
        if self.keyboard_editor.is_dirty() {
            ui.colored_label(
                crate::ui_style::ERROR,
                "键盘修改尚未写入组件，请先校验并写入，或放弃组件修改。",
            );
        }
        for message in self.field_errors.values() {
            ui.colored_label(crate::ui_style::ERROR, message);
        }
        if self.discard_confirm {
            ui.horizontal_wrapped(|ui| {
                ui.label("关闭将丢弃尚未导出的修改。");
                if ui.button("放弃并关闭草稿").clicked() {
                    self.discard();
                }
                if ui.button("继续编辑").clicked() {
                    self.discard_confirm = false;
                }
            });
        }
        applied
    }
    pub fn export_dialog(&mut self, ctx: egui::Context) {
        let Some(copy) = self.draft.clone() else {
            return;
        };
        if self.is_busy() || self.keyboard_editor.is_dirty() || !self.field_errors.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("MyWubi 皮肤", &["mywubi-skin"])
                .set_file_name(format!(
                    "{}-{}.mywubi-skin",
                    copy.package.metadata.id, copy.package.metadata.version
                ))
                .save_file()
            {
                let _ = tx.send(
                    crate::skin_image_editor::validate(&copy)
                        .and_then(|_| copy.export_new(&path))
                        .map(|_| Picked::Exported(path))
                        .map_err(|e| e.to_string()),
                );
            }
            ctx.request_repaint();
        });
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
                    Ok(Picked::Open(Box::new(draft)))
                })();
                let _ = tx.send(result);
            }
            ctx.request_repaint();
        });
    }
    pub fn ui(&mut self, ui: &mut egui::Ui, config: &mut core_engine::Config, path: &Path) -> bool {
        if !self.catalog_loaded {
            self.refresh_catalog(path);
        }
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(Picked::Open(draft))) => {
                    self.replace_draft(*draft);
                    self.pending = None;
                    self.message = "已导入并打开；当前输入法外观未改变".into();
                    self.refresh_catalog(path);
                }
                Ok(Ok(Picked::Exported(target))) => {
                    self.pending = None;
                    self.message = format!("已导出 {}", target.display());
                    match Draft::open(&target) {
                        Ok(d) => self.replace_draft(d),
                        Err(e) => self.message = e.to_string(),
                    }
                }
                Ok(Ok(Picked::Image {
                    platform,
                    id,
                    bytes,
                })) => {
                    self.pending = None;
                    if let Some(draft) = self.draft.as_mut() {
                        self.message = match draft.import_png(platform.as_deref(), &id, bytes) {
                            Ok(()) => "图片已加入草稿；请在角色中绑定".into(),
                            Err(e) => e.to_string(),
                        };
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
        if self.draft.is_none() {
            self.management_ui(ui, config, path);
        } else {
            ui.collapsing("新建或打开其他皮肤", |ui| {
                self.management_ui(ui, config, path)
            });
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
                        match skin
                            .package
                            .as_ref()
                            .map(|p| text_render::skin_images::decode(p, &skin.images))
                            .transpose()
                        {
                            Ok(cache) => self.active_images = Arc::new(cache.unwrap_or_default()),
                            Err(error) => {
                                self.message = error.to_string();
                                return false;
                            }
                        }
                        self.active_legacy = skin.legacy;
                        self.active_parameters = Some((*skin.parameters).clone());
                    }
                    Err(e) => self.message = e.to_string(),
                }
            }
            if let Some(p) = self.active_parameters.clone() {
                self.preview_parameters = Some((p, self.active_legacy, self.active_images.clone()));
            }
            return false;
        };
        let busy = self.pending.is_some();
        ui.add_enabled_ui(!busy,|ui| {
            ui.horizontal_wrapped(|ui| {
                for platform in ["windows","android"] {
                    if ui.add_enabled(!self.keyboard_editor.is_dirty() && self.field_errors.is_empty(),egui::SelectableLabel::new(self.platform==platform,platform)).clicked() {self.platform=platform.into();self.component.clear();self.remove_confirm=false;}
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (section, label) in [(Section::Appearance,"外观参数"), (Section::Metadata,"包与平台"), (Section::Images,"图片资源"), (Section::Shared,"共享样式")] {
                    ui.selectable_value(&mut self.section, section, label);
                }
            });
            ui.separator();

            if self.section == Section::Metadata { ui.vertical(|ui| {
                for (label,value) in [("包 ID",&mut draft.package.metadata.id),("版本",&mut draft.package.metadata.version),
                    ("名称",&mut draft.package.metadata.name),("作者",&mut draft.package.metadata.author),("许可证",&mut draft.package.metadata.license)] {
                    ui.horizontal_wrapped(|ui| {ui.label(label);draft.dirty|=ui.text_edit_singleline(value).changed();});
                }
            });
            }
            if self.section == Section::Images { ui.vertical(|ui| {
                ui.label("相同 ID 将替换该资源；共享替换影响引用它的 Windows 和 Android。先复制为本平台资源可隔离修改。");
                ui.horizontal_wrapped(|ui|{ui.label("资源 ID");ui.text_edit_singleline(&mut self.image_id);});
                for (label,scope) in [("导入共享 PNG",None),("导入当前平台 PNG",Some(self.platform.clone()))] {
                    if ui.button(label).clicked() {
                        let id=self.image_id.clone();let ctx=ui.ctx().clone();let (tx,rx)=mpsc::channel();self.pending=Some(rx);
                        std::thread::spawn(move || {
                            if let Some(path)=rfd::FileDialog::new().add_filter("PNG",&["png"]).pick_file() {
                                let result=(|| {use std::io::Read;let mut bytes=Vec::new();std::fs::File::open(path).map_err(|e|e.to_string())?.take(skin::archive::MAX_ENTRY+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
                                    if bytes.len() as u64>skin::archive::MAX_ENTRY {return Err("图片文件超限".into());}
                                    skin::images::dimensions(&bytes).map_err(|e|e.to_string())?;Ok(Picked::Image{platform:scope,id,bytes})})();let _=tx.send(result);
                            }ctx.request_repaint();
                        });
                    }
                }
                let resources:Vec<_>=draft.package.metadata.resources.keys().map(|id|(None,id.clone())).chain(draft.package.platforms.get(&self.platform).into_iter().flat_map(|p|p.resources.keys()).map(|id|(Some(self.platform.clone()),id.clone()))).collect();
                for (scope,id) in resources {ui.horizontal_wrapped(|ui|{ui.label(format!("{}:{id}",scope.as_deref().unwrap_or("shared")));if ui.button("删除资源").clicked(){self.message=match draft.remove_resource(scope.as_deref(),&id){Ok(())=>"未引用资源已删除".into(),Err(e)=>e.to_string()};}});}
            });
            }
            if self.section == Section::Metadata {
            if !draft.package.platforms.contains_key(&self.platform) {
                if ui.button("添加此平台入口").clicked() {if let Err(e)=draft.add_platform(&self.platform){self.message=e.to_string();}}
            } else {
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!self.keyboard_editor.is_dirty(),egui::Button::new("移除此平台…")).clicked() {self.remove_confirm=true;}
                    if self.remove_confirm {
                        ui.label("移除会删除此平台草稿，保留共享资源。");
                        if ui.button("确认移除").clicked() {if let Err(e)=draft.remove_platform(&self.platform){self.message=e.to_string();}self.remove_confirm=false;}
                        if ui.button("取消移除").clicked(){self.remove_confirm=false;}
                    }
                });
            }
            }
            if self.section == Section::Shared { ui.vertical(|ui| {
                ui.label("共享修改可能影响 Windows 和 Android；组件覆盖值仍优先。私有副本不会自动重绑定已有引用。");
                if let Ok(base)=parameters::builtin("jade",self.dark) {draft.dirty|=patch_form(ui,&mut draft.package.metadata.defaults,&base,true,false,&mut self.field_errors,self.revision);}
                for (id,resource) in draft.package.metadata.resources.clone() {
                    ui.horizontal_wrapped(|ui| {ui.label(format!("shared:{id} → {resource}"));
                        if ui.button("复制为当前平台资源").clicked() {
                            self.message=match draft.copy_shared_resource(&id,&self.platform){Ok(id)=>format!("已复制：{id}"),Err(e)=>e.to_string()};
                        }
                    });
                }
            });
            }
            if self.section == Section::Metadata && self.platform=="android" {
                ui.horizontal_wrapped(|ui| {for id in ["full26","shape14","shape9","pinyin9"] {
                    if ui.add_enabled(!self.keyboard_editor.is_dirty(),egui::Button::new(format!("添加 {id} 模板"))).clicked() {
                        match skin::keyboard_templates::add(&mut draft.package,id) {Ok(())=>{draft.dirty=true;self.component=id.into();},Err(e)=>self.message=e.to_string()}
                    }
                }});
            }
            if self.section == Section::Appearance {
            if let Some(platform)=draft.package.platforms.get(&self.platform) {
                if !platform.components.iter().any(|c|c.id==self.component) {self.component=platform.components.first().map(|c|c.id.clone()).unwrap_or_default();}
                egui::ComboBox::from_label("组件").selected_text(&self.component).show_ui(ui,|ui| {
                    for c in &platform.components {ui.add_enabled_ui(!self.keyboard_editor.is_dirty() && self.field_errors.is_empty(), |ui| {ui.selectable_value(&mut self.component,c.id.clone(),format!("{} · {}",c.id,c.kind));});}
                });
                let is_layout=self.platform=="android" && platform.components.iter().any(|c|c.id==self.component && c.kind=="layout");
                if is_layout {self.preview_parameters=None;self.keyboard_editor.ui(ui,&mut draft,&self.component);} else {
                ui.checkbox(&mut self.dark,"深色变体");
                let supported=["container.zip/1".into(),format!("{}.base/1",self.platform),"appearance.parameters/1".into(),skin::images::CAPABILITY.into()].into();
                if let Err(e)=crate::skin_image_editor::form(ui,&mut draft,&self.platform,&self.component,self.dark){self.message=e.to_string();}
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
                                    if let Some(patch)=appearance.variants.get_mut(variant) {egui::CollapsingHeader::new(if self.platform=="android" {"Android 候选栏、按键与状态参数"} else {"Windows 外观参数"}).default_open(true).show(ui,|ui| {changed=patch_form(ui,patch,&resolved.parameters,false,self.platform=="android",&mut self.field_errors,self.revision);});}
                                    if changed || (draft.dirty && appearance.variants.contains_key(variant)) {
                                        match toml::to_string(&appearance) {Ok(s)=>{draft.package.files.insert(entry_path,s.into_bytes());draft.dirty|=changed;},Err(e)=>self.message=e.to_string()}
                                    }
                                }
                            }
                        }
                        use std::hash::{Hash,Hasher};let mut hasher=std::collections::hash_map::DefaultHasher::new();
                        format!("{:?}",resolved.images).hash(&mut hasher);
                        for image in resolved.images.values(){draft.package.files[&image.path].hash(&mut hasher);}
                        let key=hasher.finish();
                        if key!=self.image_key {
                            self.image_key=key;
                            match text_render::skin_images::decode(&draft.package,&resolved.images) {Ok(cache)=>self.image_cache=Arc::new(cache),Err(e)=>{self.image_cache=Default::default();self.message=e.to_string();}}
                        }
                        self.preview_parameters=Some((resolved.parameters,false,self.image_cache.clone()));
                    },
                    Err(e)=>{ui.label(format!("此范围只读，保留原始数据：{e}"));}
                }
                }
            }
            }
        });
        self.draft = Some(draft);
        false
    }

    fn management_ui(&mut self, ui: &mut egui::Ui, config: &core_engine::Config, path: &Path) {
        crate::ui_style::heading(
            ui,
            "皮肤工作区",
            "从内置方案创建副本，或打开已有皮肤继续编辑。",
        );
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
                    match self.create(id) {
                        Ok(()) => {
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
    }

    fn preview(
        &mut self,
        ui: &mut egui::Ui,
        p: &Parameters,
        legacy: bool,
        images: Arc<text_render::skin_images::Cache>,
    ) {
        ui.horizontal_wrapped(|ui| {
            for (index, name) in ["普通候选", "长词与反查", "表情", "不可用状态"]
                .into_iter()
                .enumerate()
            {
                ui.selectable_value(&mut self.sample, index, name);
            }
        });
        let key = format!(
            "{:?}-{}-{legacy}-{:p}",
            p,
            self.sample,
            Arc::as_ptr(&images)
        );
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
                data.theme.images = images;
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
                    .as_chunks::<4>()
                    .0
                    .iter()
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
        ui.add(
            egui::Label::new(egui::RichText::new("Windows 候选预览 · 96 DPI 基准").small())
                .wrap(true),
        );
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
    errors: &mut std::collections::HashMap<egui::Id, String>,
    revision: u64,
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
        ui.push_id((revision, colors_only, key), |ui| {
            let error_id = ui.make_persistent_id("field-error");
            ui.horizontal_wrapped(|ui| {
                let label = if android {
                    field_label(key).replace("DIP", "dp").replace("pt", "sp")
                } else {
                    field_label(key).into()
                };
                ui.add_sized([130.0, 28.0], egui::Label::new(label));
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
                        ui.monospace(text.as_str());
                        response.changed()
                    }
                    toml::Value::String(text) if key == "layout" || key == "density" => {
                        let before = text.clone();
                        let options: &[(&str, &str)] = if key == "layout" {
                            &[
                                ("auto", "自动排列"),
                                ("horizontal", "横向排列"),
                                ("vertical", "纵向排列"),
                            ]
                        } else {
                            &[("compact", "紧凑"), ("comfortable", "宽松")]
                        };
                        egui::ComboBox::from_id_source(key)
                            .selected_text(
                                options
                                    .iter()
                                    .find(|(id, _)| *id == text)
                                    .map_or(text.as_str(), |(_, name)| *name),
                            )
                            .show_ui(ui, |ui| {
                                for (id, label) in options {
                                    ui.selectable_value(text, (*id).into(), *label);
                                }
                            });
                        *text != before
                    }
                    toml::Value::String(text) => {
                        let (buffer, response) =
                            crate::ui_style::text_buffer(ui, "parameter", text);
                        let valid = parameters::validate_patch(
                            &[(key.clone(), toml::Value::String(buffer.clone()))].into(),
                        )
                        .is_ok();
                        if !valid {
                            errors.insert(error_id, format!("{}：请输入有效值", field_label(key)));
                        } else {
                            errors.remove(&error_id);
                        }
                        let commit = valid && response.changed();
                        if commit {
                            *text = buffer;
                        }
                        commit
                    }
                    toml::Value::Float(n) => ui
                        .add(
                            egui::DragValue::new(n)
                                .speed(0.5)
                                .clamp_range(parameter_range(key)),
                        )
                        .changed(),
                    toml::Value::Integer(n) => ui
                        .add(egui::DragValue::new(n).clamp_range(parameter_range(key)))
                        .changed(),
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
                            ui.colored_label(crate::ui_style::ERROR, "格式或数值超出范围");
                        }
                    }
                }
                if ui
                    .add_enabled(patch.contains_key(key), egui::Button::new("恢复继承"))
                    .clicked()
                {
                    errors.remove(&error_id);
                    patch.remove(key);
                    changed = true;
                }
            });
        });
    }
    changed
}
fn parameter_range(key: &str) -> std::ops::RangeInclusive<f64> {
    match key {
        "font_size" | "hint_size" | "key_text_size" | "bubble_text_size" => 8.0..=96.0,
        "border_width" => 0.0..=8.0,
        "max_width" => 100.0..=2000.0,
        "keyboard_height" => 160.0..=600.0,
        _ => 0.0..=64.0,
    }
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
        "layout" => "候选排列",
        "density" => "间距",
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
