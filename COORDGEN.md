## Goal

Integrate Schrödinger `coordgenlibs` as a **private implementation detail** of the Rust chemistry crate, usable from:

1. native Rust builds,
2. PyO3/maturin Python wheels,
3. browser WASM.

Do **not** expose CoordGen-specific types or APIs to Python or JS.

## Architecture

Use:

```text
public Rust chemistry API
        |
        v
private safe Rust layout wrapper
        |
        v
coordgen-sys
        |
        v
tiny extern "C" C++ shim
        |
        v
vendored coordgenlibs C++
```

Create an internal `coordgen-sys` crate if convenient.

Do not depend on the existing crates.io `coordgen` wrapper; vendor/use upstream `coordgenlibs` directly.

## C++ interface

Expose only a small flat C ABI. Avoid exposing C++ classes to Rust.

For example:

```cpp
extern "C" int coordgen_layout(
    const uint8_t* atomic_numbers,
    const int8_t* formal_charges,
    const uint32_t* bond_atoms,   // packed [a0,b0,a1,b1,...]
    const uint8_t* bond_orders,
    size_t n_atoms,
    size_t n_bonds,
    float* xy_out                 // packed [x0,y0,x1,y1,...]
);
```

Add whatever additional atom/bond flags CoordGen actually requires, but keep the boundary POD/flat-array based.

Catch all C++ exceptions inside the shim and return an error code; never allow a C++ exception to cross the FFI boundary.

## Rust API

Wrap the unsafe FFI immediately in safe Rust, e.g.:

```rust
pub(crate) fn generate_2d_coords(
    mol: &Molecule
) -> Result<Vec<[f32; 2]>, CoordGenError>
```

Convert the existing Rust molecule representation internally.

The public Rust/Python/JS APIs should use project terminology such as `generate_2d_coordinates()`, not `coordgen_*`.

## Native builds

Use `build.rs` plus either:

- `cc` for direct compilation, if CoordGen's source set is manageable, or
- `cmake` if using upstream's CMake build is substantially easier.

Prefer static linking.

Support at least:

```text
Linux x86_64/aarch64
macOS x86_64/aarch64
Windows x86_64
```

Do not hard-code `stdc++` vs `c++` unnecessarily; let the buil tooling select the appropriate C++ runtime.

## Python / maturin

Keep the existing PyO3/maturin interface.

`maturin build` should invoke Cargo, whose `build.rs` compiles and statically links CoordGen into the Python extension.

Desired result:

```text
Python
  -> PyO3 API
  -> Rust chemistry code
  -> private CoordGen backend
```

No Python dependency on CoordGen, Boost, CMake, or a separately installed shared library at runtime.

Verify resulting wheels with the normal platform tooling (`auditwheel`, `delocate`, etc.) and ensure there is no unintended `libcoordgen.so` runtime dependency.

## WASM

For the CoordGen-enabled browser build, use:

```text
wasm32-unknown-emscripten
```

rather than `wasm32-unknown-unknown`.

Compile both Rust and CoordGen C++ under the same Emscripten toolchain so they link into **one WASM module and share linear memory**. Rust explicitly recommends this target for C/C++ interoperability.

Pin the Emscripten version in CI. ABI compatibility can depend on the Emscripten version and linker options, so keep Rust std, C++, and the final linker configuration aligned.

Because the JS API is thin, prefer simple exported C-style functions rather than making `wasm-bindgen` a requirement.

Target architecture:

```text
RDKit.js WASM
      |
      | small JS copy/shuttle
      v
Rust + CoordGen WASM
```

Do not create a third standalone CoordGen WASM module.

## Build-system requirements

The same source-level `coordgen-sys` integration should support native and Emscripten targets.

Branch on Cargo target information where necessary:

```rust
CARGO_CFG_TARGET_OS
CARGO_CFG_TARGET_ARCH
TARGET
HOST
```

For Emscripten, ensure the C++ compiler is `em++`/the Emscripten CMake toolchain.

For native targets, use the host/cross C++ compiler selected by Cargo/build tooling.

Note that `cc` supports C++ compilation and treats the C++ stdlib specially for wasm targets, including static linkage.

## Dependency policy

Vendor a known CoordGen revision or pin it reproducibly.

Record:

- upstream commit/tag,
- CoordGen license,
- any bundled third-party license requirements.

Investigate whether Boost is actually needed by the library build or only examples/tests; avoid shipping unnecessary build dependencies. Upstream currently still has Boost-related CMake handling that can interact with newer CMake versions.

Disable upstream examples/tests/tools unless required.

## Cargo features

Prefer something along these lines:

```toml
[features]
default = ["coordgen"]
coordgen = ["dep:coordgen-sys"]
python = ["dep:pyo3"]
```

Do not make Python or WASM feature names control core chemistry behavior unless necessary.

The CoordGen backend should remain optional if downstream Rust users should be able to build without a C++ toolchain.

## Tests

Add one common set of molecule/layout tests that runs through the same Rust API on:

- native Rust,
- Python wheel,
- Emscripten/WASM.

At minimum test:

- simple acyclic molecule,
- aromatic ring,
- fused ring,
- macrocycle,
- charged molecule,
- molecule with stereochemistry if CoordGen uses it,
- failure handling.

Validate:

- coordinate count matches atom count,
- coordinates are finite,
- deterministic output where CoordGen is deterministic,
- atom ordering is unchanged,
- no panics/exceptions cross FFI.

## Key design rule

There should be exactly **one chemistry-facing Rust abstraction for 2D layout**.

Python, JS, and downstream Rust code should neither know nor care that CoordGen is C++.
