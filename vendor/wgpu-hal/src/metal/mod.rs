/*!
# Metal API internals.

## Pipeline Layout

In Metal, immediates, vertex buffers, and resources in the bind groups
are all placed together in the native resource bindings, which work similarly to D3D11:
there are tables of textures, buffers, and samplers.

We put immediates first (if any) in the table, followed by bind group 0
resources, followed by other bind groups. The vertex buffers are bound at the very
end of the VS buffer table.

!*/

// `MTLFeatureSet` is superseded by `MTLGpuFamily`.
// However, `MTLGpuFamily` is only supported starting MacOS 10.15, whereas our minimum target is MacOS 10.13,
// See https://github.com/gpuweb/gpuweb/issues/1069 for minimum spec.
// TODO: Eventually all deprecated features should be abstracted and use new api when available.
#[allow(deprecated)]
mod adapter;
mod command;
mod conv;
mod device;
mod library_from_metallib;
mod surface;
mod time;

use alloc::{
    string::{String, ToString as _},
    sync::Arc,
    vec::Vec,
};
use core::{fmt, iter, ops, ptr::NonNull, sync::atomic};

use bitflags::bitflags;
use hashbrown::HashMap;
use naga::FastHashMap;
use objc2::{
    available,
    rc::{autoreleasepool, Retained},
    runtime::ProtocolObject,
};
use objc2_foundation::ns_string;
use objc2_metal::{
    MTLAccelerationStructure, MTLAccelerationStructureCommandEncoder, MTLArgumentBuffersTier,
    MTLBlitCommandEncoder, MTLBuffer, MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandQueue,
    MTLComputeCommandEncoder, MTLComputePipelineState, MTLCounterSampleBuffer, MTLCullMode,
    MTLDepthClipMode, MTLDepthStencilState, MTLDevice, MTLDrawable, MTLIndexType,
    MTLLanguageVersion, MTLLibrary, MTLPrimitiveType, MTLReadWriteTextureTier,
    MTLRenderCommandEncoder, MTLRenderPipelineState, MTLRenderStages, MTLResource,
    MTLResourceUsage, MTLSamplerState, MTLSharedEvent, MTLSize, MTLTexture, MTLTextureType,
    MTLTriangleFillMode, MTLWinding,
};
use objc2_quartz_core::CAMetalLayer;
use parking_lot::{Mutex, RwLock};

#[derive(Clone, Debug)]
pub struct Api;

type ResourceIndex = u32;

impl crate::Api for Api {
    const VARIANT: wgt::Backend = wgt::Backend::Metal;

    type Instance = Instance;
    type Surface = Surface;
    type Adapter = Adapter;
    type Device = Device;

    type Queue = Queue;
    type CommandEncoder = CommandEncoder;
    type CommandBuffer = CommandBuffer;

    type Buffer = Buffer;
    type Texture = Texture;
    type SurfaceTexture = SurfaceTexture;
    type TextureView = TextureView;
    type Sampler = Sampler;
    type QuerySet = QuerySet;
    type Fence = Fence;

    type BindGroupLayout = BindGroupLayout;
    type BindGroup = BindGroup;
    type PipelineLayout = PipelineLayout;
    type ShaderModule = ShaderModule;
    type RenderPipeline = RenderPipeline;
    type ComputePipeline = ComputePipeline;
    type PipelineCache = PipelineCache;

    type AccelerationStructure = AccelerationStructure;
}

crate::impl_dyn_resource!(
    Adapter,
    AccelerationStructure,
    BindGroup,
    BindGroupLayout,
    Buffer,
    CommandBuffer,
    CommandEncoder,
    ComputePipeline,
    Device,
    Fence,
    Instance,
    PipelineCache,
    PipelineLayout,
    QuerySet,
    Queue,
    RenderPipeline,
    Sampler,
    ShaderModule,
    Surface,
    SurfaceTexture,
    Texture,
    TextureView
);

/// Provides availability information about Mac APIs.
///
/// This may include Metal features that depend only on software support.
/// Features with varying hardware support are in [`CapabilitiesQuery`]
///
/// When feature detection is only needed once, it may also be done inline.
struct OsFeatures;

impl OsFeatures {
    fn display_sync() -> bool {
        // https://developer.apple.com/documentation/quartzcore/cametallayer/displaysyncenabled
        available!(macos = 10.13) || cfg!(target_abi = "macabi")
    }
}

/// Present pacing and on-screen timing hooks for the IW4 Pocket iOS build (not
/// upstream). Both are read on every `Queue::present`; left at their defaults
/// (0 and unset) the present is the upstream `presentDrawable:` call.
pub mod pacing {
    use alloc::borrow::ToOwned as _;
    use alloc::string::{String, ToString as _};
    use core::sync::atomic::AtomicU32;
    use std::sync::OnceLock;

    /// Minimum time, in microseconds, the previous drawable stays on screen
    /// before this one may replace it, passed to
    /// `presentDrawable:afterMinimumDuration:`. 0 = plain `presentDrawable:`.
    pub static MIN_PRESENT_US: AtomicU32 = AtomicU32::new(0);

    /// Called for every drawable presented without a transaction.
    pub struct Hooks {
        /// On the presenting thread, after `commit`: the drawable's
        /// `drawableID` and the nanoseconds spent presenting and committing.
        pub submitted: fn(drawable_id: u64, present_call_ns: u64),
        /// From Metal's presented handler, on any thread: `presentedTime` in
        /// host seconds, 0.0 when the drawable was never shown.
        pub presented: fn(drawable_id: u64, presented_host_s: f64),
    }

    /// Unset: no presented handler is added and nothing is timed.
    pub static HOOKS: OnceLock<Hooks> = OnceLock::new();

    /// Command buffers Metal finished with `MTLCommandBufferStatusError`, counted
    /// while `HOOKS` is set, and the last one's `NSError` code
    /// (`MTLCommandBufferError`: 1 internal, 2 timeout, 3 page fault, 4 blacklisted,
    /// 5 not permitted, 7 out of memory, 8 invalid resource, 9 memoryless, 11 stack
    /// overflow).
    pub static GPU_ERRORS: AtomicU32 = AtomicU32::new(0);
    pub static LAST_GPU_ERROR: core::sync::atomic::AtomicI64 =
        core::sync::atomic::AtomicI64::new(0);

