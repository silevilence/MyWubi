//! # settings
//!
//! 独立配置程序骨架（egui/eframe）。当前阶段先落地 config.toml 的读写
//! CLI 验证，便于核心层验证。egui 界面将在后续路线图“基础 UI 框架与主题
//! 搭建”阶段接入。

use core_engine::Config;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("config.toml");

    match Config::load(path) {
        Ok(cfg) => {
            println!("✅ 已加载配置: {path}");
            println!("  候选词个数 : {}", cfg.basic.candidate_count);
            println!("  上屏方式   : {:?}", cfg.basic.commit_mode);
            println!("  切换键     : {:?}", cfg.basic.switch_key);
            println!("  系统码表   : {}", cfg.dictionary.system_table.display());
            println!("  字体大小   : {}", cfg.appearance.font_size);
        }
        Err(e) => {
            eprintln!("❌ 加载配置失败: {e}");
            eprintln!("    将写入默认配置到 {path}");
            let cfg = Config::default();
            if let Err(err) = cfg.save(path) {
                eprintln!("❌ 写入默认配置失败: {err}");
                std::process::exit(2);
            }
            println!("✅ 已生成默认配置: {path}");
        }
    }
}