//! GPU 3D scene renderer (wgpu): depth-buffered, 4x MSAA, screen-space thick feature edges.
//!
//! The scene is rendered into an offscreen texture which is then displayed as an egui image.
//! Using a real Z-buffer (reverse-Z, f32) instead of a painter's-algorithm triangle sort removes
//! the popping / interpenetration artifacts and stray edge lines of the old 2D-painter renderer.

use bytemuck::{Pod, Zeroable};
use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use wgpu::util::DeviceExt;

const MSAA_SAMPLES: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// One vertex of a flat-shaded face triangle (color already lit on the CPU, sRGB).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct FaceVertex {
    pub pos: [f32; 3],
    pub color: [u8; 4],
    pub elem: u32,
}

/// One corner of a feature-edge quad; the vertex shader expands segment `a`-`b` in screen space.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct EdgeVertex {
    pub a: [f32; 3],
    pub b: [f32; 3],
    /// x: 0 = endpoint a, 1 = endpoint b; y: -1 / +1 side of the line
    pub corner: [f32; 2],
    pub color: [u8; 4],
    pub elem: u32,
}

impl EdgeVertex {
    /// Pushes the 6 vertices (2 triangles) forming the screen-space quad of one edge.
    pub fn push_segment(out: &mut Vec<EdgeVertex>, a: [f32; 3], b: [f32; 3], color: [u8; 4], elem: u32) {
        for corner in [[0.0, -1.0], [1.0, -1.0], [1.0, 1.0], [0.0, -1.0], [1.0, 1.0], [0.0, 1.0]] {
            out.push(EdgeVertex { a, b, corner, color, elem });
        }
    }
}

#[derive(Debug, Default)]
pub struct SceneGeometry {
    pub opaque: Vec<FaceVertex>,
    /// Grouped per element; see [`TranslucentRange`].
    pub translucent: Vec<FaceVertex>,
    pub edges: Vec<EdgeVertex>,
}

/// Contiguous vertex range of one element's translucent triangles.
#[derive(Debug, Clone, Copy)]
pub struct TranslucentRange {
    pub first_vertex: u32,
    pub vertex_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SceneUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    /// xyz: camera right, w: tan(fov/2) * aspect
    pub cam_right: [f32; 4],
    /// xyz: camera up, w: tan(fov/2)
    pub cam_up: [f32; 4],
    /// xyz: camera forward, w: near plane distance
    pub cam_fwd: [f32; 4],
    /// x, y: target size in pixels, z: edge width px, w: selected edge width px
    pub viewport: [f32; 4],
    /// x: axis (0/1/2), y: offset, z: invert (0/1), w: enabled (0/1)
    pub section: [f32; 4],
    /// x: selected element index, y: 1 if a selection exists
    pub selection: [u32; 4],
}

struct Targets {
    size: [u32; 2],
    msaa_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    resolve_view: wgpu::TextureView,
}

pub struct GpuScene {
    render_state: egui_wgpu::RenderState,
    bg_pipeline: wgpu::RenderPipeline,
    opaque_pipeline: wgpu::RenderPipeline,
    translucent_back_pipeline: wgpu::RenderPipeline,
    translucent_front_pipeline: wgpu::RenderPipeline,
    edge_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    opaque_vb: Option<(wgpu::Buffer, u32)>,
    translucent_vb: Option<(wgpu::Buffer, u32)>,
    edge_vb: Option<(wgpu::Buffer, u32)>,
    targets: Option<Targets>,
    texture_id: Option<egui::TextureId>,
}