    pub(super) fn note_completed(
        buffer: &objc2::runtime::ProtocolObject<dyn objc2_metal::MTLCommandBuffer>,
    ) {
        use core::sync::atomic::Ordering;
        const STATUS_ERROR: isize = 5;
        let in_flight = IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        // Untyped: the typed `status`/`error` need features this build leaves off.
        let status: isize = unsafe { objc2::msg_send![buffer, status] };
        if status != STATUS_ERROR {
            note_succeeded(buffer);
            return;
        }
        let error: Option<objc2::rc::Retained<objc2::runtime::NSObject>> =
            unsafe { objc2::msg_send![buffer, error] };
        let code: isize = error
            .as_ref()
            .map_or(-1, |error| unsafe { objc2::msg_send![&**error, code] });
        LAST_GPU_ERROR.store(code as i64, Ordering::Relaxed);
        if GPU_ERRORS.fetch_add(1, Ordering::Relaxed) == 0 {
            let device: Option<objc2::rc::Retained<objc2::runtime::NSObject>> =
                unsafe { objc2::msg_send![buffer, device] };
            if let Some(device) = device {
                let held: usize = unsafe { objc2::msg_send![&*device, currentAllocatedSize] };
                let held = held as u64;
                GPU_ALLOCATED_AT_FIRST_ERROR.store(held, Ordering::Relaxed);
            }
        }
        // Which command buffer (its label names the pass or encoder that recorded it)
        // and Metal's own description, kept for the log.
        let text = |object: Option<objc2::rc::Retained<objc2_foundation::NSString>>| {
            object.map_or_else(|| "none".to_owned(), |s| s.to_string())
        };
        let label = text(unsafe { objc2::msg_send![buffer, label] });
        let description = text(
            error
                .as_ref()
                .and_then(|error| unsafe { objc2::msg_send![&**error, localizedDescription] }),
        );
        let failed = GPU_ERRORS.load(Ordering::Relaxed);
        let left = FAILED_SAMPLE_BUDGET.load(Ordering::Relaxed);
        let budgeted = left > 0
            && FAILED_SAMPLE_BUDGET
                .compare_exchange(left, left - 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok();
        if budgeted || failed % 1000 == 0 {
            sample(
                &FAILED_SAMPLES,
                format!("#{failed} ({in_flight} buffers queued) {label}"),
            );
        }
        if let Ok(mut last) = LAST_GPU_ERROR_TEXT.lock() {
            *last = Some(format!("{label}: {description}"));
        }
    }

    /// Submission changes the app can switch at run time to see which one stops
    /// command buffers failing (bits of [`MODE`]):
    /// - [`SKIP_EMPTY`]: a command buffer no encoder was opened on (wgpu-core's
    ///   barrier-only "Pre Pass"/"Transit" buffers, which hold nothing on Metal) is
    ///   not committed, unless it is the submission's last (it carries the fence).
    /// - [`ONE_BY_ONE`]: wait until each committed buffer is scheduled before the
    ///   next is committed, so Metal can't hand several to the kernel as one.
    /// - [`ONE_FRAME`]: wait for each present buffer to complete, so only one
    ///   frame is on the GPU at a time.
    pub static MODE: AtomicU32 = AtomicU32::new(0);
    pub const SKIP_EMPTY: u32 = 1;
    pub const ONE_BY_ONE: u32 = 2;
    pub const ONE_FRAME: u32 = 4;
    /// Empty command buffers left uncommitted under [`SKIP_EMPTY`].
    pub static SKIPPED: AtomicU32 = AtomicU32::new(0);
    /// How many more failed command buffers to describe in the log.
    pub static FAILED_SAMPLE_BUDGET: AtomicU32 = AtomicU32::new(12);

    /// Command buffers committed and not yet completed, and the most at once.
    pub static IN_FLIGHT: AtomicU32 = AtomicU32::new(0);
    pub static IN_FLIGHT_MAX: AtomicU32 = AtomicU32::new(0);

    pub(super) fn note_committed() {
        use core::sync::atomic::Ordering;
        let now = IN_FLIGHT.fetch_add(1, Ordering::Relaxed) + 1;
        IN_FLIGHT_MAX.fetch_max(now, Ordering::Relaxed);
    }

    /// Command buffers that completed without error, and labels of the first few
    /// after the first failure (to compare with the failed ones).
    pub static GPU_OK: AtomicU32 = AtomicU32::new(0);
    static OK_SAMPLED: AtomicU32 = AtomicU32::new(0);
    pub static FAILED_SAMPLES: std::sync::Mutex<alloc::vec::Vec<String>> =
        std::sync::Mutex::new(alloc::vec::Vec::new());
    pub static OK_SAMPLES: std::sync::Mutex<alloc::vec::Vec<String>> =
        std::sync::Mutex::new(alloc::vec::Vec::new());

    fn sample(samples: &std::sync::Mutex<alloc::vec::Vec<String>>, line: String) {
        if let Ok(mut samples) = samples.lock() {
            if samples.len() < 64 {
                samples.push(line);
            }
        }
    }

    pub(super) fn note_succeeded(
        buffer: &objc2::runtime::ProtocolObject<dyn objc2_metal::MTLCommandBuffer>,
    ) {
        use core::sync::atomic::Ordering;
        GPU_OK.fetch_add(1, Ordering::Relaxed);
        if GPU_ERRORS.load(Ordering::Relaxed) == 0 || OK_SAMPLED.fetch_add(1, Ordering::Relaxed) >= 16
        {
            return;
        }
        let label: Option<objc2::rc::Retained<objc2_foundation::NSString>> =
            unsafe { objc2::msg_send![buffer, label] };
        sample(
            &OK_SAMPLES,
            label.map_or_else(|| "none".to_owned(), |s| s.to_string()),
        );
    }

    /// What one command buffer holds: its passes (label, first attachment's size
    /// and sample count), draws and the primitives they ask for, copies, compute
    /// dispatches and resources made resident through argument buffers.
    #[derive(Default)]
    pub(super) struct Contents {
        /// An encoder (render, compute, blit, acceleration structure) was opened.
        pub(super) encoded: bool,
        passes: alloc::vec::Vec<String>,
        pass_count: u32,
        draws: u32,
        indirect_draws: u32,
        elements: u64,
        largest_draw: u64,
        copies: u32,
        copy_bytes: u64,
        dispatches: u32,
        resident: u32,
        bind_groups: u32,
        bound: u32,
    }

    impl Contents {
        pub(super) fn pass(&mut self, describe: impl FnOnce() -> String) {
            self.pass_count += 1;
            if self.passes.len() < 10 {
                self.passes.push(describe());
            }
        }
        pub(super) fn draw(&mut self, elements: u32, instances: u32) {
            let total = u64::from(elements) * u64::from(instances);
            self.draws += 1;
            self.elements += total;
            self.largest_draw = self.largest_draw.max(total);
        }
        pub(super) fn indirect_draws(&mut self, count: u32) {
            self.indirect_draws += count;
        }
        pub(super) fn copy(&mut self, bytes: u64) {
            self.copies += 1;
            self.copy_bytes += bytes;
        }
        pub(super) fn dispatch(&mut self) {
            self.dispatches += 1;
        }
        pub(super) fn resident(&mut self, count: usize) {
            self.resident += count as u32;
        }
        pub(super) fn bound(&mut self, count: usize) {
            self.bind_groups += 1;
            self.bound += count as u32;
        }
        /// The label for the finished command buffer, starting with its own; resets.
        pub(super) fn finish(&mut self, label: &str) -> String {
            let more = self.pass_count as usize - self.passes.len();
            let more = if more > 0 {
                alloc::format!(" +{more} more")
            } else {
                String::new()
            };
            let text = alloc::format!(
                "{label} | {} pass(es) [{}{more}] draws {} (+{} indirect) elements {} largest {} | copies {} ({} KiB) | dispatches {} | argument-buffer resources {} | bind groups {} binding {} buffers/textures",
                self.pass_count,
                self.passes.join(", "),
                self.draws,
                self.indirect_draws,
                self.elements,
                self.largest_draw,
                self.copies,
                self.copy_bytes >> 10,
                self.dispatches,
                self.resident,
                self.bind_groups,
                self.bound,
            );
            *self = Self::default();
            text
        }
    }

    /// The last failed command buffer's label and Metal's description of the error.
    pub static LAST_GPU_ERROR_TEXT: std::sync::Mutex<Option<String>> =
        std::sync::Mutex::new(None);

    /// What the device holds (`currentAllocatedSize`) after the latest allocation,
    /// its peak, and the device's `recommendedMaxWorkingSetSize`, in bytes.
    pub static GPU_ALLOCATED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    pub static GPU_ALLOCATED_PEAK: core::sync::atomic::AtomicU64 =
        core::sync::atomic::AtomicU64::new(0);
    pub static GPU_WORKING_SET: core::sync::atomic::AtomicU64 =
        core::sync::atomic::AtomicU64::new(0);
    pub static GPU_ALLOCATED_AT_FIRST_ERROR: core::sync::atomic::AtomicU64 =
        core::sync::atomic::AtomicU64::new(0);
    /// Resources of at least [`BIG_ALLOCATION`] bytes, described for the log; drained
    /// by whoever logs them, and capped so nothing grows without one.
    pub static BIG_ALLOCATIONS: std::sync::Mutex<alloc::vec::Vec<String>> =
        std::sync::Mutex::new(alloc::vec::Vec::new());
    pub const BIG_ALLOCATION: u64 = 16 << 20;

    pub(super) fn note_allocation<R: objc2::Message + ?Sized>(
        device: &objc2::runtime::ProtocolObject<dyn objc2_metal::MTLDevice>,
        resource: &R,
        describe: impl FnOnce() -> String,
    ) {
        use core::sync::atomic::Ordering;
        // Untyped: these properties need features this build leaves off.
        let bytes: usize = unsafe { objc2::msg_send![resource, allocatedSize] };
        let held: usize = unsafe { objc2::msg_send![device, currentAllocatedSize] };
        let held = held as u64;
        GPU_ALLOCATED.store(held, Ordering::Relaxed);
        GPU_ALLOCATED_PEAK.fetch_max(held, Ordering::Relaxed);
        if GPU_WORKING_SET.load(Ordering::Relaxed) == 0 {
            let working: u64 = unsafe { objc2::msg_send![device, recommendedMaxWorkingSetSize] };
            GPU_WORKING_SET.store(working, Ordering::Relaxed);
        }
        if bytes as u64 >= BIG_ALLOCATION {
            if let Ok(mut big) = BIG_ALLOCATIONS.lock() {
                if big.len() < 256 {
                    big.push(alloc::format!("{} ({} MiB)", describe(), bytes >> 20));
                }
            }
        }
    }

    /// Bind groups whose binding arrays go through an argument buffer: every
    /// command buffer that binds one makes all of its resources resident.
    pub static BINDLESS_GROUPS: AtomicU32 = AtomicU32::new(0);
    /// The most resources any one such group made resident, and its label.
    pub static BINDLESS_MAX: core::sync::atomic::AtomicUsize =
        core::sync::atomic::AtomicUsize::new(0);
    pub static BINDLESS_MAX_LABEL: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

    pub(super) fn note_bindless_group(label: Option<&str>, resources: usize) {
        use core::sync::atomic::Ordering;
        BINDLESS_GROUPS.fetch_add(1, Ordering::Relaxed);
        if BINDLESS_MAX.fetch_max(resources, Ordering::Relaxed) < resources {
            if let Ok(mut last) = BINDLESS_MAX_LABEL.lock() {
                *last = Some(label.unwrap_or("").to_owned());
            }
        }
    }
}

pub struct Instance {}

impl Instance {
    pub fn create_surface_from_layer(&self, layer: &CAMetalLayer) -> Surface {
        Surface::from_layer(layer)
    }
}

impl crate::Instance for Instance {
    type A = Api;

