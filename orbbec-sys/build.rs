use std::env;
use std::path::{Path, PathBuf};

const SDK_ROOT_ENV: &str = "OB_SDK_ROOT";
const DEFAULT_SDK_ROOT: &str = "/usr/local";

fn sdk_root() -> PathBuf {
    PathBuf::from(env::var(SDK_ROOT_ENV).unwrap_or_else(|_| DEFAULT_SDK_ROOT.to_string()))
}

fn find_sdk(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let include_dir = root.join("include");
    let lib_dir = root.join("lib");
    let header = include_dir.join("libobsensor").join("ObSensor.h");
    if header.exists() && lib_dir.exists() {
        Some((include_dir, lib_dir))
    } else {
        None
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed={SDK_ROOT_ENV}");
    println!("cargo:rerun-if-changed=wrapper.h");

    let root = sdk_root();
    let Some((include_dir, lib_dir)) = find_sdk(&root) else {
        eprintln!(
            "error: OrbbecSDK v2 headers not found (expected {})",
            root.join("include/libobsensor/ObSensor.h").display()
        );
        eprintln!("error: The Orbbec SDK v2 is not installed on this system.");
        eprintln!("error: Please install it first, see: docs/install-sdk.md in this repo.");
        eprintln!(
            "error: You can point to an existing install via the {SDK_ROOT_ENV} env var."
        );
        std::process::exit(1);
    };

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=OrbbecSDK");
    println!("cargo:rustc-link-lib=stdc++");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", include_dir.display()))
        .allowlist_function("ob_.*")
        .allowlist_type("ob_.*")
        .allowlist_var("OB_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("bindgen: failed to generate OrbbecSDK bindings");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("bindgen: failed to write bindings");
}
