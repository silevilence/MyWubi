use eframe::egui;
use personal_input::{api::Command, exchange::ImportMode};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};

pub fn run_background(config: &Path) {
    let Ok(_lease) = personal_input::background::process_lease(config) else {
        return;
    };
    let Ok(worker) = personal_input::background::Worker::new() else {
        return;
    };
    let memory = model_ipc::MemoryPressure::new().ok();
    loop {
        let Ok(settings) = core_engine::Config::load(config) else {
            return;
        };
        if !settings.personal.learn {
            return;
        }
        let mut input = windows::Win32::UI::Input::KeyboardAndMouse::LASTINPUTINFO {
            cbSize: std::mem::size_of::<windows::Win32::UI::Input::KeyboardAndMouse::LASTINPUTINFO>(
            ) as u32,
            dwTime: 0,
        };
        let idle = unsafe {
            windows::Win32::UI::Input::KeyboardAndMouse::GetLastInputInfo(&mut input).as_bool()
                && windows::Win32::System::SystemInformation::GetTickCount()
                    .wrapping_sub(input.dwTime)
                    > 30_000
        };
        let allowed = model_ipc::auto_gpu_power_allowed()
            && !memory
                .as_ref()
                .and_then(|m| m.is_low().ok())
                .unwrap_or(false);
        worker.update(config.to_owned(), allowed, !idle);
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[derive(Default)]
pub struct PersonalPanel {
    password: String,
    encrypted: bool,
    replace: bool,
    clear_models: bool,
    clear_confirm: bool,
    selected: Option<PathBuf>,
    preview: Option<serde_json::Value>,
    worker: Option<mpsc::Receiver<Result<serde_json::Value, String>>>,
    status: String,
}
impl PersonalPanel {
    fn start(&mut self, config: &Path, action: impl FnOnce() -> Option<Command> + Send + 'static) {
        let path = config.to_owned();
        let (tx, rx) = mpsc::channel();
        self.worker = Some(rx);
        std::thread::spawn(move || {
            let result = action()
                .map(|c| personal_input::api::execute(&path, c).map_err(|e| e.to_string()))
                .unwrap_or(Ok(serde_json::Value::Null));
            let _ = tx.send(result);
        });
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        config_path: &Path,
        settings: &mut core_engine::personal::PersonalConfig,
    ) -> bool {
        if let Some(result) = self.worker.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.worker = None;
            match result {
                Ok(value) if value.get("revision").is_some() => {
                    self.preview = Some(value);
                    self.status = "请核对导入影响后执行".into();
                }
                Ok(value) => {
                    self.status = if value.get("data").is_some() {
                        format!(
                            "记录 {} 条，占用 {} 字节，缺失 {} 条；个人模型：{}；学习：{}",
                            value["data"]["records"],
                            value["data"]["bytes"],
                            value["data"]["gaps"],
                            value.get("model").filter(|m| !m.is_null()).map_or(
                                "待积累/未就绪".into(),
                                |m| format!(
                                    "版本 {}，样本 {}，更新时间 {}",
                                    m["version"], m["samples"], m["updated"]
                                )
                            ),
                            value.get("learning").filter(|v| !v.is_null()).map_or(
                                "等待输入法启动".into(),
                                |v| format!(
                                    "{} {}，上一批 {} ms，1 线程 / 2 秒批次 / 8 MB 模型",
                                    v["phase"], v["reason"], v["elapsed_ms"]
                                )
                            )
                        )
                    } else {
                        value
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("操作完成")
                            .into()
                    };
                    self.preview = None;
                }
                Err(error) => self.status = error,
            }
        }
        let mut changed = false;
        ui.collapsing("个人用预测模型（实验性）",|ui| {
            ui.label("输入数据、学习与预测均在本地。仅记录宿主可可靠观察的范围；密码和禁止个性化学习的字段不收集。");
            changed|=ui.checkbox(&mut settings.collect,"个人输入数据收集").changed();
            changed|=ui.checkbox(&mut settings.learn,"后台学习").changed();
            changed|=ui.checkbox(&mut settings.predict,"使用个人用预测模型").changed();
            changed|=ui.add(egui::Slider::new(&mut settings.max_mb,1..=128).text("记录容量 MB")).changed();
            changed|=ui.add(egui::Slider::new(&mut settings.retention_days,1..=3650).text("保留天数")).changed();
            ui.small("三个开关独立，保存后生效。停止收集仍可使用已有模型。算法尚未通过真实个人数据的质量验收。");
            ui.add_enabled_ui(self.worker.is_none(),|ui| {
                ui.horizontal(|ui| {
                    if ui.button("刷新记录与模型状态").clicked(){self.start(config_path,||Some(Command::Status));}
                    if ui.button("请求立即更新").clicked(){
                        if let Ok(exe)=std::env::current_exe(){let _=personal_input::background::start_desktop(&exe,config_path);}
                        self.start(config_path,||Some(Command::Schedule));
                    }
                    if ui.button("回退上一版本").clicked(){self.start(config_path,||Some(Command::Rollback));}
                });
                ui.checkbox(&mut self.encrypted,"使用密码加密导出");
                ui.add(egui::TextEdit::singleline(&mut self.password).password(true).hint_text("导出或导入密码"));
                ui.small("密码不可恢复。明文包包含输入与上下文，请自行保管；交换文件不需要上传服务。");
                if ui.button("导出个人数据").clicked(){
                    let password=self.encrypted.then(||self.password.clone());
                    self.start(config_path,move || rfd::FileDialog::new().set_file_name("personal.mwp").save_file().map(|path|Command::Export{path,password}));
                }
                if ui.button("选择导入文件").clicked(){
                    if let Some(path)=rfd::FileDialog::new().add_filter("个人数据",&["mwp"]).pick_file(){self.selected=Some(path);self.preview=None;}
                }
                if ui.checkbox(&mut self.replace,"覆盖已有个人数据（否则合并）").changed(){self.preview=None;}
                if let Some(path)=self.selected.clone(){
                    ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                    if ui.button("校验并预览").clicked(){
                        let password=(!self.password.is_empty()).then(||self.password.clone());
                        let mode=if self.replace{ImportMode::Replace}else{ImportMode::Merge};
                        self.start(config_path,move||Some(Command::Preview{path,password,mode}));
                    }
                }
                if let Some(preview)=self.preview.clone(){
                    ui.label(format!("新增 {}，重复 {}，冲突 {}，将替换 {} 条",preview["added"],preview["duplicates"],preview["conflicts"],preview["replaced"]));
                    if ui.add_enabled(preview["conflicts"]==0,egui::Button::new("确认执行导入")).clicked(){
                        if let Some(path)=self.selected.clone(){
                            let password=(!self.password.is_empty()).then(||self.password.clone());
                            let mode=if self.replace{ImportMode::Replace}else{ImportMode::Merge};
                            let revision=preview["revision"].as_str().unwrap_or_default().to_owned();
                            self.start(config_path,move||Some(Command::Import{path,password,mode,revision}));
                        }
                    }
                }
                ui.checkbox(&mut self.clear_models,"清除时同时删除个人模型及上一版本");
                ui.small("只清除记录与训练缓存不会删除已有模型知识；需要彻底清除时勾选同时删除模型。");
                if ui.button(if self.clear_confirm{"确认清除所选个人数据"}else{"清除个人数据…"}).clicked(){
                    if self.clear_confirm{let models=self.clear_models;self.start(config_path,move||Some(Command::Clear{models}));self.clear_confirm=false;}
                    else{self.clear_confirm=true;}
                }
            });
            if self.worker.is_some(){ui.spinner();ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));}
            ui.label(&self.status);
        });
        changed
    }
}
