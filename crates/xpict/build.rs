use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let compat = manifest.join("compat/rdkit");
    let cxx_dir = manifest.join("cxx");

    println!("cargo:rerun-if-changed=compat/rdkit/GraphMol/FileParsers/FileWriters.h");
    println!("cargo:rerun-if-changed=cxx/depict_bridge.h");
    println!("cargo:rerun-if-changed=cxx/depict_bridge.cc");
    println!("cargo:rerun-if-changed=src/ffi.rs");

    // Ubuntu librdkit-dev 202309: FileWriters.h → FileParsers.h shim for rdkit-sys.
    let mut include = std::env::var("CPLUS_INCLUDE_PATH").unwrap_or_default();
    if !include.is_empty() {
        include.push(':');
    }
    include.push_str(compat.to_str().unwrap());
    println!("cargo:rustc-env=CPLUS_INCLUDE_PATH={include}");

    let mut build = cxx_build::bridge("src/ffi.rs");
    build
        .file("cxx/depict_bridge.cc")
        .include(&cxx_dir)
        .include(&compat)
        .include("/usr/include/rdkit")
        .include("/usr/local/include/rdkit")
        .include("/usr/include")
        .include("/usr/local/include")
        .flag_if_supported("-std=c++17")
        .warnings(false);
    build.compile("xpict_depict_bridge");

    println!("cargo:rustc-link-search=native=/usr/lib");
    println!("cargo:rustc-link-search=native=/usr/local/lib");
    if let Ok(entries) = std::fs::read_dir("/usr/lib/gcc/x86_64-linux-gnu") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join("libstdc++.so").exists() || path.join("libstdc++.a").exists() {
                println!("cargo:rustc-link-search=native={}", path.display());
            }
        }
    }

    for lib in [
        "RDKitDepictor",
        "RDKitFileParsers",
        "RDKitGraphMol",
        "RDKitSmilesParse",
        "RDKitSubstructMatch",
        "RDKitMolAlign",
        "RDKitRDGeometryLib",
        "RDKitRDGeneral",
        "RDKitDataStructs",
    ] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
    println!("cargo:rustc-link-lib=dylib=boost_serialization");
    println!("cargo:rustc-link-lib=dylib=stdc++");
}
