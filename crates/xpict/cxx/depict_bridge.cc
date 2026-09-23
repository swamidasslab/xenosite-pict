#include "depict_bridge.h"
#include "xpict/src/ffi.rs.h"

#include <GraphMol/Bond.h>
#include <GraphMol/Conformer.h>
#include <GraphMol/Depictor/RDDepictor.h>
#include <GraphMol/FMCS/FMCS.h>
#include <GraphMol/FileParsers/FileParsers.h>
#include <GraphMol/FileParsers/MolFileStereochem.h>
#include <GraphMol/GraphMol.h>
#include <GraphMol/MolOps.h>
#include <GraphMol/SmilesParse/SmilesParse.h>

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

/** FMCS: element + hybridization atoms, any-bond, ring↔ring only
 *  (parity with Python ``_mcs_params``). */
bool mcs_atom_compare_elements_hybridization(
    const RDKit::MCSAtomCompareParameters &, const RDKit::ROMol &mol1,
    unsigned int idx1, const RDKit::ROMol &mol2, unsigned int idx2,
    void *) {
  const RDKit::Atom *a1 = mol1.getAtomWithIdx(idx1);
  const RDKit::Atom *a2 = mol2.getAtomWithIdx(idx2);
  if (a1->getAtomicNum() != a2->getAtomicNum()) {
    return false;
  }
  return a1->getHybridization() == a2->getHybridization();
}

std::unique_ptr<RDKit::ROMol> mcs_pattern(const RDKit::ROMol &mol,
                                          const RDKit::ROMol &tmpl) {
  std::vector<RDKit::ROMOL_SPTR> mols{
      RDKit::ROMOL_SPTR(new RDKit::ROMol(mol)),
      RDKit::ROMOL_SPTR(new RDKit::ROMol(tmpl)),
  };
  RDKit::MCSParameters params;
  params.Timeout = 2;
  params.AtomTyper = mcs_atom_compare_elements_hybridization;
  params.setMCSBondTyperFromEnum(RDKit::BondCompareAny);
  // Custom AtomTyper alone does not enforce ring↔ring; set both flags.
  params.AtomCompareParameters.RingMatchesRingOnly = true;
  params.BondCompareParameters.RingMatchesRingOnly = true;
  RDKit::MCSResult mcs = RDKit::findMCS(mols, &params);
  if (mcs.NumAtoms < kMinMcsAtoms || mcs.SmartsString.empty()) {
    return nullptr;
  }
  try {
    return std::unique_ptr<RDKit::ROMol>(RDKit::SmartsToMol(mcs.SmartsString));
  } catch (...) {
    return nullptr;
  }
}

/// Returns true when Depictor constrained the pose onto the template.
bool align_to_template(RDKit::ROMol &mol, const RDKit::ROMol &tmpl) {
  auto pattern = mcs_pattern(mol, tmpl);
  if (!pattern) {
    // No element+hybridization MCS — free layout (parity with Python/JS).
    ensure_2d(mol);
    return false;
  }
  RDDepict::ConstrainedDepictionParams p;
  p.allowRGroups = true;
  p.acceptFailure = false;
  try {
    auto match = RDDepict::generateDepictionMatching2DStructure(
        mol, tmpl, -1, pattern.get(), p);
    if (match.empty()) {
      ensure_2d(mol);
      return false;
    }
    return true;
  } catch (...) {
    ensure_2d(mol);
    return false;
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

LayoutOut prepare_layout(rust::Str molblock, rust::Str template_molblock) {
  std::string mb(molblock);
  std::string tmpl_mb(template_molblock);

  auto mol = parse_molblock(mb);
  try_kekulize(*mol);

  bool matched = false;
  if (!tmpl_mb.empty()) {
    auto tmpl = parse_molblock(tmpl_mb);
    if (tmpl->getNumConformers() == 0) {
      RDDepict::compute2DCoords(*tmpl);
    }
    matched = align_to_template(*mol, *tmpl);
  } else {
    ensure_2d(*mol);
  }

  LayoutOut out = extract(*mol);
  out.matched_template = matched;
  return out;
}

} // namespace xpict_depict
