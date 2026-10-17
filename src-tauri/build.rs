use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=icons/icon.icns");
    println!("cargo:rerun-if-changed=icons/128x128@2x.png");
    println!("cargo:rerun-if-changed=Info.plist");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_swift_bridge("FoundationModelBridge", "BriefFoundationModel");
        build_swift_bridge("ContactsBridge", "BriefContacts");
        build_swift_bridge("CalendarBridge", "BriefCalendar");
        build_swift_bridge("DocumentBridge", "BriefDocuments");
        build_swift_bridge("EmbeddingBridge", "BriefEmbeddings");
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,FoundationModels");
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=Contacts");
        println!("cargo:rustc-link-lib=framework=EventKit");
        println!("cargo:rustc-link-lib=framework=PDFKit");
        println!("cargo:rustc-link-lib=framework=Vision");
        println!("cargo:rustc-link-lib=framework=NaturalLanguage");
        println!("cargo:rustc-link-lib=dylib=swiftCore");
        println!("cargo:rustc-link-lib=dylib=swift_Concurrency");
    }
    tauri_build::build()
}

/// Compiles `native/{name}.swift` to an object linked into the app.
fn build_swift_bridge(name: &str, module: &str) {
    let source = PathBuf::from(format!("native/{name}.swift"));
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR"))
        .join(format!("{name}.o"));
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
            module,
            "-module-cache-path",
        ])
        .arg(&module_cache)
        .args(["-target", &target])
        .arg(&source)
        .args(["-o"])
        .arg(&output)
        .status()
        .unwrap_or_else(|_| panic!("Xcode is required to build {name}"));
    assert!(status.success(), "Could not compile {name}");

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rustc-link-arg={}", output.display());
}
