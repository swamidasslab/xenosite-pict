#pragma once

#include "rust/cxx.h"

namespace xpict_depict {

struct LayoutOut;

LayoutOut prepare_layout(rust::Str molblock, rust::Str template_molblock);

} // namespace xpict_depict
