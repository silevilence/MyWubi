use core_engine::skin::{directions::{self,Direction,Gesture,ALL,Preset},keyboard::{Key,KeyAction}};
use std::collections::{BTreeMap,BTreeSet};
use eframe::egui;
pub struct GestureEditor {selected:BTreeSet<Direction>,action:KeyAction,defaults:bool,gesture:Option<Gesture>,origin:Option<egui::Pos2>,result:String}
impl Default for GestureEditor {
    fn default()->Self {Self{selected:[Direction::Up].into(),action:KeyAction::Character('a'),defaults:false,gesture:None,origin:None,result:String::new()}}
}
impl GestureEditor {
    pub fn ui(&mut self,ui:&mut egui::Ui,defaults:&mut BTreeMap<String,KeyAction>,key:&mut Key) {
        ui.collapsing("八向动作与连续滑动模拟",|ui| {
            if key.action==KeyAction::Backspace {ui.label("退格使用专用长按重复与上滑清空，不能叠加普通方向动作。");
                if ui.button("隔离所有布局默认方向").clicked(){key.directions.clear();key.disabled_directions=ALL.iter().map(|d|d.name().into()).collect();key.long_press=None;}return;}
            ui.checkbox(&mut self.defaults,"编辑布局默认方向（影响继承此方向的键）");
            egui::Grid::new("direction-wheel").show(ui,|ui| {
                for row in [[Some(7),Some(0),Some(1)],[Some(6),None,Some(2)],[Some(5),Some(4),Some(3)]] {
                    for index in row {if let Some(i)=index {let d=ALL[i];let mut value=self.selected.contains(&d);
                        if ui.toggle_value(&mut value,["上","右上","右","右下","下","左下","左","左上"][i]).changed() {if value {self.selected.insert(d);}else{self.selected.remove(&d);}}
                    }else{ui.label("多选合并");}}ui.end_row();
                }
            });
            ui.push_id("direction-action",|ui|crate::keyboard_editor::action_form(ui,&mut self.action));
            let target=if self.defaults {&mut *defaults}else{&mut key.directions};
            ui.horizontal_wrapped(|ui| {
                if ui.button("合并所选方向并绑定").clicked() {
                    self.result=match directions::replace(target,&self.selected,Some(self.action.clone())) {Ok(())=>"绑定已修改，尚未写入组件".into(),Err(e)=>e.to_string()};
                    if !self.defaults {key.disabled_directions.retain(|d|!self.selected.iter().any(|s|s.name()==d));}
                }
                if ui.button("清除覆盖 / 恢复继承").clicked(){let _=directions::replace(target,&self.selected,None);key.disabled_directions.retain(|d|!self.selected.iter().any(|s|s.name()==d));}
                if !self.defaults && ui.button("显式禁用所选方向").clicked(){let _=directions::replace(target,&self.selected,None);
                    for d in &self.selected {if !key.disabled_directions.iter().any(|n|n==d.name()){key.disabled_directions.push(d.name().into());}}
                }
            });
            ui.horizontal_wrapped(|ui| {for (label,preset) in [("精确八向",Preset::Exact),("宽上滑",Preset::WideUp),("宽下滑",Preset::WideDown),("左右合并",Preset::WideSides)] {
                if ui.button(label).clicked(){*target=directions::preset(preset,std::array::from_fn(|_|self.action.clone()));if !self.defaults {key.disabled_directions.clear();}}
            }});
            let mut hold=key.long_press.is_some();if ui.checkbox(&mut hold,"长按动作").changed(){key.long_press=hold.then_some(KeyAction::Character('a'));}
            if let Some(action)=&mut key.long_press {ui.push_id("hold-action",|ui|crate::keyboard_editor::action_form(ui,action));}
            match directions::resolve(defaults,&key.directions,&key.disabled_directions) {
                Err(e)=>{ui.colored_label(ui.visuals().error_fg_color,e.to_string());},
                Ok(bindings)=>{
                    for d in ALL {ui.label(match bindings.get(&d) {Some(b)=>format!("{} · {} · {} → {:?}",d.name(),if b.inherited {"继承"}else{"覆盖"},b.group,b.action),None=>format!("{} · 禁用 / 未绑定",d.name())});}
                    if hold && ui.button("模拟长按后上滑并松手").clicked(){let mut gesture=Gesture::new(bindings.keys().copied().collect(),true);gesture.hold();gesture.move_to(0.,-40.);self.result=format!("长按先完成：{:?} / {:?}",gesture.release(),key.long_press);}
                    ui.label("在下方按住并拖动，松手显示结果。24 dp 起滑、12 dp 回中心取消、5° 防抖；此处鼠标模拟不代替 Android 触摸验收。");
                    let (rect,response)=ui.allocate_exact_size(egui::vec2(340.,200.),egui::Sense::click_and_drag());
                    ui.painter().rect_filled(rect,4.,ui.visuals().extreme_bg_color);
                    if response.drag_started(){self.origin=ui.input(|i|i.pointer.press_origin());self.gesture=Some(Gesture::new(bindings.keys().copied().collect(),hold));}
                    if let (Some(gesture),Some(origin))=(&mut self.gesture,self.origin) {
                        if let Some(position)=ui.input(|i|i.pointer.interact_pos()){let delta=position-origin;gesture.move_to(delta.x,delta.y);
                            self.result=gesture.direction.and_then(|d|bindings.get(&d)).map(|b|format!("当前预览：{:?}",b.action)).unwrap_or_else(||"取消或未起滑".into());
                            ui.painter().line_segment([origin,position],egui::Stroke::new(2_f32,ui.visuals().text_color()));}
                        if response.drag_stopped(){let outcome=gesture.release();self.result=match outcome {Some(-1)=>format!("松手：{:?}",key.action),Some(-2)=>format!("长按：{:?}",key.long_press),Some(i)=>format!("松手：{:?}",bindings.get(&ALL[i as usize]).map(|b|&b.action)),None=>"已取消，无动作".into()};self.gesture=None;self.origin=None;}
                    }
                    if response.clicked(){self.result=format!("点击：{:?}",key.action);}
                }
            }
            ui.label(&self.result);
        });
    }
}
