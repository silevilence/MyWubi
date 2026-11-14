fn main()->Result<(),Box<dyn std::error::Error>> {
    let path=std::env::args().nth(1).ok_or("用法：keyboard_templates <新包路径>")?;
    core_engine::skin::keyboard_templates::package()?.export_new(std::path::Path::new(&path))?;Ok(())
}
