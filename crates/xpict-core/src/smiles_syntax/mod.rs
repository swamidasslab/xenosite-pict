//! Vendored xenosmarts-syntax SMILES parser (as written, no perception).
//!
//! Snapshot of `crates/xenosmarts-syntax` from sibling `xenosmarts` — see
//! [`VENDOR.md`](VENDOR.md). SMARTS (`gen_xsmarts`) is not copied.

#[allow(dead_code)]
pub mod cxsmiles;
pub(crate) mod gen_smiles;
pub mod runtime;
#[allow(dead_code, unused_imports)]
pub mod smiles_rt;

use winnow::stream::Stateful;

/// Parse SMILES (optionally with a CXSMILES trailer) into a SmilesMol.
///
/// Empty string → empty molecule (RDKit `MolFromSmiles("")`).
pub fn parse_smiles(smiles: &str) -> Result<smiles_rt::SmilesMol, String> {
    if smiles.is_empty() {
        return Ok(smiles_rt::SmilesMol::default());
    }
    let mut builder = smiles_rt::SmilesBuilder::default();
    let mut input = Stateful {
        input: smiles,
        state: &mut builder,
    };
    let r = gen_smiles::r_start(&mut input);
    let rest = input.input;
    match r {
        Ok(()) if rest.is_empty() => builder.finish(),
        Ok(()) => Err(format!(
            "unparsed input at offset {}: {rest:?}",
            smiles.len() - rest.len()
        )),
        Err(e) => Err(format!(
            "parse error at offset {}: {e:?}",
            smiles.len() - rest.len()
        )),
    }
}
