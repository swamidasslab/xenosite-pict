//! Periodic table: atomic number ↔ element symbol.
//!
//! Index 0 is the wildcard / dummy atom (`*`). Indices 1..=118 are H..=Og.

/// Symbols by atomic number (`0` → `*`, `1` → `H`, … `118` → `Og`).
pub const SYMBOLS: [&str; 119] = [
    "*", "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si",
    "P", "S", "Cl", "Ar", "K", "Ca", "Sc", "Ti", "V", "Cr", "Mn", "Fe", "Co", "Ni", "Cu",
    "Zn", "Ga", "Ge", "As", "Se", "Br", "Kr", "Rb", "Sr", "Y", "Zr", "Nb", "Mo", "Tc",
    "Ru", "Rh", "Pd", "Ag", "Cd", "In", "Sn", "Sb", "Te", "I", "Xe", "Cs", "Ba", "La",
    "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd", "Tb", "Dy", "Ho", "Er", "Tm", "Yb", "Lu",
    "Hf", "Ta", "W", "Re", "Os", "Ir", "Pt", "Au", "Hg", "Tl", "Pb", "Bi", "Po", "At",
    "Rn", "Fr", "Ra", "Ac", "Th", "Pa", "U", "Np", "Pu", "Am", "Cm", "Bk", "Cf", "Es",
    "Fm", "Md", "No", "Lr", "Rf", "Db", "Sg", "Bh", "Hs", "Mt", "Ds", "Rg", "Cn", "Nh",
    "Fl", "Mc", "Lv", "Ts", "Og",
];

/// Element symbol for atomic number `z`. Unknown `z` → `"X"`.
pub fn element_symbol(z: u32) -> &'static str {
    SYMBOLS.get(z as usize).copied().unwrap_or("X")
}

/// Atomic number for an element symbol (case-insensitive). `"*"` / `"R"` → `0`.
pub fn atomic_number(symbol: &str) -> Option<u32> {
    let s = symbol.trim();
    if s.is_empty() {
        return None;
    }
    if s == "*" || s.eq_ignore_ascii_case("R") {
        return Some(0);
    }
    for (z, sym) in SYMBOLS.iter().enumerate().skip(1) {
        if sym.eq_ignore_ascii_case(s) {
            return Some(z as u32);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_0_to_118() {
        assert_eq!(SYMBOLS.len(), 119);
        assert_eq!(SYMBOLS[0], "*");
        assert_eq!(SYMBOLS[1], "H");
        assert_eq!(SYMBOLS[6], "C");
        assert_eq!(SYMBOLS[8], "O");
        assert_eq!(SYMBOLS[92], "U");
        assert_eq!(SYMBOLS[118], "Og");
    }

    #[test]
    fn symbol_roundtrip() {
        for z in 0u32..=118 {
            let sym = element_symbol(z);
            let back = atomic_number(sym).expect(sym);
            assert_eq!(back, z, "{sym}");
        }
        assert_eq!(element_symbol(999), "X");
        assert_eq!(atomic_number("fe"), Some(26));
        assert_eq!(atomic_number("R"), Some(0));
        assert_eq!(atomic_number("nope"), None);
        assert_eq!(atomic_number(""), None);
        assert_eq!(atomic_number("   "), None);
        assert_eq!(atomic_number("*"), Some(0));
    }
}
