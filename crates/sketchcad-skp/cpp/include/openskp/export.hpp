// Replaces the header CMake's generate_export_header() would create for OpenSKP:
// the library is compiled statically into SketchCAD, so no symbol export decoration is needed.
#pragma once

#ifndef OPENSKP_STATIC_DEFINE
#define OPENSKP_STATIC_DEFINE
#endif
#define OPENSKP_EXPORT
#define OPENSKP_NO_EXPORT
#define OPENSKP_DEPRECATED [[deprecated]]
#define OPENSKP_DEPRECATED_EXPORT OPENSKP_EXPORT OPENSKP_DEPRECATED
#define OPENSKP_DEPRECATED_NO_EXPORT OPENSKP_NO_EXPORT OPENSKP_DEPRECATED
