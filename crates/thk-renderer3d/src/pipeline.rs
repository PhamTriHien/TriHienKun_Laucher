use crate::geometry::{build_character_meshes, BackEquipmentMode, SkinVertex};
use crate::skin_image::{generate_default_brand_cape, generate_default_steve_skin};
use bytemuck::{Pod, Zeroable};
use core::num::NonZeroU64;
use egui_wgpu::wgpu::util::DeviceExt;
use egui_wgpu::{wgpu, RenderState};
use glam::{Mat4, Vec3, Vec4};
use image::RgbaImage;
use std::borrow::Cow;
use std::time::Duration;
use thk_core::models::SkinVariant;

const MSAA_SAMPLES: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MAX_VERTICES: usize = 2048;
const MAX_INDICES: usize = 3072;

const SKIN_SHADER_WGSL: &str = r#"
struct SceneUniforms {
    mvp: mat4x4<f32>,
    light_dir: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;

@group(1) @binding(0)
var tex_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var samp_diffuse: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_pos = scene.mvp * vec4<f32>(in.position, 1.0);
    out.world_normal = in.normal;
    out.uv = in.uv;
    return out;
}

fn shade_sample(sample: vec4<f32>, normal_in: vec3<f32>) -> vec4<f32> {
    let n = normalize(normal_in);
    let key_dir = normalize(scene.light_dir.xyz);
    let fill_dir = normalize(vec3<f32>(-0.55, 0.35, -0.75));
    let key = max(dot(n, key_dir), 0.0);
    let fill = max(dot(n, fill_dir), 0.0);
    let hemi = n.y * 0.08 + 0.54;
    let intensity = clamp(hemi + key * 0.38 + fill * 0.14, 0.35, 1.08);
    let rgb = clamp(sample.rgb * intensity, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(rgb * sample.a, sample.a);
}

@fragment
fn fs_cutout(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    let sample = textureSample(tex_diffuse, samp_diffuse, in.uv);
    if sample.a < 0.92 {
        discard;
    }
    let oriented_n = select(-in.world_normal, in.world_normal, is_front);
    return shade_sample(vec4<f32>(sample.rgb, 1.0), oriented_n);
}

@fragment
fn fs_translucent(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    let sample = textureSample(tex_diffuse, samp_diffuse, in.uv);
    if sample.a < 0.05 || sample.a >= 0.92 {
        discard;
    }
    let oriented_n = select(-in.world_normal, in.world_normal, is_front);
    return shade_sample(sample, oriented_n);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    mvp: [[f32; 4]; 4],
    light_dir: [f32; 4],
}

struct OffscreenTarget {
    width: u32,
    height: u32,
    _msaa_color: wgpu::Texture,
    msaa_view: wgpu::TextureView,
    _resolved_color: wgpu::Texture,
    resolved_view: wgpu::TextureView,
    _depth_tex: wgpu::Texture,
    depth_view: wgpu::TextureView,
    egui_tex_id: egui::TextureId,
}

pub struct SkinPreviewViewport {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub variant: SkinVariant,
    pub back_mode: BackEquipmentMode,
    pub show_outer_layer: bool,
    pub animate_walk: bool,
    pub auto_rotate: bool,
    anim_phase: f32,

    cutout_pipeline: wgpu::RenderPipeline,
    translucent_pipeline: wgpu::RenderPipeline,
    tex_bind_layout: wgpu::BindGroupLayout,
    nearest_sampler: wgpu::Sampler,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,

    skin_vbuf: wgpu::Buffer,
    skin_ibuf: wgpu::Buffer,
    back_vbuf: wgpu::Buffer,
    back_ibuf: wgpu::Buffer,

    skin_bg: wgpu::BindGroup,
    cape_bg: wgpu::BindGroup,
    target: Option<OffscreenTarget>,
}

impl SkinPreviewViewport {
    pub fn new(render_state: &RenderState) -> Self {
        let device = &render_state.device;
        let queue = &render_state.queue;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("thk_skin3d_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SKIN_SHADER_WGSL)),
        });

        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thk_skin3d_uniform_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(core::mem::size_of::<SceneUniforms>() as u64),
                },
                count: None,
            }],
        });

        let tex_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thk_skin3d_tex_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("thk_skin3d_pipeline_layout"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&tex_bind_layout)],
            immediate_size: 0,
        });

        let cutout_pipeline = create_skin_pipeline(
            device,
            &pipeline_layout,
            &shader,
            "fs_cutout",
            true,
        );
        let translucent_pipeline = create_skin_pipeline(
            device,
            &pipeline_layout,
            &shader,
            "fs_translucent",
            false,
        );

        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("thk_skin3d_nearest_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("thk_skin3d_uniform_buf"),
            contents: bytemuck::bytes_of(&SceneUniforms {
                mvp: Mat4::IDENTITY.to_cols_array_2d(),
                light_dir: [0.45, 0.72, 0.68, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thk_skin3d_uniform_bg"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        let vbuf_bytes = (MAX_VERTICES * core::mem::size_of::<SkinVertex>()) as u64;
        let ibuf_bytes = (MAX_INDICES * core::mem::size_of::<u32>()) as u64;

        let skin_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thk_skin3d_skin_vbuf"),
            size: vbuf_bytes,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let skin_ibuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thk_skin3d_skin_ibuf"),
            size: ibuf_bytes,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let back_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thk_skin3d_back_vbuf"),
            size: vbuf_bytes,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let back_ibuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thk_skin3d_back_ibuf"),
            size: ibuf_bytes,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let default_skin = generate_default_steve_skin();
        let default_cape = generate_default_brand_cape();
        let skin_bg = upload_rgba_bind_group(
            device,
            queue,
            &tex_bind_layout,
            &nearest_sampler,
            &default_skin,
            "thk_default_skin_tex",
        );
        let cape_bg = upload_rgba_bind_group(
            device,
            queue,
            &tex_bind_layout,
            &nearest_sampler,
            &default_cape,
            "thk_default_cape_tex",
        );

        Self {
            yaw: 0.42,
            pitch: 0.14,
            distance: 48.0,
            variant: SkinVariant::Wide,
            back_mode: BackEquipmentMode::Cape,
            show_outer_layer: true,
            animate_walk: true,
            auto_rotate: false,
            anim_phase: 0.0,
            cutout_pipeline,
            translucent_pipeline,
            tex_bind_layout,
            nearest_sampler,
            uniform_buf,
            uniform_bg,
            skin_vbuf,
            skin_ibuf,
            back_vbuf,
            back_ibuf,
            skin_bg,
            cape_bg,
            target: None,
        }
    }

    pub fn set_skin_rgba(&mut self, render_state: &RenderState, img: &RgbaImage) {
        self.skin_bg = upload_rgba_bind_group(
            &render_state.device,
            &render_state.queue,
            &self.tex_bind_layout,
            &self.nearest_sampler,
            img,
            "thk_user_skin_tex",
        );
    }

    pub fn reset_default_skin(&mut self, render_state: &RenderState) {
        let img = generate_default_steve_skin();
        self.set_skin_rgba(render_state, &img);
    }

    pub fn set_cape_rgba(&mut self, render_state: &RenderState, img: &RgbaImage) {
        self.cape_bg = upload_rgba_bind_group(
            &render_state.device,
            &render_state.queue,
            &self.tex_bind_layout,
            &self.nearest_sampler,
            img,
            "thk_user_cape_tex",
        );
    }

    pub fn reset_default_cape(&mut self, render_state: &RenderState) {
        let img = generate_default_brand_cape();
        self.set_cape_rgba(render_state, &img);
    }

    pub fn reset_camera(&mut self, face_back: bool) {
        self.yaw = if face_back {
            std::f32::consts::PI - 0.35
        } else {
            0.42
        };
        self.pitch = 0.14;
        self.distance = 48.0;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, render_state: &RenderState, desired_size: egui::Vec2) {
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());
        if !ui.is_rect_visible(rect) {
            return;
        }

        if response.dragged() {
            let delta = response.drag_delta();
            self.yaw += delta.x * 0.013;
            self.pitch = (self.pitch + delta.y * 0.011).clamp(-1.35, 1.35);
        }
        if response.double_clicked() {
            self.reset_camera(self.back_mode != BackEquipmentMode::None && self.yaw.cos() < 0.0);
        }
        if response.hovered() {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y.abs() > 0.01 {
                self.distance = (self.distance - scroll_y * 0.06).clamp(26.0, 82.0);
            }
        }

        let dt = ui.input(|i| i.unstable_dt).min(0.1);
        if self.animate_walk {
            self.anim_phase = (self.anim_phase + dt * 3.2) % (std::f32::consts::TAU * 2.0);
        } else {
            self.anim_phase = 0.0;
        }
        if self.auto_rotate && !response.dragged() {
            self.yaw = (self.yaw + dt * 0.75) % std::f32::consts::TAU;
        }
        if self.animate_walk || self.auto_rotate {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }

        let ppp = ui.ctx().pixels_per_point().max(1.0);
        let px_w = ((rect.width() * ppp) as u32).clamp(128, 1024);
        let px_h = ((rect.height() * ppp) as u32).clamp(128, 1024);

        self.ensure_offscreen_target(render_state, px_w, px_h);
        self.render_offscreen_frame(render_state, px_w, px_h);

        if let Some(ref target) = self.target {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(12),
                egui::Color32::from_rgb(16, 20, 28),
            );
            let mut mesh = egui::Mesh::with_texture(target.egui_tex_id);
            mesh.add_rect_with_uv(
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            ui.painter().add(egui::Shape::mesh(mesh));
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(12),
                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(56, 189, 126, 70)),
                egui::StrokeKind::Inside,
            );
        }
    }

    fn ensure_offscreen_target(&mut self, render_state: &RenderState, width: u32, height: u32) {
        let needs_recreate = self
            .target
            .as_ref()
            .map(|t| t.width != width || t.height != height)
            .unwrap_or(true);
        if !needs_recreate {
            return;
        }

        let device = &render_state.device;
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let msaa_color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("thk_skin3d_msaa_color"),
            size: extent,
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa_color.create_view(&wgpu::TextureViewDescriptor::default());

        let resolved_color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("thk_skin3d_resolved_color"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let resolved_view = resolved_color.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("thk_skin3d_depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let mut egui_renderer = render_state.renderer.write();
        let egui_tex_id = if let Some(prev) = self.target.take() {
            egui_renderer.update_egui_texture_from_wgpu_texture(
                device,
                &resolved_view,
                wgpu::FilterMode::Linear,
                prev.egui_tex_id,
            );
            prev.egui_tex_id
        } else {
            egui_renderer.register_native_texture(device, &resolved_view, wgpu::FilterMode::Linear)
        };

        self.target = Some(OffscreenTarget {
            width,
            height,
            _msaa_color: msaa_color,
            msaa_view,
            _resolved_color: resolved_color,
            resolved_view,
            _depth_tex: depth_tex,
            depth_view,
            egui_tex_id,
        });
    }

    fn render_offscreen_frame(&mut self, render_state: &RenderState, width: u32, height: u32) {
        let Some(ref target) = self.target else {
            return;
        };

        let aspect = (width as f32 / height.max(1) as f32).max(0.1);
        let proj = perspective_wgpu_rh(38.0f32.to_radians(), aspect, 2.0, 220.0);
        let target_center = Vec3::new(0.0, 16.5, 0.0);
        let eye_offset = Vec3::new(
            self.distance * self.pitch.cos() * self.yaw.sin(),
            self.distance * self.pitch.sin(),
            self.distance * self.pitch.cos() * self.yaw.cos(),
        );
        let eye = target_center + eye_offset;
        let view = look_at_rh(eye, target_center, Vec3::Y);
        let mvp = proj * view;
        let light_dir = (eye_offset.normalize_or_zero() + Vec3::new(0.38, 0.65, 0.25)).normalize();

        let uniforms = SceneUniforms {
            mvp: mvp.to_cols_array_2d(),
            light_dir: [light_dir.x, light_dir.y, light_dir.z, 0.0],
        };
        render_state
            .queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        let batch = build_character_meshes(
            self.variant,
            self.show_outer_layer,
            self.back_mode,
            self.anim_phase,
        );

        let skin_v_cnt = batch.skin_vertices.len().min(MAX_VERTICES);
        let skin_i_cnt = batch.skin_indices.len().min(MAX_INDICES);
        if skin_v_cnt > 0 && skin_i_cnt > 0 {
            render_state.queue.write_buffer(
                &self.skin_vbuf,
                0,
                bytemuck::cast_slice(&batch.skin_vertices[..skin_v_cnt]),
            );
            render_state.queue.write_buffer(
                &self.skin_ibuf,
                0,
                bytemuck::cast_slice(&batch.skin_indices[..skin_i_cnt]),
            );
        }

        let back_v_cnt = batch.back_vertices.len().min(MAX_VERTICES);
        let back_i_cnt = batch.back_indices.len().min(MAX_INDICES);
        if back_v_cnt > 0 && back_i_cnt > 0 {
            render_state.queue.write_buffer(
                &self.back_vbuf,
                0,
                bytemuck::cast_slice(&batch.back_vertices[..back_v_cnt]),
            );
            render_state.queue.write_buffer(
                &self.back_ibuf,
                0,
                bytemuck::cast_slice(&batch.back_indices[..back_i_cnt]),
            );
        }

        let mut encoder =
            render_state
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("thk_skin3d_encoder"),
                });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("thk_skin3d_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&target.resolved_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Pass 1: Opaque / Alpha-Cutout
            rpass.set_pipeline(&self.cutout_pipeline);
            rpass.set_bind_group(0, &self.uniform_bg, &[]);
            if skin_i_cnt > 0 {
                rpass.set_bind_group(1, &self.skin_bg, &[]);
                rpass.set_vertex_buffer(0, self.skin_vbuf.slice(..));
                rpass.set_index_buffer(self.skin_ibuf.slice(..), wgpu::IndexFormat::Uint32);
                rpass.draw_indexed(0..skin_i_cnt as u32, 0, 0..1);
            }
            if back_i_cnt > 0 {
                rpass.set_bind_group(1, &self.cape_bg, &[]);
                rpass.set_vertex_buffer(0, self.back_vbuf.slice(..));
                rpass.set_index_buffer(self.back_ibuf.slice(..), wgpu::IndexFormat::Uint32);
                rpass.draw_indexed(0..back_i_cnt as u32, 0, 0..1);
            }

            // Pass 2: Semi-transparent overlay pixels
            rpass.set_pipeline(&self.translucent_pipeline);
            rpass.set_bind_group(0, &self.uniform_bg, &[]);
            if skin_i_cnt > 0 {
                rpass.set_bind_group(1, &self.skin_bg, &[]);
                rpass.set_vertex_buffer(0, self.skin_vbuf.slice(..));
                rpass.set_index_buffer(self.skin_ibuf.slice(..), wgpu::IndexFormat::Uint32);
                rpass.draw_indexed(0..skin_i_cnt as u32, 0, 0..1);
            }
        }

        render_state.queue.submit(core::iter::once(encoder.finish()));
    }
}

