use std::env;
use std::path::{Path, PathBuf};

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
    println!("cargo:rerun-if-env-changed=EMSDK");

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
    let is_emscripten = target.contains("emscripten");
    let is_wasm = target.starts_with("wasm32");

    if is_wasm {
        if !is_emscripten {
            panic!(
                "CoordGen wasm is wasm32-unknown-emscripten only (em++). \
                 Install emsdk and source emsdk_env.sh, or set EMSDK. See scripts/ensure_emsdk.sh."
            );
        }
        let emxx = find_emxx().unwrap_or_else(|| {
            panic!(
                "CoordGen wasm needs em++ (emscripten). Install emsdk and source emsdk_env.sh, \
                 or set EMSDK. See scripts/ensure_emsdk.sh."
            )
        });
        println!("cargo:warning=coordgen-sys wasm C++ via {}", emxx.display());
        build.compiler(&emxx);
        // Match rustc's `-fwasm-exceptions` (not sjlj / `-fexceptions`).
        build.flag("-fwasm-exceptions");
        // rustc cdylib on emscripten is a SIDE_MODULE; objects need PIC.
        build.flag("-fPIC");
        build.cpp_link_stdlib(None);
    } else if is_emscripten {
        build.flag("-fexceptions");
    }

    let profile = env::var("PROFILE").unwrap_or_default();
    if profile == "release" {
        build.opt_level_str("z");
        build.flag_if_supported("-fvisibility=hidden");
    }

    build.compile("coordgen_sys");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if is_wasm {
        // em++ objects carry libc++; rustc/wasm-ld must not pull host libstdc++.
    } else if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-arg=-Wl,-dead_strip");
    } else if is_emscripten {
        println!("cargo:rustc-link-arg=-Wl,--gc-sections");
    } else {
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-arg=-Wl,--gc-sections");
    }
}

fn find_emxx() -> Option<PathBuf> {
    if let Ok(explicit) = env::var("EMXX").or_else(|_| env::var("EMSCRIPTEN_CXX")) {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(sdk) = env::var("EMSDK") {
        let p = PathBuf::from(sdk).join("upstream/emscripten/em++");
        if p.is_file() {
            return Some(p);
        }
    }
    let cache_root = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")));
    if let Some(cache) = cache_root {
        let p = cache.join("emsdk/upstream/emscripten/em++");
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(':') {
            let p = Path::new(dir).join("em++");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}
