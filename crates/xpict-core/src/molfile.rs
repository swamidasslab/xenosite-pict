//! Tiny MDL CTAB reader (V2000 / V3000).
//!
//! Atoms (Z, charge), bonds (order; type 4 = aromatic), `M  CHG` / `M  RAD`.
//! Coordinates are ignored — CoordGen lays the graph out. Kitchen-sink
//! formats (MOL2, MolJSON, SDF extras) are out of scope.

use crate::elements::atomic_number;
use crate::structure::{AtomRec, BondKind, BondRec, MolGraph};

pub fn parse_molfile(text: &str) -> Result<MolGraph, String> {
    if text.contains("V3000") {
        parse_v3000(text).map_err(|e| format!("molfile V3000: {e}"))
    } else {
        parse_v2000(text).map_err(|e| format!("molfile: {e}"))
    }
}

fn parse_v2000(text: &str) -> Result<MolGraph, String> {
    let lines: Vec<&str> = text.lines().collect();
    let counts_i = lines
        .iter()
        .position(|l| l.contains("V2000") || (l.len() >= 6 && i3(l, 0) > 0 && i3(l, 3) >= 0))
        .ok_or("missing counts line")?;
    let counts = lines[counts_i];
    let na = i3(counts, 0) as usize;
    let nb = i3(counts, 3) as usize;
    if na == 0 {
        return Err("empty molecule".into());
    }
    let atom0 = counts_i + 1;
    if lines.len() < atom0 + na + nb {
        return Err("truncated atom/bond block".into());
    }
    let mut atoms = Vec::with_capacity(na);
    for line in &lines[atom0..atom0 + na] {
        atoms.push(parse_v2000_atom(line)?);
    }
    let mut bonds = Vec::with_capacity(nb);
    for line in &lines[atom0 + na..atom0 + na + nb] {
        bonds.push(parse_v2000_bond(line, na)?);
    }
    for b in &bonds {
        if b.kind == BondKind::Aromatic {
            atoms[b.a].aromatic = true;
            atoms[b.b].aromatic = true;
        }
    }
    apply_properties(&lines[atom0 + na + nb..], &mut atoms)?;
    Ok(MolGraph { atoms, bonds })
}

fn parse_v2000_atom(line: &str) -> Result<AtomRec, String> {
    if line.len() < 34 {
        return Err("short atom line".into());
    }
    let sym = line.get(31..34).unwrap_or("").trim();
    let z = if sym.is_empty() || sym == "*" || sym == "R" || sym == "A" {
        0
    } else {
        atomic_number(sym).ok_or_else(|| format!("unknown element {sym}"))? as i32
    };
    let mut charge = 0i32;
    let mut radical = 0u8;
    if let Some(code) = line.get(36..39).and_then(|s| s.trim().parse::<i32>().ok()) {
        match code {
            1 => charge = 3,
            2 => charge = 2,
            3 => charge = 1,
            4 => radical = 1,
            5 => charge = -1,
            6 => charge = -2,
            7 => charge = -3,
            _ => {}
        }
    }
    Ok(AtomRec {
        z,
        charge,
        aromatic: false,
        hcount: None,
        radical,
        mapno: None,
    })
}

fn parse_v2000_bond(line: &str, na: usize) -> Result<BondRec, String> {
    if line.len() < 9 {
        return Err("short bond line".into());
    }
    let a = (i3(line, 0) as usize).saturating_sub(1);
    let b = (i3(line, 3) as usize).saturating_sub(1);
    if a >= na || b >= na {
        return Err("bond atom out of range".into());
    }
    let order = i3(line, 6);
    Ok(BondRec {
        a,
        b,
        kind: ctab_kind(order),
    })
}

fn apply_properties(lines: &[&str], atoms: &mut [AtomRec]) -> Result<(), String> {
    for line in lines {
        let t = line.trim_end();
        if t.starts_with("M  END") || t == "$$$$" {
            break;
        }
        if let Some(rest) = t.strip_prefix("M  CHG") {
            for (idx, val) in pairs(rest)? {
                if idx < atoms.len() {
                    atoms[idx].charge = val;
                }
            }
        } else if let Some(rest) = t.strip_prefix("M  RAD") {
            for (idx, val) in pairs(rest)? {
                if idx < atoms.len() {
                    atoms[idx].radical = match val {
                        1 => 2, // singlet
                        2 => 1, // doublet
                        3 => 2, // triplet
                        n if n > 0 => n as u8,
                        _ => 0,
                    };
                }
            }
        }
    }
    Ok(())
}

