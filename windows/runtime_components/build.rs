fn main() {
    println!("cargo:rerun-if-env-changed=MYWUBI_COMPONENT_CATALOG");
    let data = match std::env::var_os("MYWUBI_COMPONENT_CATALOG") {
        Some(path) => {
            println!(
                "cargo:rerun-if-changed={}",
                std::path::Path::new(&path).display()
            );
            std::fs::read(path).expect("read release component catalog")
        }
        None => b"[]".to_vec(),
    };
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    std::fs::write(out.join("components.json"), data).expect("write compiled component catalog");
}
