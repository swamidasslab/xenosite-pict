# xenosmarts-syntax SMILES snapshot

Copied from sibling `xenosmarts` `crates/xenosmarts-syntax` at

`9f9c733c188265ad5f867de8fd23df432767cf20`

SMILES only: `gen_smiles.rs`, `smiles_rt.rs`, `cxsmiles.rs`, plus the `lit` /
`tok` / `num` helpers. Not copied: `gen_xsmarts`, SMARTS dialect, writer.

Refresh: copy those files again and keep the two `crate::` → `super::` import
rewrites in `gen_smiles.rs` / `smiles_rt.rs`.