/// `M  CHG` / `M  RAD`: count then (atom, value) pairs, 1-based atoms.
fn pairs(rest: &str) -> Result<Vec<(usize, i32)>, String> {
    let nums: Vec<i32> = rest
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    if nums.is_empty() {
        return Ok(Vec::new());
    }
    let count = nums[0] as usize;
    let mut out = Vec::new();
    let mut i = 1;
    while i + 1 < nums.len() && out.len() < count {
        let atom = (nums[i] as usize).saturating_sub(1);
        out.push((atom, nums[i + 1]));
        i += 2;
    }
    Ok(out)
}

fn parse_v3000(text: &str) -> Result<MolGraph, String> {
    let joined = join_v30(text);
    let mut na = 0usize;
    let mut nb = 0usize;
    for line in &joined {
        if let Some(rest) = line.strip_prefix("COUNTS ") {
            let mut it = rest.split_whitespace();
            na = it.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            nb = it.next().and_then(|t| t.parse().ok()).unwrap_or(0);
        }
    }
    if na == 0 {
        return Err("empty molecule".into());
    }
    let mut atoms = vec![
        AtomRec {
            z: 0,
            charge: 0,
            aromatic: false,
            hcount: None,
            radical: 0,
            mapno: None,
        };
        na
    ];
    let mut seen_atom = vec![false; na];
    let mut bonds = Vec::with_capacity(nb);
    let mut in_atom = false;
    let mut in_bond = false;
    for line in &joined {
        match line.as_str() {
            "BEGIN ATOM" => {
                in_atom = true;
                in_bond = false;
                continue;
            }
            "END ATOM" => {
                in_atom = false;
                continue;
            }
            "BEGIN BOND" => {
                in_bond = true;
                in_atom = false;
                continue;
            }
            "END BOND" => {
                in_bond = false;
                continue;
            }
            _ => {}
        }
        if in_atom {
            parse_v3000_atom(line, &mut atoms, &mut seen_atom)?;
        } else if in_bond {
            bonds.push(parse_v3000_bond(line, na)?);
        }
    }
    if !seen_atom.iter().all(|&s| s) {
        return Err("missing V3000 atoms".into());
    }
    for b in &bonds {
        if b.kind == BondKind::Aromatic {
            atoms[b.a].aromatic = true;
            atoms[b.b].aromatic = true;
        }
    }
    Ok(MolGraph { atoms, bonds })
}