    unsafe fn init(_desc: &crate::InstanceDescriptor<'_>) -> Result<Self, crate::InstanceError> {
        profiling::scope!("Init Metal Backend");
        // We do not enable metal validation based on the validation flags as it affects the entire
        // process. Instead, we enable the validation inside the test harness itself in tests/src/native.rs.
        Ok(Instance {})
    }

    unsafe fn create_surface(
        &self,
        display_handle: raw_window_handle::RawDisplayHandle,
        window_handle: raw_window_handle::RawWindowHandle,
    ) -> Result<Surface, crate::InstanceError> {
        let layer = match (display_handle, window_handle) {
            (
                raw_window_handle::RawDisplayHandle::AppKit(_),
                raw_window_handle::RawWindowHandle::AppKit(handle),
            ) => unsafe { raw_window_metal::Layer::from_ns_view(handle.ns_view) },
            (
                raw_window_handle::RawDisplayHandle::UiKit(_),
                raw_window_handle::RawWindowHandle::UiKit(handle),
            ) => unsafe { raw_window_metal::Layer::from_ui_view(handle.ui_view) },
            _ => {
                return Err(crate::InstanceError::new(format!(
                    "window handle {window_handle:?} is not a Metal-compatible handle"
                )))
            }
        };

        // SAFETY: The layer is an initialized instance of `CAMetalLayer`, and
        // we transfer the retain count to `Retained` using `into_raw`.
        let layer = unsafe {
            Retained::from_raw(layer.into_raw().cast::<CAMetalLayer>().as_ptr()).unwrap()
        };

        Ok(Surface::new(layer))
    }