fn create_skin_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    fs_entry: &str,
    depth_write: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fs_entry),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: core::mem::size_of::<SkinVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x3,
                    1 => Float32x3,
                    2 => Float32x2
                ],
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(depth_write),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: MSAA_SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn upload_rgba_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    img: &RgbaImage,
    label: &str,
) -> wgpu::BindGroup {
    let (width, height) = img.dimensions();
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        img.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

pub(crate) fn perspective_wgpu_rh(
    fov_y_radians: f32,
    aspect_ratio: f32,
    z_near: f32,
    z_far: f32,
) -> Mat4 {
    let inv_tan = 1.0 / (0.5 * fov_y_radians).tan();
    let r = z_far / (z_near - z_far);
    Mat4::from_cols(
        Vec4::new(inv_tan / aspect_ratio, 0.0, 0.0, 0.0),
        Vec4::new(0.0, inv_tan, 0.0, 0.0),
        Vec4::new(0.0, 0.0, r, -1.0),
        Vec4::new(0.0, 0.0, r * z_near, 0.0),
    )
}

pub(crate) fn look_at_rh(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    let f = (center - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols(
        Vec4::new(s.x, u.x, -f.x, 0.0),
        Vec4::new(s.y, u.y, -f.y, 0.0),
        Vec4::new(s.z, u.z, -f.z, 0.0),
        Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    )
}
