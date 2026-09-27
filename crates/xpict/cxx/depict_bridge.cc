#include "depict_bridge.h"
#include "xpict/src/ffi.rs.h"

#include <GraphMol/Bond.h>
#include <GraphMol/Conformer.h>
#include <GraphMol/Depictor/RDDepictor.h>
#include <GraphMol/FileParsers/FileParsers.h>
#include <GraphMol/FileParsers/MolFileStereochem.h>
#include <GraphMol/GraphMol.h>
#include <GraphMol/MolOps.h>
#include <GraphMol/Substruct/SubstructMatch.h>

#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

namespace xpict_depict {
namespace {

constexpr unsigned kMinMcsAtoms = 3;

std::unique_ptr<RDKit::RWMol> parse_molblock(const std::string &mb) {
  RDKit::RWMol *raw = nullptr;
  try {
    raw = RDKit::MolBlockToMol(mb, /*sanitize=*/true, /*removeHs=*/false);
  } catch (const std::exception &e) {
    throw std::runtime_error(std::string("MolBlockToMol failed: ") + e.what());
  }
  if (!raw) {
    throw std::runtime_error("MolBlockToMol returned null");
  }
  return std::unique_ptr<RDKit::RWMol>(raw);
}

void try_kekulize(RDKit::RWMol &mol) {
  try {
    RDKit::MolOps::Kekulize(mol, true);
  } catch (...) {
  }
}

void ensure_2d(RDKit::ROMol &mol) {
  if (mol.getNumConformers() == 0) {
    RDDepict::compute2DCoords(mol);
  }
}

rust::String bond_stereo(const RDKit::Bond &b) {
  using BD = RDKit::Bond::BondDir;
  using BS = RDKit::Bond::BondStereo;
  auto dir = b.getBondDir();
  if (dir == BD::BEGINWEDGE) {
    return "up";
  }
  if (dir == BD::BEGINDASH) {
    return "down";
  }
  if (dir == BD::UNKNOWN || b.getStereo() == BS::STEREOANY) {
    return "either";
  }
  return "";
}

LayoutOut extract(RDKit::ROMol &mol) {
  if (mol.getNumConformers() == 0) {
    RDDepict::compute2DCoords(mol);
  }
  try {
    RDKit::WedgeMolBonds(mol, &mol.getConformer());
  } catch (...) {
  }

  LayoutOut out;
  out.molblock = RDKit::MolToMolBlock(mol);
  out.matched_template = false;
  const auto &conf = mol.getConformer();
  const unsigned n = mol.getNumAtoms();
  out.atoms.reserve(n);
  for (unsigned i = 0; i < n; ++i) {
    const RDKit::Atom *a = mol.getAtomWithIdx(i);
    const auto &pos = conf.getAtomPos(i);
    LaidAtom la;
    la.index = static_cast<std::int32_t>(i);
    la.z = a->getAtomicNum();
    la.charge = a->getFormalCharge();
    la.total_hs = a->getTotalNumHs();
    la.isotope = a->getIsotope();
    la.x = pos.x;
    la.y = pos.y;
    la.symbol = a->getSymbol();
    out.atoms.push_back(std::move(la));
  }

  const unsigned nb = mol.getNumBonds();
  out.bonds.reserve(nb);
  for (unsigned i = 0; i < nb; ++i) {
    const RDKit::Bond *b = mol.getBondWithIdx(i);
    LaidBond lb;
    lb.index = static_cast<std::int32_t>(i);
    lb.begin = static_cast<std::int32_t>(b->getBeginAtomIdx());
    lb.end = static_cast<std::int32_t>(b->getEndAtomIdx());
    lb.order = b->getBondTypeAsDouble();
    lb.stereo = bond_stereo(*b);
    out.bonds.push_back(std::move(lb));
  }
  return out;
}


} // namespace

LayoutOut prepare_layout(rust::Str molblock, rust::Str /*template_molblock*/) {
  // Free 2D only. MCS is chematic in Rust (`mcs_atom_map`); align uses
  // prepare_layout_mapped with an explicit atom map.
  std::string mb(molblock);
  auto mol = parse_molblock(mb);
  try_kekulize(*mol);
  ensure_2d(*mol);
  LayoutOut out = extract(*mol);
  out.matched_template = false;
  return out;
}

LayoutOut prepare_layout_mapped(rust::Str molblock, rust::Str template_molblock,
                                rust::Slice<const std::int32_t> atom_map_qt) {
  std::string mb(molblock);
  std::string tmpl_mb(template_molblock);
  auto mol = parse_molblock(mb);
  try_kekulize(*mol);

  bool matched = false;
  if (!tmpl_mb.empty() && atom_map_qt.size() >= 2 &&
      atom_map_qt.size() % 2 == 0) {
    auto tmpl = parse_molblock(tmpl_mb);
    if (tmpl->getNumConformers() == 0) {
      RDDepict::compute2DCoords(*tmpl);
    }
    RDKit::MatchVectType atomMap;
    const auto nq = mol->getNumAtoms();
    const auto nt = tmpl->getNumAtoms();
    for (size_t i = 0; i + 1 < atom_map_qt.size(); i += 2) {
      const auto q = atom_map_qt[i];
      const auto t = atom_map_qt[i + 1];
      if (q < 0 || t < 0 || static_cast<unsigned>(q) >= nq ||
          static_cast<unsigned>(t) >= nt) {
        continue;
      }
      atomMap.emplace_back(t, q);
    }
    if (atomMap.size() >= kMinMcsAtoms) {
      RDDepict::ConstrainedDepictionParams p;
      p.allowRGroups = true;
      p.acceptFailure = false;
      try {
        RDDepict::generateDepictionMatching2DStructure(*mol, *tmpl, atomMap, -1,
                                                       p);
        matched = true;
      } catch (...) {
        ensure_2d(*mol);
        matched = false;
      }
    } else {
      ensure_2d(*mol);
    }
  } else {
    ensure_2d(*mol);
  }

  LayoutOut out = extract(*mol);
  out.matched_template = matched;
  return out;
}

} // namespace xpict_depict
