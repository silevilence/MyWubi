//! Copy a developer/packager-selected ONNX runtime beside binaries and test binaries.
//! Users never need Python; official native runtime distribution is a release input.
fn main() {
    println!("cargo:rerun-if-env-changed=MYWUBI_ORT_DYLIB");
    if std::env::var_os("CARGO_FEATURE_ORT").is_none() {
        return;
    }
    let Some(source) = std::env::var_os("MYWUBI_ORT_DYLIB") else {
        return;
    };
    let source = std::path::PathBuf::from(source);
    if !source.is_absolute() || !source.is_file() {
        panic!("MYWUBI_ORT_DYLIB must name an existing absolute native runtime path");
    }
    println!("cargo:rerun-if-changed={}", source.display());
    // Refuse a host DLL accidentally passed to an Android cross build. Merely
    // renaming a PE image to .so would create an unusable deployment artifact.
    let bytes = std::fs::read(&source).expect("read native runtime header");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("target OS");
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect("target architecture");
    let compatible = if target_os == "windows" {
        let pe = bytes
            .get(0x3c..0x40)
            .map(|v| u32::from_le_bytes(v.try_into().unwrap()) as usize);
        pe.and_then(|offset| bytes.get(offset..offset.checked_add(6)?))
            .is_some_and(|header| {
                header[..4] == *b"PE\0\0"
                    && match target_arch.as_str() {
                        "x86_64" => header[4..6] == [0x64, 0x86],
                        "aarch64" => header[4..6] == [0x64, 0xaa],
                        _ => false,
                    }
            })
    } else {
        bytes.get(..20).is_some_and(|header| {
            header[..4] == *b"\x7fELF"
                && header[4] == 2
                && header[5] == 1
                && match target_arch.as_str() {
                    "x86_64" => header[18..20] == [62, 0],
                    "aarch64" => header[18..20] == [183, 0],
                    _ => false,
                }
        })
    };
    assert!(compatible, "MYWUBI_ORT_DYLIB has the wrong native format or architecture for {target_os}/{target_arch}");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    let name = if target_os == "windows" {
        "onnxruntime.dll"
    } else {
        "libonnxruntime.so"
    };
    for directory in [profile.to_path_buf(), profile.join("deps")] {
        std::fs::create_dir_all(&directory).expect("create native runtime destination");
        std::fs::copy(&source, directory.join(name)).expect("copy ONNX runtime");
    }
}