    unsafe fn enumerate_adapters(
        &self,
        _surface_hint: Option<&Surface>,
    ) -> Vec<crate::ExposedAdapter<Api>> {
        let devices = objc2_metal::MTLCopyAllDevices();
        let mut adapters: Vec<crate::ExposedAdapter<Api>> =
            devices.into_iter().map(AdapterShared::expose).collect();
        adapters.sort_by_key(|ad| {
            (
                ad.adapter.shared.private_caps.low_power,
                ad.adapter.shared.private_caps.headless,
            )
        });
        adapters
    }
}

bitflags!(
    /// Similar to `MTLCounterSamplingPoint`, but a bit higher abstracted for our purposes.
    #[derive(Debug, Copy, Clone)]
    pub struct TimestampQuerySupport: u32 {
        /// On creating Metal encoders.
        const STAGE_BOUNDARIES = 1 << 1;
        /// Within existing draw encoders.
        const ON_RENDER_ENCODER = Self::STAGE_BOUNDARIES.bits() | (1 << 2);
        /// Within existing dispatch encoders.
        const ON_COMPUTE_ENCODER = Self::STAGE_BOUNDARIES.bits() | (1 << 3);
        /// Within existing blit encoders.
        const ON_BLIT_ENCODER = Self::STAGE_BOUNDARIES.bits() | (1 << 4);

        /// Within any wgpu render/compute pass.
        const INSIDE_WGPU_PASSES = Self::ON_RENDER_ENCODER.bits() | Self::ON_COMPUTE_ENCODER.bits();
    }
);

#[allow(dead_code)]
struct CapabilitiesQuery {
    msl_version: MTLLanguageVersion,
    fragment_rw_storage: bool,
    read_write_texture_tier: MTLReadWriteTextureTier,
    msaa_desktop: bool,
    msaa_apple3: bool,
    msaa_apple7: bool,
    resource_heaps: bool,
    argument_buffers: Option<MTLArgumentBuffersTier>,
    mutable_comparison_samplers: bool,
    sampler_clamp_to_border: bool,
    indirect_draw_dispatch: bool,
    base_vertex_first_instance_drawing: bool,
    dual_source_blending: bool,
    low_power: bool,
    headless: bool,
    layered_rendering: bool,
    function_specialization: bool,
    depth_clip_mode: bool,
    texture_cube_array: bool,
    supports_float_filtering: bool,
    format_depth24_stencil8: bool,
    format_depth32_stencil8_filter: bool,
    format_depth32_stencil8_none: bool,
    format_min_srgb_channels: u8,
    format_b5: bool,
    format_bc: bool,
    format_eac_etc: bool,
    format_astc: bool,
    format_astc_hdr: bool,
    format_astc_3d: bool,
    format_any8_unorm_srgb_all: bool,
    format_any8_unorm_srgb_no_write: bool,
    format_any8_snorm_all: bool,
    format_r16_norm_all: bool,
    format_r32_all: bool,
    format_r32_no_write: bool,
    format_r32float_no_write_no_filter: bool,
    format_r32float_no_filter: bool,
    format_r32float_all: bool,
    format_rgba8_srgb_all: bool,
    format_rgba8_srgb_no_write: bool,
    format_rgb10a2_unorm_all: bool,
    format_rgb10a2_unorm_no_write: bool,
    format_rgb10a2_uint_write: bool,
    format_rg11b10_all: bool,
    format_rg11b10_no_write: bool,
    format_rgb9e5_all: bool,
    format_rgb9e5_no_write: bool,
    format_rgb9e5_filter_only: bool,
    format_rg32_color: bool,
    format_rg32_color_write: bool,
    format_rg32float_all: bool,
    format_rg32float_color_blend: bool,
    format_rg32float_no_filter: bool,
    format_rgba32int_color: bool,
    format_rgba32int_color_write: bool,
    format_rgba32float_color: bool,
    format_rgba32float_color_write: bool,
    format_rgba32float_all: bool,
    format_depth16unorm: bool,
    format_depth16unorm_filter: bool,
    format_depth32float_filter: bool,
    format_depth32float_none: bool,
    format_bgr10a2_all: bool,
    format_bgr10a2_no_write: bool,
    max_buffers_per_stage: ResourceIndex,
    max_vertex_buffers: ResourceIndex,
    max_textures_per_stage: ResourceIndex,
    max_samplers_per_stage: ResourceIndex,
    max_binding_array_elements: ResourceIndex,
    max_sampler_binding_array_elements: ResourceIndex,
    buffer_alignment: u64,

    /// Platform-reported maximum buffer size
    ///
    /// This value is clamped to `u32::MAX` for `wgt::Limits`, so you probably
    /// shouldn't be looking at this copy.
    max_buffer_size: u64,
    max_texture_size: u64,
    max_texture_3d_size: u64,
    max_texture_layers: u64,
    max_fragment_input_components: u64,
    max_color_render_targets: u8,
    max_color_attachment_bytes_per_sample: u8,
    max_varying_components: u32,
    max_threads_per_group: u32,
    max_total_threadgroup_memory: u32,
    sample_count_mask: crate::TextureFormatCapabilities,
    supports_debug_markers: bool,
    supports_binary_archives: bool,
    supports_arrays_of_textures: bool,
    supports_arrays_of_textures_write: bool,
    supports_depth_clip_control: bool,
    supports_shader_primitive_index: bool,
    has_unified_memory: Option<bool>,
    timestamp_query_support: TimestampQuerySupport,
    supports_simd_scoped_operations: bool,
    supports_cooperative_matrix: bool,
    int64: bool,
    int64_atomics_min_max: bool,
    int64_atomics: bool,
    float_atomics: bool,
    mesh_shaders: bool,
    max_mesh_task_workgroup_count: u32,
    max_task_payload_size: u32,
    supported_vertex_amplification_factor: u32,
    shader_barycentrics: bool,
    supports_memoryless_storage: bool,
    supports_raytracing: bool,
}

#[derive(Debug)]
struct PrivateCapabilities {
    msl_version: MTLLanguageVersion,
    low_power: bool,
    headless: bool,
    has_unified_memory: Option<bool>,
    timestamp_query_support: TimestampQuerySupport,
    supports_memoryless_storage: bool,
    mesh_shaders: bool,
    max_buffers_per_stage: ResourceIndex,
    max_vertex_buffers: ResourceIndex,
    max_textures_per_stage: ResourceIndex,
    max_samplers_per_stage: ResourceIndex,
}

#[derive(Debug)]
struct PrivateTextureFormatCapabilities {
    read_write_texture_tier: MTLReadWriteTextureTier,
    sample_count_mask: crate::TextureFormatCapabilities,
    int64_atomics: bool,
    msaa_desktop: bool,
    msaa_apple3: bool,
    msaa_apple7: bool,
    format_r32float_all: bool,
    format_rgba8_srgb_all: bool,
    format_rgb10a2_uint_write: bool,
    format_rgb10a2_unorm_all: bool,
    format_rg11b10_all: bool,
    format_rg32float_all: bool,
    format_rgba32float_all: bool,
    format_depth16unorm: bool,
    format_depth16unorm_filter: bool,
    format_depth32float_filter: bool,
    format_depth24_stencil8: bool,
    format_bc: bool,
    format_eac_etc: bool,
    format_astc: bool,
    format_astc_hdr: bool,
}

#[derive(Clone, Debug)]
struct PrivateDisabilities {
    /// Near depth is not respected properly on some Intel GPUs.
    broken_viewport_near_depth: bool,
    /// Multi-target clears don't appear to work properly on Intel GPUs.
    #[allow(dead_code)]
    broken_layered_clear_image: bool,
}

#[derive(Debug)]
struct Settings {
    retain_command_buffer_references: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            retain_command_buffer_references: true,
        }
    }
}

struct AdapterShared {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    disabilities: PrivateDisabilities,
    private_caps: PrivateCapabilities,
    private_texture_format_caps: PrivateTextureFormatCapabilities,
    settings: Settings,
    presentation_timer: time::PresentationTimer,
}

unsafe impl Send for AdapterShared {}
unsafe impl Sync for AdapterShared {}

