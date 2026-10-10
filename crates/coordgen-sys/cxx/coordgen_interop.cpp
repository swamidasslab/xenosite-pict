#include "coordgen_interop.h"

#include "sketcherMinimizer.h"

#include <exception>
#include <vector>

extern "C" int coordgen_generate_2d_ffi(const CoordgenInput* input,
                                        CoordgenOutput* output)
{
    if (input == nullptr || output == nullptr) {
        return COORDGEN_ERR_NULL;
    }
    if (input->atom_count <= 0) {
        return COORDGEN_ERR_EMPTY;
    }
    if (input->atomic_numbers == nullptr || output->out_x == nullptr ||
        output->out_y == nullptr) {
        return COORDGEN_ERR_NULL;
    }
    if (input->bond_count < 0) {
        return COORDGEN_ERR_BAD_INDEX;
    }
    if (input->bond_count > 0 &&
        (input->bond_from == nullptr || input->bond_to == nullptr ||
         input->bond_orders == nullptr)) {
        return COORDGEN_ERR_NULL;
    }

    try {
        // Precision: QUICK wins over BEST if both set; else STANDARD.
        float precision = SKETCHER_STANDARD_PRECISION;
        if (input->layout_flags & LAYOUT_FLAG_QUICK_PRECISION) {
            precision = SKETCHER_QUICK_PRECISION;
        } else if (input->layout_flags & LAYOUT_FLAG_BEST_PRECISION) {
            precision = SKETCHER_BEST_PRECISION;
        }

        sketcherMinimizer minimizer(precision);

        if (input->layout_flags & LAYOUT_FLAG_EVEN_ANGLES) {
            minimizer.setEvenAngles(true);
        }
        if (input->layout_flags & LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES) {
            minimizer.setForceOpenMacrocycles(true);
        }
        if (input->layout_flags & LAYOUT_FLAG_METAL_BONDS_AS_ZOB) {
            minimizer.setTreatNonterminalBondsToMetalAsZOBs(true);
        }
        if (input->layout_flags & LAYOUT_FLAG_SKIP_MINIMIZATION) {
            minimizer.setSkipMinimization(true);
        }

        auto* min_mol = new sketcherMinimizerMolecule();

        std::vector<sketcherMinimizerAtom*> atoms;
        atoms.reserve(static_cast<size_t>(input->atom_count));

        for (int i = 0; i < input->atom_count; ++i) {
            auto* atom = min_mol->addNewAtom();
            atom->setAtomicNumber(input->atomic_numbers[i]);
            if (input->formal_charges != nullptr) {
                atom->charge = input->formal_charges[i];
            }

            const bool templated = input->has_template_match != nullptr &&
                                   input->has_template_match[i] == 1;
            const bool fixed = input->is_atom_fixed != nullptr &&
                               input->is_atom_fixed[i] == 1;

            if ((templated || fixed) && input->input_x != nullptr &&
                input->input_y != nullptr) {
                const sketcherMinimizerPointF pt(input->input_x[i],
                                                 input->input_y[i]);
                atom->templateCoordinates = pt;
                atom->setCoordinates(pt);
                if (fixed) {
                    atom->fixed = true;
                } else {
                    atom->constrained = true;
                }
            }
            atoms.push_back(atom);
        }

        std::vector<sketcherMinimizerBond*> bonds;
        bonds.reserve(static_cast<size_t>(input->bond_count));

        for (int i = 0; i < input->bond_count; ++i) {
            const int a = input->bond_from[i];
            const int b = input->bond_to[i];
            if (a < 0 || b < 0 || a >= input->atom_count ||
                b >= input->atom_count) {
                delete min_mol;
                return COORDGEN_ERR_BAD_INDEX;
            }
            auto* bond = min_mol->addNewBond(atoms[static_cast<size_t>(a)],
                                             atoms[static_cast<size_t>(b)]);
            bond->setBondOrder(input->bond_orders[i]);
            bonds.push_back(bond);
        }

        // Neighbor graph required for CIP-aware stereo resolution.
        if (!bonds.empty()) {
            sketcherMinimizerMolecule::assignBondsAndNeighbors(atoms, bonds);
        }

        const bool have_stereo_atoms = input->bond_stereo_atom1 != nullptr &&
                                       input->bond_stereo_atom2 != nullptr;

        for (int i = 0; i < input->bond_count; ++i) {
            if (input->bond_stereos == nullptr) {
                continue;
            }
            const int stereo = input->bond_stereos[i];
            auto* bond = bonds[static_cast<size_t>(i)];

            if (stereo != 1 && stereo != 2) {
                // Unspecified: let CoordGen ignore Z/E for this bond.
                sketcherMinimizerBondStereoInfo info;
                info.stereo = sketcherMinimizerBondStereoInfo::unspecified;
                bond->setStereoChemistry(info);
                bond->setAbsoluteStereoFromStereoInfo();
                continue;
            }

            bool used_neighbors = false;
            if (have_stereo_atoms) {
                const int s1 = input->bond_stereo_atom1[i];
                const int s2 = input->bond_stereo_atom2[i];
                if (s1 >= 0 && s2 >= 0) {
                    if (s1 >= input->atom_count || s2 >= input->atom_count) {
                        delete min_mol;
                        return COORDGEN_ERR_BAD_INDEX;
                    }
                    sketcherMinimizerBondStereoInfo info;
                    info.atom1 = atoms[static_cast<size_t>(s1)];
                    info.atom2 = atoms[static_cast<size_t>(s2)];
                    info.stereo = (stereo == 1)
                                      ? sketcherMinimizerBondStereoInfo::cis
                                      : sketcherMinimizerBondStereoInfo::trans;
                    bond->setStereoChemistry(info);
                    bond->setAbsoluteStereoFromStereoInfo();
                    used_neighbors = true;
                }
            }
            if (!used_neighbors) {
                // Fallback without stereo substituents (less reliable).
                bond->isZEActive = true;
                bond->isZ = (stereo == 1);
            }
        }

        // minimizer owns min_mol after initialize().
        minimizer.initialize(min_mol);
        minimizer.runGenerateCoordinates();

        for (int i = 0; i < input->atom_count; ++i) {
            const auto& c = atoms[static_cast<size_t>(i)]->getCoordinates();
            output->out_x[i] = c.x();
            output->out_y[i] = c.y();
        }
        return COORDGEN_OK;
    } catch (const std::exception&) {
        return COORDGEN_ERR_EXCEPTION;
    } catch (...) {
        return COORDGEN_ERR_EXCEPTION;
    }
}