impl GpuScene {
    pub fn new(render_state: &egui_wgpu::RenderState) -> Self {
        let device = &render_state.device;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sketchcad_scene_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sketchcad_scene_uniforms"),
            size: std::mem::size_of::<SceneUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sketchcad_scene_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sketchcad_scene_bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sketchcad_scene_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let face_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4, 2 => Uint32];
        let face_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<FaceVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &face_attrs,
        };
        let edge_attrs = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Unorm8x4, 4 => Uint32
        ];
        let edge_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<EdgeVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &edge_attrs,
        };

        let premultiplied = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);

        let make_pipeline = |label: &str,
                             vs: &str,
                             fs: &str,
                             buffers: &[wgpu::VertexBufferLayout],
                             blend: Option<wgpu::BlendState>,
                             cull_mode: Option<wgpu::Face>,
                             depth_write: bool,
                             depth_compare: wgpu::CompareFunction| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: depth_write,
                    depth_compare,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: MSAA_SAMPLES,
                    ..Default::default()
                },
                multiview: None,
                cache: None,
            })
        };

        // Reverse-Z: larger depth = closer to the camera.
        // Solids are drawn two-sided so open / inconsistently wound meshes never show holes.
        let bg_pipeline = make_pipeline("sketchcad_bg", "vs_bg", "fs_bg", &[], None, None, false, wgpu::CompareFunction::Always);
        let opaque_pipeline = make_pipeline(
            "sketchcad_opaque",
            "vs_face",
            "fs_face",
            &[face_layout.clone()],
            None,
            None,
            true,
            wgpu::CompareFunction::GreaterEqual,
        );
        // Translucent elements: back faces first, then front faces, so every triangle is drawn
        // exactly once and blended in a consistent order.
        let translucent_back_pipeline = make_pipeline(
            "sketchcad_translucent_back",
            "vs_face_biased",
            "fs_face",
            &[face_layout.clone()],
            premultiplied,
            Some(wgpu::Face::Front),
            false,
            wgpu::CompareFunction::GreaterEqual,
        );
        let translucent_front_pipeline = make_pipeline(
            "sketchcad_translucent_front",
            "vs_face_biased",
            "fs_face",
            &[face_layout],
            premultiplied,
            Some(wgpu::Face::Back),
            false,
            wgpu::CompareFunction::GreaterEqual,
        );
        let edge_pipeline = make_pipeline(
            "sketchcad_edges",
            "vs_edge",
            "fs_edge",
            &[edge_layout],
            premultiplied,
            None,
            false,
            wgpu::CompareFunction::GreaterEqual,
        );

        Self {
            render_state: render_state.clone(),
            bg_pipeline,
            opaque_pipeline,
            translucent_back_pipeline,
            translucent_front_pipeline,
            edge_pipeline,
            uniform_buffer,
            bind_group,
            opaque_vb: None,
            translucent_vb: None,
            edge_vb: None,
            targets: None,
            texture_id: None,
        }
    }

    pub fn max_texture_dimension(&self) -> u32 {
        self.render_state.device.limits().max_texture_dimension_2d
    }

    /// Uploads static scene geometry. Called only when the document or style settings change.
    pub fn upload_geometry(&mut self, geom: &SceneGeometry) {
        let device = &self.render_state.device;
        let make = |label: &str, data: &[u8], count: usize| {
            (count > 0).then(|| {
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: data,
                    usage: wgpu::BufferUsages::VERTEX,
                });
                (buf, count as u32)
            })
        };
        self.opaque_vb = make("sketchcad_opaque_vb", bytemuck::cast_slice(&geom.opaque), geom.opaque.len());
        self.translucent_vb = make(
            "sketchcad_translucent_vb",
            bytemuck::cast_slice(&geom.translucent),
            geom.translucent.len(),
        );
        self.edge_vb = make("sketchcad_edge_vb", bytemuck::cast_slice(&geom.edges), geom.edges.len());
    }

    fn ensure_targets(&mut self, size: [u32; 2]) -> egui::TextureId {
        if self.targets.as_ref().map_or(true, |t| t.size != size) {
            let device = &self.render_state.device;
            let extent = wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            };
            let make_tex = |label: &str, format, samples, usage| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let view_of = |tex: &wgpu::Texture| tex.create_view(&wgpu::TextureViewDescriptor::default());
            let msaa = make_tex("sketchcad_msaa", COLOR_FORMAT, MSAA_SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
            let depth = make_tex("sketchcad_depth", DEPTH_FORMAT, MSAA_SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
            let resolve = make_tex(
                "sketchcad_resolve",
                COLOR_FORMAT,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            );
            let targets = Targets {
                size,
                msaa_view: view_of(&msaa),
                depth_view: view_of(&depth),
                resolve_view: view_of(&resolve),
            };

            let mut renderer = self.render_state.renderer.write();
            match self.texture_id {
                Some(id) => renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &targets.resolve_view,
                    wgpu::FilterMode::Linear,
                    id,
                ),
                None => {
                    self.texture_id = Some(renderer.register_native_texture(
                        device,
                        &targets.resolve_view,
                        wgpu::FilterMode::Linear,
                    ))
                }
            }
            self.targets = Some(targets);
        }
        self.texture_id.expect("texture registered above")
    }

    /// Renders the scene offscreen and returns the egui texture holding the resolved image.
    pub fn render(
        &mut self,
        size: [u32; 2],
        uniforms: &SceneUniforms,
        translucent_back_to_front: &[TranslucentRange],
        draw_edges: bool,
    ) -> egui::TextureId {
        let texture_id = self.ensure_targets(size);
        let targets = self.targets.as_ref().expect("targets created above");
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sketchcad_scene_encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sketchcad_scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.msaa_view,
                    resolve_target: Some(&targets.resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0), // reverse-Z: 0 = infinitely far
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);

            pass.set_pipeline(&self.bg_pipeline);
            pass.draw(0..3, 0..1);

            if let Some((vb, count)) = &self.opaque_vb {
                pass.set_pipeline(&self.opaque_pipeline);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..*count, 0..1);
            }
            if draw_edges {
                if let Some((vb, count)) = &self.edge_vb {
                    pass.set_pipeline(&self.edge_pipeline);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.draw(0..*count, 0..1);
                }
            }
            if let Some((vb, _)) = &self.translucent_vb {
                pass.set_vertex_buffer(0, vb.slice(..));
                for range in translucent_back_to_front {
                    let vertices = range.first_vertex..range.first_vertex + range.vertex_count;
                    pass.set_pipeline(&self.translucent_back_pipeline);
                    pass.draw(vertices.clone(), 0..1);
                    pass.set_pipeline(&self.translucent_front_pipeline);
                    pass.draw(vertices, 0..1);
                }
            }
        }
        queue.submit(Some(encoder.finish()));
        texture_id
    }
}

