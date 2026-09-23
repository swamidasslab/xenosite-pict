#pragma once

#include <cstdint>

#include "rust/cxx.h"

namespace xpict_depict {

struct LayoutOut;

LayoutOut prepare_layout(rust::Str molblock, rust::Str template_molblock);

/// ``atom_map_qt`` is flat ``[q0, t0, q1, t1, …]`` (query, template) pairs.
LayoutOut prepare_layout_mapped(rust::Str molblock, rust::Str template_molblock,
                                rust::Slice<const std::int32_t> atom_map_qt);

} // namespace xpict_depict