impl AdapterShared {
    fn new(
        device: Retained<ProtocolObject<dyn MTLDevice>>,
        capabilities_query: &CapabilitiesQuery,
    ) -> Self {
        let private_caps = capabilities_query.private_capabilities();
        let private_texture_format_caps = capabilities_query.private_texture_format_capabilities();
        log::debug!("{private_caps:#?}");
        log::debug!("{private_texture_format_caps:#?}");

        Self {
            disabilities: PrivateDisabilities::new(&device),
            private_caps,
            private_texture_format_caps,
            device,
            settings: Settings::default(),
            presentation_timer: time::PresentationTimer::new(),
        }
    }

    fn expose(device: Retained<ProtocolObject<dyn MTLDevice>>) -> crate::ExposedAdapter<Api> {
        let name = device.name().to_string();
        let capabilities_query = CapabilitiesQuery::new(&device);
        let shared = AdapterShared::new(device, &capabilities_query);
        let features = capabilities_query.features();
        let capabilities = capabilities_query.capabilities();
        crate::ExposedAdapter {
            info: wgt::AdapterInfo {
                name,
                vendor: 0,
                device: 0,
                device_type: shared.private_caps.device_type(),
                device_pci_bus_id: String::new(),
                driver: String::new(),
                driver_info: String::new(),
                backend: wgt::Backend::Metal,
                // These are hardcoded based on typical values for Metal devices
                //
                // See <https://github.com/gpuweb/gpuweb/blob/main/proposals/subgroups.md#adapter-info>
                // for more information.
                subgroup_min_size: 4,
                subgroup_max_size: 64,
                transient_saves_memory: shared.private_caps.supports_memoryless_storage,
            },
            features,
            capabilities,
            adapter: Adapter::new(Arc::new(shared)),
        }
    }
}

pub struct Adapter {
    shared: Arc<AdapterShared>,
}

pub struct Queue {
    shared: Arc<QueueShared>,
    timestamp_period: f32,
}

unsafe impl Send for Queue {}
unsafe impl Sync for Queue {}

impl Queue {
    pub unsafe fn queue_from_raw(
        raw: Retained<ProtocolObject<dyn MTLCommandQueue>>,
        timestamp_period: f32,
    ) -> Self {
        Self {
            shared: Arc::new(QueueShared {
                raw,
                command_buffer_created_not_submitted: atomic::AtomicUsize::new(0),
            }),
            timestamp_period,
        }
    }

    pub fn as_raw(&self) -> &ProtocolObject<dyn MTLCommandQueue> {
        &self.shared.raw
    }
}

#[derive(Debug)]
pub struct QueueShared {
    raw: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    // Tracks command buffers created via `CommandEncoder::begin_encoding` that
    // have not yet been submitted or discarded. Used to proactively fail
    // before hitting Metal's `maxCommandBufferCount`.
    //
    // (In a few places we call `.commandBuffer{,WithUnretainedReferences}` directly
    // to create command buffers for internal purposes. In those cases we always
    // commit the buffer immediately, so we don't adjust the counter for them.)
    command_buffer_created_not_submitted: atomic::AtomicUsize,
}

pub struct Device {
    shared: Arc<AdapterShared>,
    features: wgt::Features,
    counters: Arc<wgt::HalCounters>,
}

pub struct Surface {
    render_layer: Mutex<Retained<CAMetalLayer>>,
    swapchain_format: RwLock<Option<wgt::TextureFormat>>,
    extent: RwLock<wgt::Extent3d>,
}

unsafe impl Send for Surface {}
unsafe impl Sync for Surface {}

#[derive(Debug)]
pub struct SurfaceTexture {
    texture: Texture,
    // Useful for UI-intensive applications that are sensitive to
    // window resizing.
    drawable: Retained<ProtocolObject<dyn MTLDrawable>>,
    present_with_transaction: bool,
}

impl crate::DynSurfaceTexture for SurfaceTexture {}

impl core::borrow::Borrow<Texture> for SurfaceTexture {
    fn borrow(&self) -> &Texture {
        &self.texture
    }
}

impl core::borrow::Borrow<dyn crate::DynTexture> for SurfaceTexture {
    fn borrow(&self) -> &dyn crate::DynTexture {
        &self.texture
    }
}

unsafe impl Send for SurfaceTexture {}
unsafe impl Sync for SurfaceTexture {}

impl crate::Queue for Queue {
    type A = Api;