impl Drop for GpuScene {
    fn drop(&mut self) {
        if let Some(id) = self.texture_id.take() {
            self.render_state.renderer.write().free_texture(&id);
        }
    }
}

const SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    cam_fwd: vec4<f32>,
    viewport: vec4<f32>,
    section: vec4<f32>,
    selection: vec4<u32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

const SELECT_GOLD: vec3<f32> = vec3<f32>(1.0, 0.68, 0.0);

fn linear_from_srgb(c: vec3<f32>) -> vec3<f32> {
    let cutoff = c < vec3<f32>(0.04045);
    let lower = c / vec3<f32>(12.92);
    let higher = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(higher, lower, cutoff);
}

fn is_selected(elem: u32) -> bool {
    return u.selection.y == 1u && elem == u.selection.x;
}

// Per-pixel section cut: true when the world point lies on the removed side of the plane.
fn section_removed(p: vec3<f32>) -> bool {
    if (u.section.w < 0.5) {
        return false;
    }
    let axis = i32(u.section.x + 0.5);
    var c = p.z;
    if (axis == 0) {
        c = p.x;
    } else if (axis == 1) {
        c = p.y;
    }
    if (u.section.z > 0.5) {
        return c < u.section.y;
    }
    return c > u.section.y;
}

// ---------- Background (sky / ground with true perspective horizon) ----------
struct BgOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_bg(@builtin(vertex_index) i: u32) -> BgOut {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    var o: BgOut;
    o.pos = vec4<f32>(x, y, 0.0, 1.0);
    o.ndc = vec2<f32>(x, y);
    return o;
}

@fragment
fn fs_bg(in: BgOut) -> @location(0) vec4<f32> {
    let dir = normalize(
        u.cam_fwd.xyz
        + in.ndc.x * u.cam_right.w * u.cam_right.xyz
        + in.ndc.y * u.cam_up.w * u.cam_up.xyz
    );
    let sky_horizon = vec3<f32>(205.0, 225.0, 242.0) / 255.0;
    let sky_zenith = vec3<f32>(150.0, 192.0, 232.0) / 255.0;
    let ground_near = vec3<f32>(214.0, 216.0, 209.0) / 255.0;
    let ground_far = vec3<f32>(226.0, 228.0, 222.0) / 255.0;
    let sky = mix(sky_horizon, sky_zenith, clamp(dir.z * 1.6, 0.0, 1.0));
    let ground = mix(ground_far, ground_near, clamp(-dir.z * 2.5, 0.0, 1.0));
    let t = smoothstep(-0.003, 0.003, dir.z);
    return vec4<f32>(linear_from_srgb(mix(ground, sky, t)), 1.0);
}

