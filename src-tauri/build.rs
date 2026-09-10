use std::{env, path::PathBuf, process::Command};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_foundation_model_bridge();
    }
    tauri_build::build()
}

fn build_foundation_model_bridge() {
    let source = PathBuf::from("native/FoundationModelBridge.swift");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR"))
        .join("FoundationModelBridge.o");
    let module_cache = output
        .parent()
        .expect("bridge output has a parent")
        .join("swift-module-cache");
    let architecture = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        other => panic!("Unsupported macOS architecture: {other:?}"),
    };
    let target = format!("{architecture}-apple-macosx14.0");
    let status = Command::new("xcrun")
        .args([
            "--sdk",
            "macosx",
            "swiftc",
            "-parse-as-library",
            "-emit-object",
            "-module-name",
            "BriefFoundationModel",
            "-module-cache-path",
        ])
        .arg(&module_cache)
        .args(["-target", &target])
        .arg(&source)
        .args(["-o"])
        .arg(&output)
        .status()
        .expect("Xcode is required to build the Apple Foundation Models bridge");
    assert!(
        status.success(),
        "Could not compile the Apple Foundation Models bridge"
    );

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rustc-link-arg={}", output.display());
    println!("cargo:rustc-link-arg=-Wl,-weak_framework,FoundationModels");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=dylib=swiftCore");
    println!("cargo:rustc-link-lib=dylib=swift_Concurrency");
}