    unsafe fn submit(
        &self,
        command_buffers: &[&CommandBuffer],
        _surface_textures: &[&SurfaceTexture],
        (signal_fence, signal_value): (&mut Fence, crate::FenceValue),
    ) -> Result<(), crate::DeviceError> {
        autoreleasepool(|_| {
            let extra_command_buffer = {
                let completed_value = Arc::clone(&signal_fence.completed_value);
                let block = block2::RcBlock::new(move |_cmd_buf| {
                    completed_value.store(signal_value, atomic::Ordering::Release);
                });

                let raw = match command_buffers.last() {
                    Some(&cmd_buf) => cmd_buf.raw.clone(),
                    None => {
                        // We do not bother adjusting `command_buffer_created_not_submitted`
                        // because we immediately commit this buffer.
                        self.shared
                            .raw
                            .commandBufferWithUnretainedReferences()
                            .unwrap()
                    }
                };
                // IW4 Pocket: keep what the label says the buffer holds.
                match raw.label() {
                    Some(held) if command_buffers.last().is_some() => raw.setLabel(Some(
                        &objc2_foundation::NSString::from_str(&format!(
                            "(wgpu internal) Signal + {held}"
                        )),
                    )),
                    _ => raw.setLabel(Some(ns_string!("(wgpu internal) Signal"))),
                }
                unsafe { raw.addCompletedHandler(block2::RcBlock::as_ptr(&block)) };

                signal_fence.maintain();
                signal_fence
                    .pending_command_buffers
                    .push((signal_value, raw.clone()));

                if let Some(shared_event) = &signal_fence.shared_event {
                    raw.encodeSignalEvent_value(shared_event.as_ref(), signal_value);
                }
                // only return an extra one if it's extra
                match command_buffers.last() {
                    Some(_) => None,
                    None => Some(raw),
                }
            };

            let mode = pacing::MODE.load(atomic::Ordering::Relaxed);
            for (i, cmd_buffer) in command_buffers.iter().enumerate() {
                // IW4 Pocket: see `pacing::SKIP_EMPTY`. An uncommitted buffer gives
                // back its slot when it is dropped.
                if mode & pacing::SKIP_EMPTY != 0
                    && cmd_buffer.empty
                    && i + 1 < command_buffers.len()
                {
                    pacing::SKIPPED.fetch_add(1, atomic::Ordering::Relaxed);
                    continue;
                }
                // IW4 Pocket: count command buffers Metal fails (with the timing hooks on).
                // A failed frame's drawable is still presented by the present command
                // buffer, showing whatever it last held.
                if pacing::HOOKS.get().is_some() {
                    let block = block2::RcBlock::new(
                        |buffer: NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                            pacing::note_completed(unsafe { buffer.as_ref() });
                        },
                    );
                    unsafe {
                        cmd_buffer
                            .raw
                            .addCompletedHandler(block2::RcBlock::as_ptr(&block))
                    };
                    pacing::note_committed();
                }
                cmd_buffer.raw.commit();
                if mode & pacing::ONE_BY_ONE != 0 {
                    cmd_buffer.raw.waitUntilScheduled();
                }
                // One command buffer per `end_encoding` call moves from the
                // "created but not yet submitted" bucket into the submitted
                // set, so update the counter.
                let previous = self
                    .shared
                    .command_buffer_created_not_submitted
                    .fetch_sub(1, atomic::Ordering::AcqRel);
                debug_assert!(previous > 0);
            }

            if let Some(raw) = extra_command_buffer {
                raw.commit();
            }
        });
        Ok(())
    }
    unsafe fn present(
        &self,
        _surface: &Surface,
        texture: SurfaceTexture,
    ) -> Result<(), crate::SurfaceError> {
        autoreleasepool(|_| {
            // We do not bother adjusting `command_buffer_created_not_submitted`
            // because we immediately commit this buffer.
            let command_buffer = self.shared.raw.commandBuffer().unwrap();
            command_buffer.setLabel(Some(ns_string!("(wgpu internal) Present")));

            // IW4 Pocket: optional timing hooks and a minimum on-screen
            // duration (see `pacing`). Neither is set by default, which keeps
            // the upstream `presentDrawable:` path.
            let hooks = pacing::HOOKS.get();
            let mut submitted = None;

            // https://developer.apple.com/documentation/quartzcore/cametallayer/1478157-presentswithtransaction?language=objc
            if !texture.present_with_transaction {
                if let Some(hooks) = hooks {
                    let id = texture.drawable.drawableID() as u64;
                    let presented = hooks.presented;
                    let block = block2::RcBlock::new(
                        move |drawable: NonNull<ProtocolObject<dyn MTLDrawable>>| {
                            // `presentedTime` is typed only behind objc2-metal's
                            // "objc2-core-foundation" feature, which is off.
                            let at: f64 =
                                unsafe { objc2::msg_send![drawable.as_ref(), presentedTime] };
                            presented(id, at);
                        },
                    );
                    unsafe {
                        texture
                            .drawable
                            .addPresentedHandler(block2::RcBlock::as_ptr(&block))
                    };
                    submitted = Some((hooks.submitted, id, std::time::Instant::now()));
                }
                let min_us = pacing::MIN_PRESENT_US.load(atomic::Ordering::Relaxed);
                if min_us == 0 {
                    command_buffer.presentDrawable(&texture.drawable);
                } else {
                    // Untyped for the same reason: the typed binding takes a
                    // CFTimeInterval, also behind that feature.
                    let seconds = f64::from(min_us) * 1e-6;
                    let _: () = unsafe {
                        objc2::msg_send![
                            &*command_buffer,
                            presentDrawable: &*texture.drawable,
                            afterMinimumDuration: seconds
                        ]
                    };
                }
            }

            command_buffer.commit();
            if pacing::MODE.load(atomic::Ordering::Relaxed) & pacing::ONE_FRAME != 0 {
                command_buffer.waitUntilCompleted();
            }

            if let Some((hook, id, started)) = submitted {
                hook(id, started.elapsed().as_nanos() as u64);
            }

            if texture.present_with_transaction {
                command_buffer.waitUntilScheduled();
                texture.drawable.present();
            }
        });
        Ok(())
    }

    unsafe fn get_timestamp_period(&self) -> f32 {
        self.timestamp_period
    }
}

#[derive(Debug)]
pub struct Buffer {
    raw: Retained<ProtocolObject<dyn MTLBuffer>>,
    size: wgt::BufferAddress,
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

impl crate::DynBuffer for Buffer {}

impl Buffer {
    fn as_raw(&self) -> NonNull<ProtocolObject<dyn MTLBuffer>> {
        unsafe { NonNull::new_unchecked(Retained::as_ptr(&self.raw) as *mut _) }
    }
}

impl crate::BufferBinding<'_, Buffer> {
    fn resolve_size(&self) -> wgt::BufferAddress {
        match self.size {
            Some(size) => size.get(),
            None => self.buffer.size - self.offset,
        }
    }
}

#[derive(Debug)]
pub struct Texture {
    raw: Retained<ProtocolObject<dyn MTLTexture>>,
    format: wgt::TextureFormat,
    raw_type: MTLTextureType,
    array_layers: u32,
    mip_levels: u32,
    copy_size: crate::CopyExtent,
}

impl Texture {
    pub fn raw_handle(&self) -> &ProtocolObject<dyn MTLTexture> {
        &self.raw
    }
}

impl crate::DynTexture for Texture {}

unsafe impl Send for Texture {}
unsafe impl Sync for Texture {}

#[derive(Debug)]
pub struct TextureView {
    raw: Retained<ProtocolObject<dyn MTLTexture>>,
    aspects: crate::FormatAspects,
}

impl crate::DynTextureView for TextureView {}

unsafe impl Send for TextureView {}
unsafe impl Sync for TextureView {}

impl TextureView {
    fn as_raw(&self) -> NonNull<ProtocolObject<dyn MTLTexture>> {
        unsafe { NonNull::new_unchecked(Retained::as_ptr(&self.raw) as *mut _) }
    }
}

#[derive(Debug)]
pub struct Sampler {
    raw: Retained<ProtocolObject<dyn MTLSamplerState>>,
}

impl crate::DynSampler for Sampler {}

unsafe impl Send for Sampler {}
unsafe impl Sync for Sampler {}

impl Sampler {
    fn as_raw(&self) -> NonNull<ProtocolObject<dyn MTLSamplerState>> {
        unsafe { NonNull::new_unchecked(Retained::as_ptr(&self.raw) as *mut _) }
    }
}

#[derive(Debug)]
pub struct BindGroupLayout {
    /// Sorted list of BGL entries.
    entries: Arc<[wgt::BindGroupLayoutEntry]>,
}

impl crate::DynBindGroupLayout for BindGroupLayout {}

#[derive(Clone, Debug, Default)]
struct ResourceData<T> {
    buffers: T,
    textures: T,
    samplers: T,
}

#[derive(Clone, Debug, Default)]
struct MultiStageData<T> {
    vs: T,
    fs: T,
    cs: T,
    ts: T,
    ms: T,
}

const NAGA_STAGES: MultiStageData<naga::ShaderStage> = MultiStageData {
    vs: naga::ShaderStage::Vertex,
    fs: naga::ShaderStage::Fragment,
    cs: naga::ShaderStage::Compute,
    ts: naga::ShaderStage::Task,
    ms: naga::ShaderStage::Mesh,
};

impl<T> ops::Index<naga::ShaderStage> for MultiStageData<T> {
    type Output = T;
    fn index(&self, stage: naga::ShaderStage) -> &T {
        match stage {
            naga::ShaderStage::Vertex => &self.vs,
            naga::ShaderStage::Fragment => &self.fs,
            naga::ShaderStage::Compute => &self.cs,
            naga::ShaderStage::Task => &self.ts,
            naga::ShaderStage::Mesh => &self.ms,
            naga::ShaderStage::RayGeneration
            | naga::ShaderStage::AnyHit
            | naga::ShaderStage::ClosestHit
            | naga::ShaderStage::Miss => unimplemented!(),
        }
    }
}

