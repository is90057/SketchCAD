// C ABI over the vendored OpenSKP C++ library, consumed from Rust (see src/ffi.rs).
// Every entry point catches C++ exceptions: they must never unwind into Rust.

#include <openskp/create.hpp>
#include <openskp/parser.hpp>
#include <openskp/scene.hpp>

#include <cmath>
#include <cstdint>
#include <cstring>
#include <exception>
#include <map>
#include <memory>
#include <string>
#include <vector>

namespace {

void write_error(char* buf, std::size_t len, const char* message) {
  if (buf == nullptr || len == 0) return;
  std::strncpy(buf, message, len - 1);
  buf[len - 1] = '\0';
}

}  // namespace

extern "C" {

// ---------------------------------------------------------------- reading

struct SkpScene {
  openskp::Scene scene;
  // Tag (layer) display colours: OpenSKP paints faces without a material in their tag colour
  std::map<std::string, openskp::Color3> layer_colors;
};

struct SkpPrimView {
  const float* positions;  // x, y, z triplets
  std::size_t position_count;
  const std::uint32_t* indices;
  std::size_t index_count;
  const char* geom_name;
  const char* name;
  const char* definition_name;
  const char* layer;
  // Entity path in the scene hierarchy ("ROOT / House / Door#1"); unique per placed instance
  const char* path;
  float color[4];
  std::uint8_t layer_hidden;
  // 1 when `color` equals the tag's own colour, i.e. no explicit material on these faces
  std::uint8_t uses_layer_color;
};

SkpScene* skp_scene_load(const char* path_utf8, char* err, std::size_t err_len) {
  try {
    auto file = openskp::SkpFile::open(std::filesystem::u8path(path_utf8));
    auto result = std::make_unique<SkpScene>();
    result->scene = file.build_scene();
    for (const auto& layer : file.parse().layers) result->layer_colors[layer.name] = layer.color;
    return result.release();
  } catch (const std::exception& e) {
    write_error(err, err_len, e.what());
  } catch (...) {
    write_error(err, err_len, "unknown OpenSKP error");
  }
  return nullptr;
}

void skp_scene_free(SkpScene* scene) { delete scene; }

std::size_t skp_scene_prim_count(const SkpScene* scene) {
  return scene ? scene->scene.glb_primitives.size() : 0;
}

int skp_scene_prim(const SkpScene* s, std::size_t index, SkpPrimView* out) {
  if (s == nullptr || out == nullptr || index >= s->scene.glb_primitives.size()) return 0;
  static const std::string kEmpty;
  const auto& prim = s->scene.glb_primitives[index];
  out->positions = prim.positions.data();
  out->position_count = prim.positions.size() / 3;
  out->indices = prim.indices.data();
  out->index_count = prim.indices.size();
  out->geom_name = prim.geom_name.c_str();

  const auto meta = s->scene.mesh_index.find(prim.geom_name);
  const bool has_meta = meta != s->scene.mesh_index.end();
  out->name = has_meta ? meta->second.name.c_str() : kEmpty.c_str();
  out->definition_name = has_meta ? meta->second.definition_name.c_str() : kEmpty.c_str();
  out->layer = has_meta ? meta->second.layer.c_str() : kEmpty.c_str();
  out->path = has_meta ? meta->second.path.c_str() : kEmpty.c_str();
  const auto hidden = has_meta ? s->scene.layer_hidden.find(meta->second.layer)
                               : s->scene.layer_hidden.end();
  out->layer_hidden = hidden != s->scene.layer_hidden.end() && hidden->second ? 1 : 0;

  out->color[0] = out->color[1] = out->color[2] = out->color[3] = 1.0f;
  if (prim.material_index < s->scene.gltf_materials.size()) {
    const auto& c = s->scene.gltf_materials[prim.material_index].pbr_metallic_roughness.base_color_factor;
    for (int k = 0; k < 4; ++k) out->color[k] = static_cast<float>(c[k]);
  }
  out->uses_layer_color = 0;
  if (has_meta) {
    const auto lc = s->layer_colors.find(meta->second.layer);
    const openskp::Color3 tag = lc != s->layer_colors.end() ? lc->second : openskp::Color3{136, 136, 136};
    bool same = true;
    for (int k = 0; k < 3; ++k) same &= std::abs(out->color[k] * 255.0f - tag[k]) < 0.6f;
    out->uses_layer_color = same ? 1 : 0;
  }
  return 1;
}

// ---------------------------------------------------------------- writing

struct SkpMaterialDesc {
  const char* name;
  std::uint8_t rgba[4];
};

struct SkpFaceDesc {
  // Loop vertex counts: loops[0] is the outer boundary, the rest are holes
  const std::uint32_t* loop_sizes;
  std::size_t loop_count;
  // Concatenated loop points, x/y/z in inches
  const double* points;
  // Hide this face's newly declared edges (used for triangulated fallbacks)
  std::uint8_t soft_edges;
};

struct SkpGroupDesc {
  const char* name;
  std::int32_t layer;     // index into the layer list, -1 = Layer0
  std::int32_t material;  // index into the material list, -1 = default
  const SkpFaceDesc* faces;
  std::size_t face_count;
};

// Writes a new legacy-format (SketchUp 2013-2020 readable) .skp file: one group per entry in
// `groups`. Faces OpenSKP rejects are retried with auto-triangulation and otherwise skipped;
// the number of skipped faces is reported through `skipped_faces`. Returns 1 on success.
int skp_write(const char* path_utf8, const SkpMaterialDesc* materials, std::size_t material_count,
              const char* const* layers, std::size_t layer_count, const SkpGroupDesc* groups,
              std::size_t group_count, std::size_t* skipped_faces, char* err,
              std::size_t err_len) {
  try {
    auto builder = openskp::create();
    std::vector<int> material_slots;
    for (std::size_t i = 0; i < material_count; ++i) {
      const auto& m = materials[i];
      material_slots.push_back(
          builder->add_material(m.name, openskp::Color4{m.rgba[0], m.rgba[1], m.rgba[2], m.rgba[3]}));
    }
    std::vector<int> layer_slots;
    for (std::size_t i = 0; i < layer_count; ++i) layer_slots.push_back(builder->add_layer(layers[i]));

    std::size_t skipped = 0;
    for (std::size_t g = 0; g < group_count; ++g) {
      const auto& desc = groups[g];
      openskp::GroupOptions options;
      options.name = std::string(desc.name);
      if (desc.layer >= 0 && static_cast<std::size_t>(desc.layer) < layer_slots.size())
        options.layer = layer_slots[desc.layer];
      auto& group = builder->add_group(options);

      for (std::size_t f = 0; f < desc.face_count; ++f) {
        const auto& face = desc.faces[f];
        if (face.loop_count == 0) continue;
        std::vector<std::vector<openskp::Point3>> loops;
        const double* p = face.points;
        for (std::size_t l = 0; l < face.loop_count; ++l) {
          std::vector<openskp::Point3> loop;
          for (std::uint32_t v = 0; v < face.loop_sizes[l]; ++v, p += 3) loop.push_back({p[0], p[1], p[2]});
          loops.push_back(std::move(loop));
        }
        openskp::FaceOptions face_options;
        if (desc.material >= 0 && static_cast<std::size_t>(desc.material) < material_slots.size())
          face_options.material = material_slots[desc.material];
        if (desc.layer >= 0 && static_cast<std::size_t>(desc.layer) < layer_slots.size())
          face_options.layer = layer_slots[desc.layer];
        face_options.soft_edges = face.soft_edges != 0;
        face_options.smooth_edges = face.soft_edges != 0;
        face_options.holes.assign(loops.begin() + 1, loops.end());
        try {
          group.add_face(loops[0], face_options);
        } catch (const std::exception&) {
          try {
            face_options.holes.clear();
            face_options.auto_triangulate = true;
            group.add_face(loops[0], face_options);
          } catch (const std::exception&) {
            ++skipped;
          }
        }
      }
      group.close();
    }
    builder->save(std::filesystem::u8path(path_utf8));
    if (skipped_faces) *skipped_faces = skipped;
    return 1;
  } catch (const std::exception& e) {
    write_error(err, err_len, e.what());
  } catch (...) {
    write_error(err, err_len, "unknown OpenSKP error");
  }
  return 0;
}

}  // extern "C"