// ---------- Faces ----------
struct FaceOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world: vec3<f32>,
    @location(2) @interpolate(flat) elem: u32,
};

fn face_common(pos: vec3<f32>, color: vec4<f32>, elem: u32) -> FaceOut {
    var o: FaceOut;
    o.pos = u.view_proj * vec4<f32>(pos, 1.0);
    o.color = vec4<f32>(linear_from_srgb(color.rgb), color.a);
    o.world = pos;
    o.elem = elem;
    return o;
}

@vertex
fn vs_face(@location(0) pos: vec3<f32>, @location(1) color: vec4<f32>, @location(2) elem: u32) -> FaceOut {
    return face_common(pos, color, elem);
}

// Translucent surfaces are nudged slightly toward the camera so that coplanar overlays
// (floor finish plates, light pools, glass in frames) never z-fight with solids.
@vertex
fn vs_face_biased(@location(0) pos: vec3<f32>, @location(1) color: vec4<f32>, @location(2) elem: u32) -> FaceOut {
    var o = face_common(pos, color, elem);
    o.pos.z = o.pos.z * 1.00008;
    return o;
}

@fragment
fn fs_face(in: FaceOut) -> @location(0) vec4<f32> {
    if (section_removed(in.world)) {
        discard;
    }
    var c = in.color;
    if (is_selected(in.elem)) {
        c = vec4<f32>(mix(c.rgb, SELECT_GOLD, 0.25), max(c.a, 0.45));
    }
    return vec4<f32>(c.rgb * c.a, c.a);
}

// ---------- Screen-space thick edges ----------
struct EdgeOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_edge(
    @location(0) a: vec3<f32>,
    @location(1) b: vec3<f32>,
    @location(2) corner: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) elem: u32,
) -> EdgeOut {
    var o: EdgeOut;
    var va = u.view * vec4<f32>(a, 1.0);
    var vb = u.view * vec4<f32>(b, 1.0);
    var ta = 0.0;
    var tb = 1.0;

    // Clip the segment against the near plane in view space (camera looks down -Z).
    let clip_z = -u.cam_fwd.w * 1.5;
    if (va.z > clip_z && vb.z > clip_z) {
        o.pos = vec4<f32>(0.0, 0.0, -2.0, 1.0);
        o.color = vec4<f32>(0.0);
        o.world = a;
        return o;
    }
    if (va.z > clip_z) {
        let t = (clip_z - va.z) / (vb.z - va.z);
        va = mix(va, vb, t);
        ta = t;
    } else if (vb.z > clip_z) {
        let t = (clip_z - vb.z) / (va.z - vb.z);
        vb = mix(vb, va, t);
        tb = 1.0 - t;
    }

    let ca = u.proj * va;
    let cb = u.proj * vb;
    let half_vp = u.viewport.xy * 0.5;
    let sa = ca.xy / ca.w * half_vp;
    let sb = cb.xy / cb.w * half_vp;
    var dir = sb - sa;
    let len = length(dir);
    if (len < 1e-4) {
        dir = vec2<f32>(1.0, 0.0);
    } else {
        dir = dir / len;
    }
    let nrm = vec2<f32>(-dir.y, dir.x);

    let selected = is_selected(elem);
    let half_width = select(u.viewport.z, u.viewport.w, selected) * 0.5;
    let use_b = corner.x > 0.5;
    var c = select(ca, cb, use_b);
    let along = select(-1.0, 1.0, use_b);
    let offset_px = nrm * corner.y * half_width + dir * along * half_width;
    c = vec4<f32>(c.xy + offset_px / half_vp * c.w, c.z * 1.0004, c.w);

    o.pos = c;
    if (selected) {
        o.color = vec4<f32>(SELECT_GOLD, 1.0);
    } else {
        o.color = vec4<f32>(linear_from_srgb(color.rgb), color.a);
    }
    o.world = mix(a, b, select(ta, tb, use_b));
    return o;
}

@fragment
fn fs_edge(in: EdgeOut) -> @location(0) vec4<f32> {
    if (section_removed(in.world)) {
        discard;
    }
    return vec4<f32>(in.color.rgb * in.color.a, in.color.a);
}
"#;