impl<T> MultiStageData<T> {
    fn map_ref<Y>(&self, fun: impl Fn(&T) -> Y) -> MultiStageData<Y> {
        MultiStageData {
            vs: fun(&self.vs),
            fs: fun(&self.fs),
            cs: fun(&self.cs),
            ts: fun(&self.ts),
            ms: fun(&self.ms),
        }
    }
    fn map<Y>(self, fun: impl Fn(T) -> Y) -> MultiStageData<Y> {
        MultiStageData {
            vs: fun(self.vs),
            fs: fun(self.fs),
            cs: fun(self.cs),
            ts: fun(self.ts),
            ms: fun(self.ms),
        }
    }
    fn iter<'a>(&'a self) -> impl Iterator<Item = &'a T> {
        iter::once(&self.vs)
            .chain(iter::once(&self.fs))
            .chain(iter::once(&self.cs))
            .chain(iter::once(&self.ts))
            .chain(iter::once(&self.ms))
    }
    fn iter_mut<'a>(&'a mut self) -> impl Iterator<Item = &'a mut T> {
        iter::once(&mut self.vs)
            .chain(iter::once(&mut self.fs))
            .chain(iter::once(&mut self.cs))
            .chain(iter::once(&mut self.ts))
            .chain(iter::once(&mut self.ms))
    }
}

type MultiStageResourceCounters = MultiStageData<ResourceData<ResourceIndex>>;
type MultiStageResources = MultiStageData<naga::back::msl::EntryPointResources>;

