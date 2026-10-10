use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor = manifest.join("../../vendor/coordgenlibs");
    let cxx = manifest.join("cxx");

    if !vendor.join("sketcherMinimizer.h").is_file() {
        panic!(
            "missing vendored CoordGen at {} — see vendor/coordgen-NOTICE.md",
            vendor.display()
        );
    }

    println!("cargo:rerun-if-changed=cxx/coordgen_interop.cpp");
    println!("cargo:rerun-if-changed=cxx/coordgen_interop.h");
    println!("cargo:rerun-if-changed={}", vendor.display());

    let sources = [
        "CoordgenFragmentBuilder.cpp",
        "CoordgenFragmenter.cpp",
        "CoordgenMacrocycleBuilder.cpp",
        "CoordgenMinimizer.cpp",
        "CoordgenTemplates.cpp",
        "sketcherMinimizer.cpp",
        "sketcherMinimizerAtom.cpp",
        "sketcherMinimizerBond.cpp",
        "sketcherMinimizerFragment.cpp",
        "sketcherMinimizerMarchingSquares.cpp",
        "sketcherMinimizerMolecule.cpp",
        "sketcherMinimizerResidue.cpp",
        "sketcherMinimizerResidueInteraction.cpp",
        "sketcherMinimizerRing.cpp",
    ];

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++11")
        .define("STATIC_COORDGEN", None)
        .include(&vendor)
        .include(&cxx)
        .warnings(false)
        .flag_if_supported("-ffunction-sections")
        .flag_if_supported("-fdata-sections")
        .file(cxx.join("coordgen_interop.cpp"));

    for name in sources {
        build.file(vendor.join(name));
    }

    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("emscripten") {
        build.flag("-fexceptions");
    }

    let profile = env::var("PROFILE").unwrap_or_default();
    if profile == "release" {
        build.opt_level_str("z");
        build.flag_if_supported("-fvisibility=hidden");
    }

    build.compile("coordgen_sys");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-arg=-Wl,-dead_strip");
    } else if target.contains("emscripten") {
        println!("cargo:rustc-link-arg=-Wl,--gc-sections");
    } else {
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-arg=-Wl,--gc-sections");
    }
}
