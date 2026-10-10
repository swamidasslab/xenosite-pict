#ifndef COORDGEN_INTEROP_H
#define COORDGEN_INTEROP_H

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Layout flags map to real sketcherMinimizer setters / precision ctor.
 * (Draft FFI names that do not exist upstream are not carried forward.)
 */
#define LAYOUT_FLAG_NONE 0
/* sketcherMinimizer::setEvenAngles(true) */
#define LAYOUT_FLAG_EVEN_ANGLES (1 << 0)
/* ctor precision = SKETCHER_QUICK_PRECISION */
#define LAYOUT_FLAG_QUICK_PRECISION (1 << 1)
/* sketcherMinimizer::setForceOpenMacrocycles(true) */
#define LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES (1 << 2)
/* ctor precision = SKETCHER_BEST_PRECISION */
#define LAYOUT_FLAG_BEST_PRECISION (1 << 3)
/* sketcherMinimizer::setTreatNonterminalBondsToMetalAsZOBs(true) */
#define LAYOUT_FLAG_METAL_BONDS_AS_ZOB (1 << 4)
/* sketcherMinimizer::setSkipMinimization(true) */
#define LAYOUT_FLAG_SKIP_MINIMIZATION (1 << 5)

/* Back-compat aliases used by earlier drafts */
#define LAYOUT_FLAG_FORCE_PLANAR LAYOUT_FLAG_EVEN_ANGLES
#define LAYOUT_FLAG_MACROCYCLE_MODE LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES

enum {
  COORDGEN_OK = 0,
  COORDGEN_ERR_NULL = 1,
  COORDGEN_ERR_EMPTY = 2,
  COORDGEN_ERR_BAD_INDEX = 3,
  COORDGEN_ERR_EXCEPTION = 4
};

typedef struct {
  int atom_count;
  const int* atomic_numbers;
  /* Optional; length atom_count. NULL → all zero. */
  const int* formal_charges;

  int bond_count;
  const int* bond_from;
  const int* bond_to;
  const int* bond_orders;
  /* 0=unspecified, 1=cis/Z, 2=trans/E. NULL → all unspecified. */
  const int* bond_stereos;
  /*
   * Optional stereo substituent atom indices (length bond_count).
   * Used with sketcherMinimizerBondStereoInfo + setAbsoluteStereoFromStereoInfo.
   * Pass NULL to fall back to setting isZ directly when bond_stereos is set.
   */
  const int* bond_stereo_atom1;
  const int* bond_stereo_atom2;

  /* Optional per-atom arrays (NULL if unused). */
  const int* has_template_match; /* soft constraint → atom->constrained */
  const int* is_atom_fixed;      /* hard fix → atom->fixed */
  const float* input_x;
  const float* input_y;

  int layout_flags;
} CoordgenInput;

typedef struct {
  float* out_x;
  float* out_y;
} CoordgenOutput;

/**
 * Single-pass layout for Rust FFI.
 * Returns COORDGEN_OK or an error code; never throws across the boundary.
 */
int coordgen_generate_2d_ffi(const CoordgenInput* input, CoordgenOutput* output);

#ifdef __cplusplus
}
#endif

#endif