#[derive(Debug)]
struct BindGroupLayoutInfo {
    base_resource_indices: MultiStageResourceCounters,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct ImmediateDataInfo {
    count: u32,
    buffer_index: ResourceIndex,
}

#[derive(Debug)]
pub struct PipelineLayout {
    bind_group_infos: [Option<BindGroupLayoutInfo>; crate::MAX_BIND_GROUPS],
    immediates_infos: MultiStageData<Option<ImmediateDataInfo>>,
    total_counters: MultiStageResourceCounters,
    total_immediates: u32,
    per_stage_map: MultiStageResources,
}

impl crate::DynPipelineLayout for PipelineLayout {}

#[derive(Debug)]
enum BufferLikeResource {
    Buffer {
        ptr: NonNull<ProtocolObject<dyn MTLBuffer>>,
        offset: wgt::BufferAddress,
        dynamic_index: Option<u32>,

        /// The buffer's size, if it is a [`Storage`] binding. Otherwise `None`.
        ///
        /// Buffers with the [`wgt::BufferBindingType::Storage`] binding type can
        /// hold WGSL runtime-sized arrays. When one does, we must pass its size to
        /// shader entry points to implement bounds checks and WGSL's `arrayLength`
        /// function. See `device::CompiledShader::sized_bindings` for details.
        ///
        /// [`Storage`]: wgt::BufferBindingType::Storage
        binding_size: Option<wgt::BufferSize>,

        binding_location: u32,
    },
    AccelerationStructure(NonNull<ProtocolObject<dyn MTLAccelerationStructure>>),
}

#[derive(Debug)]
struct UseResourceInfo {
    uses: MTLResourceUsage,
    stages: MTLRenderStages,
    visible_in_compute: bool,
}

impl Default for UseResourceInfo {
    fn default() -> Self {
        Self {
            uses: MTLResourceUsage::empty(),
            stages: MTLRenderStages::empty(),
            visible_in_compute: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct BindGroup {
    counters: MultiStageResourceCounters,
    buffers: Vec<BufferLikeResource>,
    samplers: Vec<NonNull<ProtocolObject<dyn MTLSamplerState>>>,
    textures: Vec<NonNull<ProtocolObject<dyn MTLTexture>>>,

    argument_buffers: Vec<Retained<ProtocolObject<dyn MTLBuffer>>>,
    resources_to_use: HashMap<NonNull<ProtocolObject<dyn MTLResource>>, UseResourceInfo>,
}

impl crate::DynBindGroup for BindGroup {}

unsafe impl Send for BindGroup {}
unsafe impl Sync for BindGroup {}

#[derive(Debug)]
pub enum ShaderModuleSource {
    Naga(crate::NagaShader),
    Passthrough(PassthroughShader),
}

#[derive(Debug)]
pub struct PassthroughShader {
    pub library: Retained<ProtocolObject<dyn MTLLibrary>>,
    pub num_workgroups: (u32, u32, u32),
}

unsafe impl Send for PassthroughShader {}
unsafe impl Sync for PassthroughShader {}

#[derive(Debug)]
pub struct ShaderModule {
    source: ShaderModuleSource,
    bounds_checks: wgt::ShaderRuntimeChecks,
}

impl crate::DynShaderModule for ShaderModule {}

#[derive(Debug)]
struct PipelineStageInfo {
    #[allow(dead_code)]
    library: Option<Retained<ProtocolObject<dyn MTLLibrary>>>,
    immediates: Option<ImmediateDataInfo>,

    /// The buffer argument table index at which we pass runtime-sized arrays' buffer sizes.
    ///
    /// See `device::CompiledShader::sized_bindings` for more details.
    sizes_slot: Option<naga::back::msl::Slot>,

    /// Bindings of all WGSL `storage` globals that contain runtime-sized arrays.
    ///
    /// See `device::CompiledShader::sized_bindings` for more details.
    sized_bindings: Vec<naga::ResourceBinding>,

    /// Info on all bound vertex buffers.
    vertex_buffer_mappings: Vec<naga::back::msl::VertexBufferMapping>,

    /// The workgroup size for compute, task or mesh stages
    raw_wg_size: MTLSize,

    /// The workgroup memory sizes for compute task or mesh stages
    work_group_memory_sizes: Vec<u32>,
}

// TODO(madsmtm): Derive this when a release with
// https://github.com/madsmtm/objc2/issues/804 is available (likely 0.4).
impl Default for PipelineStageInfo {
    fn default() -> Self {
        Self {
            library: Default::default(),
            immediates: Default::default(),
            sizes_slot: Default::default(),
            sized_bindings: Default::default(),
            vertex_buffer_mappings: Default::default(),
            raw_wg_size: MTLSize {
                width: 0,
                height: 0,
                depth: 0,
            },
            work_group_memory_sizes: Default::default(),
        }
    }
}

impl PipelineStageInfo {
    fn clear(&mut self) {
        self.immediates = None;
        self.sizes_slot = None;
        self.sized_bindings.clear();
        self.vertex_buffer_mappings.clear();
        self.library = None;
        self.work_group_memory_sizes.clear();
        self.raw_wg_size = MTLSize {
            width: 0,
            height: 0,
            depth: 0,
        };
    }

    fn assign_from(&mut self, other: &Self) {
        self.immediates = other.immediates;
        self.sizes_slot = other.sizes_slot;
        self.sized_bindings.clear();
        self.sized_bindings.extend_from_slice(&other.sized_bindings);
        self.vertex_buffer_mappings.clear();
        self.vertex_buffer_mappings
            .extend_from_slice(&other.vertex_buffer_mappings);
        self.library = Some(other.library.as_ref().unwrap().clone());
        self.raw_wg_size = other.raw_wg_size;
        self.work_group_memory_sizes.clear();
        self.work_group_memory_sizes
            .extend_from_slice(&other.work_group_memory_sizes);
    }
}

#[derive(Debug)]
pub struct RenderPipeline {
    raw: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    vs_info: Option<PipelineStageInfo>,
    fs_info: Option<PipelineStageInfo>,
    ts_info: Option<PipelineStageInfo>,
    ms_info: Option<PipelineStageInfo>,
    raw_primitive_type: MTLPrimitiveType,
    raw_triangle_fill_mode: MTLTriangleFillMode,
    raw_front_winding: MTLWinding,
    raw_cull_mode: MTLCullMode,
    raw_depth_clip_mode: Option<MTLDepthClipMode>,
    depth_stencil: Option<(
        Retained<ProtocolObject<dyn MTLDepthStencilState>>,
        wgt::DepthBiasState,
    )>,
}

unsafe impl Send for RenderPipeline {}
unsafe impl Sync for RenderPipeline {}

impl crate::DynRenderPipeline for RenderPipeline {}

#[derive(Debug)]
pub struct ComputePipeline {
    raw: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    cs_info: PipelineStageInfo,
}

unsafe impl Send for ComputePipeline {}
unsafe impl Sync for ComputePipeline {}

impl crate::DynComputePipeline for ComputePipeline {}

#[derive(Debug, Clone)]
pub struct QuerySet {
    raw_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    //Metal has a custom buffer for counters.
    counter_sample_buffer: Option<Retained<ProtocolObject<dyn MTLCounterSampleBuffer>>>,
    ty: wgt::QueryType,
}

impl crate::DynQuerySet for QuerySet {}

unsafe impl Send for QuerySet {}
unsafe impl Sync for QuerySet {}

#[derive(Debug)]
pub struct Fence {
    completed_value: Arc<atomic::AtomicU64>,
    /// The pending fence values have to be ascending.
    pending_command_buffers: Vec<(
        crate::FenceValue,
        Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    )>,
    shared_event: Option<Retained<ProtocolObject<dyn MTLSharedEvent>>>,
}

impl crate::DynFence for Fence {}

unsafe impl Send for Fence {}
unsafe impl Sync for Fence {}

impl Fence {
    fn get_latest(&self) -> crate::FenceValue {
        let mut max_value = self.completed_value.load(atomic::Ordering::Acquire);
        for &(value, ref cmd_buf) in self.pending_command_buffers.iter() {
            if cmd_buf.status() == MTLCommandBufferStatus::Completed {
                max_value = value;
            }
        }
        max_value
    }

    fn maintain(&mut self) {
        let latest = self.get_latest();
        self.pending_command_buffers
            .retain(|&(value, _)| value > latest);
    }

    pub fn raw_shared_event(&self) -> Option<&ProtocolObject<dyn MTLSharedEvent>> {
        self.shared_event.as_deref()
    }
}

struct IndexState {
    buffer_ptr: NonNull<ProtocolObject<dyn MTLBuffer>>,
    offset: wgt::BufferAddress,
    stride: wgt::BufferAddress,
    raw_type: MTLIndexType,
}

#[derive(Default)]
struct Temp {
    binding_sizes: Vec<u32>,
}

struct CommandState {
    blit: Option<Retained<ProtocolObject<dyn MTLBlitCommandEncoder>>>,
    acceleration_structure_builder:
        Option<Retained<ProtocolObject<dyn MTLAccelerationStructureCommandEncoder>>>,
    render: Option<Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>>,
    compute: Option<Retained<ProtocolObject<dyn MTLComputeCommandEncoder>>>,
    raw_primitive_type: MTLPrimitiveType,
    index: Option<IndexState>,
    stage_infos: MultiStageData<PipelineStageInfo>,

    /// Sizes of currently bound [`wgt::BufferBindingType::Storage`] buffers.
    ///
    /// Specifically:
    ///
    /// - The keys are [`ResourceBinding`] values (that is, the WGSL `@group`
    ///   and `@binding` attributes) for `var<storage>` global variables in the
    ///   current module that contain runtime-sized arrays.
    ///
    /// - The values are the actual sizes of the buffers currently bound to
    ///   provide those globals' contents, which are needed to implement bounds
    ///   checks and the WGSL `arrayLength` function.
    ///
    /// For each stage `S` in `stage_infos`, we consult this to find the sizes
    /// of the buffers listed in `stage_infos.S.sized_bindings`, which we must
    /// pass to the entry point.
    ///
    /// See `device::CompiledShader::sized_bindings` for more details.
    ///
    /// [`ResourceBinding`]: naga::ResourceBinding
    storage_buffer_length_map: FastHashMap<naga::ResourceBinding, wgt::BufferSize>,

    vertex_buffer_size_map: FastHashMap<u64, wgt::BufferSize>,

    immediates: Vec<u32>,

    /// Timer query that should be executed when the next pass starts.
    pending_timer_queries: Vec<(QuerySet, u32)>,
}

pub struct CommandEncoder {
    shared: Arc<AdapterShared>,
    queue_shared: Arc<QueueShared>,
    raw_cmd_buf: Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
    state: CommandState,
    temp: Temp,
    counters: Arc<wgt::HalCounters>,
    /// IW4 Pocket: what this command buffer holds, written into its label.
    contents: pacing::Contents,
}

impl fmt::Debug for CommandEncoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandEncoder")
            .field("raw_cmd_buf", &self.raw_cmd_buf)
            .finish()
    }
}

unsafe impl Send for CommandEncoder {}
unsafe impl Sync for CommandEncoder {}

#[derive(Debug)]
pub struct CommandBuffer {
    raw: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    queue_shared: Arc<QueueShared>,
    /// IW4 Pocket: no encoder was ever opened on it (see `pacing::SKIP_EMPTY`).
    empty: bool,
}

impl crate::DynCommandBuffer for CommandBuffer {}

unsafe impl Send for CommandBuffer {}
unsafe impl Sync for CommandBuffer {}

#[derive(Debug)]
pub struct PipelineCache;

impl crate::DynPipelineCache for PipelineCache {}

#[derive(Debug)]
pub struct AccelerationStructure {
    raw: Retained<ProtocolObject<dyn MTLAccelerationStructure>>,
}

impl AccelerationStructure {
    fn as_raw(&self) -> NonNull<ProtocolObject<dyn MTLAccelerationStructure>> {
        unsafe { NonNull::new_unchecked(Retained::as_ptr(&self.raw) as *mut _) }
    }
}

impl crate::DynAccelerationStructure for AccelerationStructure {}
unsafe impl Send for AccelerationStructure {}
unsafe impl Sync for AccelerationStructure {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsType {
    Macos,
    Ios,
    Tvos,
    VisionOs,
}