fn join_v30(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for raw in text.lines() {
        let Some(rest) = raw.trim_start().strip_prefix("M  V30 ") else {
            continue;
        };
        if rest.ends_with('-') {
            buf.push_str(&rest[..rest.len() - 1]);
            continue;
        }
        if !buf.is_empty() {
            buf.push_str(rest);
            out.push(std::mem::take(&mut buf));
        } else {
            out.push(rest.to_string());
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

fn parse_v3000_atom(line: &str, atoms: &mut [AtomRec], seen: &mut [bool]) -> Result<(), String> {
    let mut parts = line.split_whitespace();
    let idx: usize = parts
        .next()
        .and_then(|t| t.parse().ok())
        .ok_or("bad V3000 atom index")?;
    let i = idx.checked_sub(1).ok_or("V3000 atom index 0")?;
    if i >= atoms.len() {
        return Err("V3000 atom out of range".into());
    }
    let sym = parts.next().unwrap_or("*");
    let z = if sym == "*" || sym == "R" || sym == "A" {
        0
    } else {
        atomic_number(sym).ok_or_else(|| format!("unknown element {sym}"))? as i32
    };
    let _x = parts.next();
    let _y = parts.next();
    let _z = parts.next();
    let map = parts.next().and_then(|t| t.parse::<u32>().ok());
    let mut charge = 0i32;
    let mut radical = 0u8;
    for tok in parts {
        if let Some(v) = tok.strip_prefix("CHG=") {
            charge = v.parse().unwrap_or(0);
        } else if let Some(v) = tok.strip_prefix("RAD=") {
            radical = match v.parse().unwrap_or(0) {
                1 => 2,
                2 => 1,
                3 => 2,
                n if n > 0 => n as u8,
                _ => 0,
            };
        }
    }
    atoms[i] = AtomRec {
        z,
        charge,
        aromatic: false,
        hcount: None,
        radical,
        mapno: map.filter(|&m| m > 0),
    };
    seen[i] = true;
    Ok(())
}

fn parse_v3000_bond(line: &str, na: usize) -> Result<BondRec, String> {
    let mut parts = line.split_whitespace();
    let _idx = parts.next();
    let order: i32 = parts
        .next()
        .and_then(|t| t.parse().ok())
        .ok_or("bad V3000 bond type")?;
    let a = parts
        .next()
        .and_then(|t| t.parse::<usize>().ok())
        .ok_or("bad V3000 bond atom")?
        .saturating_sub(1);
    let b = parts
        .next()
        .and_then(|t| t.parse::<usize>().ok())
        .ok_or("bad V3000 bond atom")?
        .saturating_sub(1);
    if a >= na || b >= na {
        return Err("V3000 bond atom out of range".into());
    }
    Ok(BondRec {
        a,
        b,
        kind: ctab_kind(order),
    })
}

fn ctab_kind(order: i32) -> BondKind {
    match order {
        2 => BondKind::Double,
        3 => BondKind::Triple,
        4 => BondKind::Aromatic,
        _ => BondKind::Single,
    }
}

fn i3(line: &str, start: usize) -> i32 {
    line.get(start..start + 3)
        .or_else(|| line.get(start..))
        .and_then(|s| {
            let t = s.get(..3).unwrap_or(s).chars().take(3).collect::<String>();
            t.trim().parse().ok()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ETHANOL_V2000: &str = "\
ethanol
  xpict

  3  2  0  0  0  0  0  0  0  0999 V2000
    0.0000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    1.5000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    2.2500    1.2990    0.0000 O   0  0  0  0  0  0  0  0  0  0  0  0
  1  2  1  0  0  0  0
  2  3  1  0  0  0  0
M  END
";

    const ETHANOL_V3000: &str = "\
ethanol
  xpict

  0  0  0     0  0            999 V3000
M  V30 BEGIN CTAB
M  V30 COUNTS 3 2 0 0 0
M  V30 BEGIN ATOM
M  V30 1 C 0 0 0 0
M  V30 2 C 1.5 0 0 0
M  V30 3 O 2.25 1.3 0 0
M  V30 END ATOM
M  V30 BEGIN BOND
M  V30 1 1 1 2
M  V30 2 1 2 3
M  V30 END BOND
M  V30 END CTAB
M  END
";

    #[test]
    fn ethanol_v2000() {
        let g = parse_molfile(ETHANOL_V2000).unwrap();
        assert_eq!(
            g.atoms.iter().map(|a| a.z).collect::<Vec<_>>(),
            vec![6, 6, 8]
        );
        assert_eq!(g.bonds.len(), 2);
    }

    #[test]
    fn ethanol_v3000() {
        let g = parse_molfile(ETHANOL_V3000).unwrap();
        assert_eq!(
            g.atoms.iter().map(|a| a.z).collect::<Vec<_>>(),
            vec![6, 6, 8]
        );
        assert_eq!(g.bonds.len(), 2);
    }

    #[test]
    fn benzene_type4_marks_aromatic() {
        let src = "\
benzene

  6  6  0  0  0  0  0  0  0  0999 V2000
    0.0000    1.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.8660    0.5000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.8660   -0.5000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.0000   -1.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -0.8660   -0.5000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -0.8660    0.5000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
  1  2  4  0  0  0  0
  2  3  4  0  0  0  0
  3  4  4  0  0  0  0
  4  5  4  0  0  0  0
  5  6  4  0  0  0  0
  6  1  4  0  0  0  0
M  END
";
        let g = parse_molfile(src).unwrap();
        assert!(g.atoms.iter().all(|a| a.aromatic));
        assert!(g.bonds.iter().all(|b| b.kind == BondKind::Aromatic));
    }
}
