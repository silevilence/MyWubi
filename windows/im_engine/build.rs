fn main() {
    println!("cargo:rerun-if-changed=../../assets/branding/mywubi.ico");
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("../../assets/branding/mywubi.ico");
        if let Err(error) = resource.compile() {
            panic!("无法编译输入法图标资源: {error}");
        }
    }
}
