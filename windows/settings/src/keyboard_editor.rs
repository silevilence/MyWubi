//! Android layout editing is a schematic; pointer simulation does not send host input.
use core_engine::skin::{self,editor::Draft,keyboard::{Layout,Mapping,Key,KeyAction}};
use eframe::egui;

pub struct KeyboardEditor {
    source: Vec<u8>, id:String, working:Option<(Layout,Option<Mapping>)>,
    selected:usize, width:f32, font_scale:f32, landscape:bool, message:String,
}
impl Default for KeyboardEditor {
    fn default()->Self {Self{source:Vec::new(),id:String::new(),working:None,selected:0,width:393.,font_scale:1.,landscape:false,message:String::new()}}
}
impl KeyboardEditor {
    pub fn ui(&mut self,ui:&mut egui::Ui,draft:&mut Draft,id:&str) {
        let Some(component)=draft.package.platforms.get("android").and_then(|p|p.components.iter().find(|c|c.id==id)).cloned() else {return;};
        let source=draft.package.files.get(&format!("android/{}",component.path)).cloned().unwrap_or_default();
        if self.id!=id || self.source!=source {
            self.id=id.into();self.source=source;self.selected=0;
            match skin::keyboard::layout(&draft.package,&component) {Ok(value)=>{self.working=Some(value);self.message.clear();},Err(e)=>{self.working=None;self.message=e.to_string();}}
        }
        ui.label("Android 布局示意预览 · 修改先留在组件草稿，校验通过后才能写入包。");
        let Some((layout,mapping))=&mut self.working else {ui.label(&self.message);return;};
        egui::ComboBox::from_label("推荐外观组件").selected_text(layout.recommended_appearance.as_deref().unwrap_or("运行端独立选择")).show_ui(ui,|ui| {
            ui.selectable_value(&mut layout.recommended_appearance,None,"运行端独立选择");
            for c in &draft.package.platforms["android"].components {if c.kind=="appearance" {ui.selectable_value(&mut layout.recommended_appearance,Some(c.id.clone()),&c.id);}}
        });
        if let Some(id)=&layout.recommended_appearance {if let Err(e)=skin::combination::validate_layout_appearance(&draft.package,id){ui.colored_label(ui.visuals().error_fg_color,e.to_string());}}
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.width,240.0..=1000.0).text("屏宽 dp"));
            ui.add(egui::Slider::new(&mut self.font_scale,0.8..=2.0).text("字体缩放"));
            ui.checkbox(&mut self.landscape,"编辑横屏布局");
        });
        if self.landscape && layout.landscape.is_none() {layout.landscape=Some(Box::new(layout.clone()));}
        let canvas=if self.landscape {layout.landscape.as_deref_mut().unwrap()} else {layout};
        ui.horizontal(|ui| {
            ui.label("画布宽 / 高");ui.add(egui::DragValue::new(&mut canvas.width).clamp_range(1.0..=100.0).speed(0.1));
            ui.add(egui::DragValue::new(&mut canvas.height).clamp_range(1.0..=100.0).speed(0.1));
            if ui.button("添加按键").clicked() {
                let mut id=format!("key{}",canvas.keys.len());while canvas.keys.iter().any(|k|k.id==id){id.push('x');}
                let y=canvas.height;canvas.height+=1.;
                canvas.keys.push(Key{id,label:"新键".into(),x:0.,y,width:1.,height:1.,role:"key".into(),action:KeyAction::Character('a'),long_press:None,
                    directions:Default::default(),disabled_directions:Vec::new(),extensions:Default::default()});self.selected=canvas.keys.len()-1;
            }
        });
        let width=self.width.min(ui.available_width());let unit=width/canvas.width.max(1.);
        let row=52.;let (rect,_)=ui.allocate_exact_size(egui::vec2(width,row*canvas.height.min(20.)),egui::Sense::hover());
        for (index,key) in canvas.keys.iter().enumerate() {
            let area=egui::Rect::from_min_size(rect.min+egui::vec2(key.x*unit,key.y*row),egui::vec2(key.width*unit,key.height*row)).shrink(2.);
            let response=ui.interact(area,ui.id().with((id,index)),egui::Sense::click());
            ui.painter().rect_filled(area,4.,if self.selected==index {ui.visuals().selection.bg_fill}else{ui.visuals().widgets.inactive.bg_fill});
            ui.painter().text(area.center(),egui::Align2::CENTER_CENTER,&key.label,egui::FontId::proportional(16.*self.font_scale),ui.visuals().text_color());
            if response.clicked() {self.selected=index;self.message=format!("模拟点击 {} → {:?}",key.id,key.action);}
        }
        if !canvas.keys.is_empty() {
            self.selected=self.selected.min(canvas.keys.len()-1);
            ui.horizontal(|ui| {
                egui::ComboBox::from_label("按键").selected_text(&canvas.keys[self.selected].id).show_ui(ui,|ui| {
                    for (i,key) in canvas.keys.iter().enumerate(){ui.selectable_value(&mut self.selected,i,&key.id);}
                });
                if ui.button("删除所选键").clicked(){canvas.keys.remove(self.selected);self.selected=self.selected.saturating_sub(1);}
                if self.selected>0 && self.selected<canvas.keys.len() && ui.button("与前键交换位置").clicked(){
                    let a=self.selected;let b=a-1;let (x,y)=(canvas.keys[a].x,canvas.keys[a].y);
                    canvas.keys[a].x=canvas.keys[b].x;canvas.keys[a].y=canvas.keys[b].y;canvas.keys[b].x=x;canvas.keys[b].y=y;
                    canvas.keys.swap(a,b);self.selected=b;
                }
            });
            if let Some(key)=canvas.keys.get_mut(self.selected) {
                ui.horizontal(|ui| {ui.label("ID");ui.text_edit_singleline(&mut key.id);ui.label("标签");ui.text_edit_singleline(&mut key.label);});
                ui.horizontal(|ui| {for (label,value) in [("x",&mut key.x),("y",&mut key.y),("宽",&mut key.width),("高",&mut key.height)] {
                    ui.label(label);ui.add(egui::DragValue::new(value).speed(0.1));
                }});
                ui.horizontal(|ui| {ui.label("样式角色");ui.text_edit_singleline(&mut key.role);});
                ui.push_id("primary",|ui|action_form(ui,&mut key.action));
            }
        }
        if let Some(mapping)=mapping {
            ui.collapsing("字母分组（与标签独立）",|ui| {
                let mut remove=None;
                for (id,letters) in &mut mapping.groups {ui.horizontal(|ui|{ui.label(id);ui.text_edit_singleline(letters);if ui.button("删除组").clicked(){remove=Some(id.clone());}});}
                if let Some(id)=remove {mapping.groups.remove(&id);}
                if ui.button("添加分组").clicked(){let mut id=format!("g{}",mapping.groups.len());while mapping.groups.contains_key(&id){id.push('x');}mapping.groups.insert(id,"a".into());}
            });
        }
        let (layout,mapping)=self.working.as_ref().unwrap();
        let validation=skin::combination::validate_layout(layout,mapping.as_ref());
        if let Err(e)=&validation {ui.colored_label(ui.visuals().error_fg_color,e.to_string());}
        if ui.add_enabled(validation.is_ok(),egui::Button::new("校验并写入组件草稿")).clicked() {
            self.message=match draft.update_keyboard(id,layout,mapping.as_ref()) {Ok(())=>"组件已更新，可导出新包".into(),Err(e)=>e.to_string()};
        }
        ui.label(&self.message);
    }
}

pub(crate) fn action_form(ui:&mut egui::Ui,action:&mut KeyAction) {
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_source("action").selected_text(format!("{action:?}")).show_ui(ui,|ui| {
            for candidate in [KeyAction::Character('a'),KeyAction::Group("g0".into()),KeyAction::Backspace,KeyAction::Space,KeyAction::Enter,KeyAction::Cancel,
                KeyAction::PageNext,KeyAction::PagePrevious,KeyAction::Letters,KeyAction::Numbers,KeyAction::Symbols,KeyAction::Shift,KeyAction::Pinyin,KeyAction::English] {
                let label=format!("{candidate:?}");if ui.selectable_label(std::mem::discriminant(action)==std::mem::discriminant(&candidate),label).clicked(){*action=candidate;}
            }
        });
        match action {KeyAction::Character(c)=>{let mut value=c.to_string();if ui.text_edit_singleline(&mut value).changed() && value.chars().count()==1 {*c=value.chars().next().unwrap();}},
            KeyAction::Group(id)=>{ui.label("组 ID");ui.text_edit_singleline(id);},_=>{}}
    });
}
