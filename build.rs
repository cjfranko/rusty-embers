use std::env;
use std::path::PathBuf;

fn main() {
    let ember_dir = PathBuf::from("libember_plus/libember_slim/Source");

    // Compile libember_slim as a static library.
    let mut build = cc::Build::new();
    build.include(&ember_dir);
    build.file(ember_dir.join("ber.c"));
    build.file(ember_dir.join("berio.c"));
    build.file(ember_dir.join("berreader.c"));
    build.file(ember_dir.join("bertag.c"));
    build.file(ember_dir.join("bytebuffer.c"));
    build.file(ember_dir.join("ember.c"));
    build.file(ember_dir.join("emberasyncreader.c"));
    build.file(ember_dir.join("emberframing.c"));
    build.file(ember_dir.join("emberinternal.c"));
    build.file(ember_dir.join("glow.c"));
    build.file(ember_dir.join("glowrx.c"));
    build.file(ember_dir.join("glowtx.c"));
    build.compile("ember_slim");

    // Generate bindings.
    let bindings = bindgen::Builder::default()
        .header(ember_dir.join("emberplus.h").to_string_lossy())
        .clang_arg(format!("-I{}", ember_dir.display()))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=libember_plus/libember_slim/Source");
}
