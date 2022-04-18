use crate::*;
use std::cell::{Cell, RefCell};
use windows::core::{Interface, PCSTR};
use windows::Win32::{
    Foundation::*,
    Graphics::{Direct3D::*, Direct3D12::*, Dxgi::Common::*, Dxgi::*},
};

#[repr(C)]
struct Vertex {
    _position: [f32; 3],
    _coord: [f32; 2],
}

impl Vertex {
    const fn new(position: [f32; 3], coord: [f32; 2]) -> Self {
        Self {
            _position: position,
            _coord: coord,
        }
    }
}

type PlaneVertices = [Vertex; 4];
type PlaneIndices = [u32; 6];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PlaneOrigin {
    LeftTop,
    LeftBottom,
}

#[repr(C)]
struct PlaneBuffer {
    vertices: PlaneVertices,
    indices: PlaneIndices,
}

impl PlaneBuffer {
    const fn new(origin: PlaneOrigin) -> Self {
        Self {
            vertices: match origin {
                PlaneOrigin::LeftTop => [
                    Vertex::new([-1.0, 1.0, 0.0], [0.0, 0.0]),
                    Vertex::new([1.0, 1.0, 0.0], [1.0, 0.0]),
                    Vertex::new([-1.0, -1.0, 0.0], [0.0, 1.0]),
                    Vertex::new([1.0, -1.0, 0.0], [1.0, 1.0]),
                ],
                PlaneOrigin::LeftBottom => [
                    Vertex::new([-1.0, 1.0, 0.0], [0.0, 1.0]),
                    Vertex::new([1.0, 1.0, 0.0], [1.0, 1.0]),
                    Vertex::new([-1.0, -1.0, 0.0], [0.0, 0.0]),
                    Vertex::new([1.0, -1.0, 0.0], [1.0, 0.0]),
                ],
            },
            indices: [0, 1, 2, 1, 3, 2],
        }
    }

    const fn vertices_size() -> usize {
        std::mem::size_of::<PlaneVertices>()
    }

    const fn indicies_size() -> usize {
        std::mem::size_of::<PlaneIndices>()
    }

    const fn indices_len(&self) -> usize {
        self.indices.len()
    }
}

struct Plane {
    _buffer: Buffer,
    vbv: D3D12_VERTEX_BUFFER_VIEW,
    ibv: D3D12_INDEX_BUFFER_VIEW,
    origin: PlaneOrigin,
}

impl Plane {
    fn new(device: &ID3D12Device, origin: PlaneOrigin) -> anyhow::Result<Self> {
        const BUFFER_SIZE: u64 = std::mem::size_of::<PlaneBuffer>() as _;
        unsafe {
            let buffer = Buffer::new(
                "Plane::buffer",
                device,
                HeapProperties::new(D3D12_HEAP_TYPE_DEFAULT),
                BUFFER_SIZE,
                D3D12_RESOURCE_STATE_COMMON,
                None,
            )?;
            let uploader = {
                let uploader = Buffer::new(
                    "Plane::uploader",
                    device,
                    HeapProperties::new(D3D12_HEAP_TYPE_UPLOAD),
                    BUFFER_SIZE + (16 - BUFFER_SIZE % 16) % 16,
                    D3D12_RESOURCE_STATE_GENERIC_READ,
                    None,
                )?;
                {
                    let data = uploader.map()?;
                    data.copy(&PlaneBuffer::new(origin));
                }
                uploader
            };
            let cmd_allocator: ID3D12CommandAllocator =
                device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_COPY)?;
            let cmd_list: ID3D12GraphicsCommandList =
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_COPY, &cmd_allocator, None)?;
            transition_barriers(
                &cmd_list,
                [TransitionBarrier {
                    resource: buffer.clone().into(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_COMMON,
                    state_after: D3D12_RESOURCE_STATE_COPY_DEST,
                }],
            );
            cmd_list.CopyBufferRegion(buffer.handle(), 0, uploader.handle(), 0, BUFFER_SIZE as _);
            transition_barriers(
                &cmd_list,
                [TransitionBarrier {
                    resource: buffer.clone().into(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_COPY_DEST,
                    state_after: D3D12_RESOURCE_STATE_COMMON,
                }],
            );
            cmd_list.Close()?;
            let cmd_queue = CommandQueue::new(device, D3D12_COMMAND_LIST_TYPE_COPY)?;
            let signal = cmd_queue.execute_command_lists(&[Some(cmd_list.cast()?)])?;
            let vbv = D3D12_VERTEX_BUFFER_VIEW {
                BufferLocation: buffer.gpu_virtual_address(),
                SizeInBytes: PlaneBuffer::vertices_size() as _,
                StrideInBytes: std::mem::size_of::<Vertex>() as _,
            };
            let ibv = D3D12_INDEX_BUFFER_VIEW {
                BufferLocation: buffer.gpu_virtual_address() + PlaneBuffer::vertices_size() as u64,
                SizeInBytes: PlaneBuffer::indicies_size() as _,
                Format: DXGI_FORMAT_R32_UINT,
            };
            if !signal.is_completed() {
                let event = Event::new()?;
                signal.set_event(&event)?;
                event.wait();
            }
            Ok(Self {
                _buffer: buffer,
                vbv,
                ibv,
                origin,
            })
        }
    }

    fn indices_len(&self) -> usize {
        PlaneBuffer::new(self.origin).indices_len()
    }
}

#[derive(Clone)]
struct Signal {
    fence: ID3D12Fence,
    value: u64,
}

impl Signal {
    fn is_completed(&self) -> bool {
        unsafe { self.fence.GetCompletedValue() >= self.value }
    }

    fn set_event(&self, event: &Event) -> anyhow::Result<()> {
        unsafe {
            self.fence
                .SetEventOnCompletion(self.value, event.handle())?;
            Ok(())
        }
    }
}

struct CommandQueue {
    queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    value: Cell<u64>,
}

impl CommandQueue {
    fn new(device: &ID3D12Device, t: D3D12_COMMAND_LIST_TYPE) -> anyhow::Result<Self> {
        unsafe {
            let queue = device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: t,
                ..Default::default()
            })?;
            let fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?;
            Ok(Self {
                queue,
                fence,
                value: Cell::new(1),
            })
        }
    }

    fn execute_command_lists(
        &self,
        cmd_lists: &[Option<ID3D12CommandList>],
    ) -> anyhow::Result<Signal> {
        unsafe {
            self.queue.ExecuteCommandLists(cmd_lists);
            self.signal()
        }
    }

    fn signal(&self) -> anyhow::Result<Signal> {
        unsafe {
            let value = self.value.get();
            self.queue.Signal(&self.fence, value)?;
            self.value.set(value + 1);
            Ok(Signal {
                fence: self.fence.clone(),
                value,
            })
        }
    }

    fn wait(&self, signal: &Signal) -> anyhow::Result<()> {
        unsafe {
            self.queue.Wait(&signal.fence, signal.value)?;
        }
        Ok(())
    }

    fn handle(&self) -> &ID3D12CommandQueue {
        &self.queue
    }
}

struct SwapChain {
    cmd_queue: CommandQueue,
    swap_chain: IDXGISwapChain4,
    back_buffers: Vec<ID3D12Resource>,
    rtv_heap: ID3D12DescriptorHeap,
    rtv_size: usize,
}

impl SwapChain {
    fn new(device: &ID3D12Device, window: &wita::Window, count: usize) -> anyhow::Result<Self> {
        unsafe {
            let cmd_queue = CommandQueue::new(device, D3D12_COMMAND_LIST_TYPE_DIRECT)?;
            let window_size = window.inner_size();
            let dxgi_factory: IDXGIFactory5 = CreateDXGIFactory1()?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: window_size.width,
                Height: window_size.height,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                BufferCount: count as _,
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                Scaling: DXGI_SCALING_NONE,
                SampleDesc: SampleDesc::default().into(),
                ..Default::default()
            };
            let swap_chain: IDXGISwapChain4 = {
                dxgi_factory
                    .CreateSwapChainForHwnd(
                        cmd_queue.handle(),
                        HWND(window.raw_handle() as _),
                        &desc,
                        std::ptr::null(),
                        None,
                    )?
                    .cast()?
            };
            let rtv_heap: ID3D12DescriptorHeap =
                device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                    NumDescriptors: desc.BufferCount,
                    ..Default::default()
                })?;
            rtv_heap.SetName("SwapChain::rtv_heap")?;
            let rtv_size =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV) as usize;
            let back_buffers = Self::create_back_buffers(device, &swap_chain, &rtv_heap, rtv_size)?;
            Ok(Self {
                cmd_queue,
                swap_chain,
                back_buffers,
                rtv_heap,
                rtv_size,
            })
        }
    }

    fn desc(&self) -> anyhow::Result<DXGI_SWAP_CHAIN_DESC1> {
        unsafe {
            let desc = self.swap_chain.GetDesc1()?;
            Ok(desc)
        }
    }

    fn current_buffer(&self) -> (D3D12_CPU_DESCRIPTOR_HANDLE, ID3D12Resource, usize) {
        unsafe {
            let index = self.swap_chain.GetCurrentBackBufferIndex() as usize;
            let mut handle = self.rtv_heap.GetCPUDescriptorHandleForHeapStart();
            handle.ptr += self.rtv_size * index;
            (handle, self.back_buffers[index].clone(), index)
        }
    }

    fn present(&self, interval: u32) -> anyhow::Result<Signal> {
        unsafe {
            self.swap_chain.Present(interval, 0)?;
            let value = self.cmd_queue.value.get();
            self.cmd_queue.queue.Signal(&self.cmd_queue.fence, value)?;
            self.cmd_queue.value.set(value + 1);
            Ok(Signal {
                fence: self.cmd_queue.fence.clone(),
                value,
            })
        }
    }

    fn resize(
        &mut self,
        device: &ID3D12Device,
        size: wita::PhysicalSize<u32>,
    ) -> anyhow::Result<()> {
        self.back_buffers.clear();
        unsafe {
            self.swap_chain
                .ResizeBuffers(0, size.width, size.height, DXGI_FORMAT_UNKNOWN, 0)?;
            self.back_buffers =
                Self::create_back_buffers(device, &self.swap_chain, &self.rtv_heap, self.rtv_size)?;
        }
        Ok(())
    }

    fn create_back_buffers(
        device: &ID3D12Device,
        swap_chain: &IDXGISwapChain4,
        rtv_heap: &ID3D12DescriptorHeap,
        rtv_size: usize,
    ) -> anyhow::Result<Vec<ID3D12Resource>> {
        unsafe {
            let desc = swap_chain.GetDesc1()?;
            let mut back_buffers = Vec::with_capacity(desc.BufferCount as _);
            let mut handle = rtv_heap.GetCPUDescriptorHandleForHeapStart();
            for i in 0..desc.BufferCount {
                let buffer: ID3D12Resource = swap_chain.GetBuffer(i)?;
                buffer.SetName(format!("SwapChain::back_buffers[{}]", i))?;
                device.CreateRenderTargetView(&buffer, std::ptr::null(), handle);
                back_buffers.push(buffer);
                handle.ptr += rtv_size;
            }
            Ok(back_buffers)
        }
    }
}

#[repr(C)]
pub struct Parameters {
    pub resolution: [f32; 2],
    pub mouse: [f32; 2],
    pub time: f32,
}

#[repr(transparent)]
pub struct PixelShaderPipeline(ID3D12PipelineState);

struct PixelShader {
    device: ID3D12Device,
    root_signature: ID3D12RootSignature,
    parameters: Buffer,
    plane: Plane,
    vs: hlsl::Blob,
}

impl PixelShader {
    fn new(
        device: &ID3D12Device,
        compiler: &hlsl::Compiler,
        shader_model: hlsl::ShaderModel,
    ) -> anyhow::Result<Self> {
        unsafe {
            let root_signature: ID3D12RootSignature = {
                let params = [D3D12_ROOT_PARAMETER {
                    ParameterType: D3D12_ROOT_PARAMETER_TYPE_CBV,
                    ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
                    Anonymous: D3D12_ROOT_PARAMETER_0 {
                        Descriptor: D3D12_ROOT_DESCRIPTOR {
                            ShaderRegister: 0,
                            RegisterSpace: 0,
                        },
                    },
                }];
                let desc = D3D12_ROOT_SIGNATURE_DESC {
                    NumParameters: params.len() as _,
                    pParameters: params.as_ptr(),
                    NumStaticSamplers: 0,
                    pStaticSamplers: std::ptr::null(),
                    Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT
                        | D3D12_ROOT_SIGNATURE_FLAG_DENY_DOMAIN_SHADER_ROOT_ACCESS
                        | D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS
                        | D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS,
                };
                let mut blob: Option<ID3DBlob> = None;
                let blob = D3D12SerializeRootSignature(
                    &desc,
                    D3D_ROOT_SIGNATURE_VERSION_1_0,
                    &mut blob,
                    std::ptr::null_mut(),
                )
                .map(|_| blob.unwrap())?;
                device.CreateRootSignature(
                    0,
                    std::slice::from_raw_parts(
                        blob.GetBufferPointer() as *const u8,
                        blob.GetBufferSize(),
                    ),
                )?
            };
            root_signature.SetName("PixelShader::root_signature")?;
            let parameters = Buffer::new(
                "PixelShader::parameters",
                device,
                HeapProperties::new(D3D12_HEAP_TYPE_UPLOAD),
                std::mem::size_of::<Parameters>() as _,
                D3D12_RESOURCE_STATE_GENERIC_READ,
                None,
            )?;
            let plane = Plane::new(device, PlaneOrigin::LeftBottom)?;
            let vs = compiler.compile_from_str(
                include_str!("./shader/plane.hlsl"),
                "main",
                hlsl::Target::VS(shader_model),
                &vec![],
            )?;
            Ok(Self {
                device: device.clone(),
                root_signature,
                parameters,
                plane,
                vs,
            })
        }
    }

    fn create_pipeline(&self, ps: &hlsl::Blob) -> anyhow::Result<PixelShaderPipeline> {
        unsafe {
            let input_elements = [
                D3D12_INPUT_ELEMENT_DESC {
                    SemanticName: PCSTR(b"POSITION\0".as_ptr()),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32B32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: 0,
                    InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
                D3D12_INPUT_ELEMENT_DESC {
                    SemanticName: PCSTR(b"TEXCOORD\0".as_ptr()),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: D3D12_APPEND_ALIGNED_ELEMENT,
                    InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
            ];
            let mut render_target_blend = [D3D12_RENDER_TARGET_BLEND_DESC::default(); 8];
            render_target_blend[0] = D3D12_RENDER_TARGET_BLEND_DESC {
                BlendEnable: false.into(),
                LogicOpEnable: false.into(),
                SrcBlend: D3D12_BLEND_ONE,
                DestBlend: D3D12_BLEND_ZERO,
                BlendOp: D3D12_BLEND_OP_ADD,
                SrcBlendAlpha: D3D12_BLEND_ONE,
                DestBlendAlpha: D3D12_BLEND_ZERO,
                BlendOpAlpha: D3D12_BLEND_OP_ADD,
                LogicOp: D3D12_LOGIC_OP_NOOP,
                RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as _,
            };
            let mut rtv_formats = [DXGI_FORMAT_UNKNOWN; 8];
            rtv_formats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
            let desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
                pRootSignature: Some(self.root_signature.clone()),
                VS: self.vs.as_shader_bytecode(),
                PS: ps.as_shader_bytecode(),
                PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
                InputLayout: D3D12_INPUT_LAYOUT_DESC {
                    pInputElementDescs: input_elements.as_ptr(),
                    NumElements: input_elements.len() as _,
                },
                BlendState: D3D12_BLEND_DESC {
                    RenderTarget: render_target_blend,
                    ..Default::default()
                },
                RasterizerState: D3D12_RASTERIZER_DESC {
                    FillMode: D3D12_FILL_MODE_SOLID,
                    CullMode: D3D12_CULL_MODE_BACK,
                    ..Default::default()
                },
                NumRenderTargets: 1,
                RTVFormats: rtv_formats,
                SampleMask: u32::MAX,
                SampleDesc: SampleDesc::default().into(),
                ..Default::default()
            };
            self.device
                .CreateGraphicsPipelineState(&desc)
                .map(|pl| PixelShaderPipeline(pl))
                .map_err(|e| e.into())
        }
    }

    fn execute(
        &self,
        cmd_list: &ID3D12GraphicsCommandList,
        pipeline: &PixelShaderPipeline,
        parameters: &Parameters,
    ) {
        unsafe {
            let data = self.parameters.map().unwrap();
            data.copy(parameters);
        }
        unsafe {
            cmd_list.SetGraphicsRootSignature(&self.root_signature);
            cmd_list.SetPipelineState(&pipeline.0);
            cmd_list.SetGraphicsRootConstantBufferView(0, self.parameters.gpu_virtual_address());
            cmd_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            cmd_list.IASetVertexBuffers(0, &[self.plane.vbv.clone()]);
            cmd_list.IASetIndexBuffer(&self.plane.ibv);
            cmd_list.DrawIndexedInstanced(self.plane.indices_len() as _, 1, 0, 0, 0);
        }
    }
}

pub trait UiRender {
    fn render(&self, cmd: &mltg::DrawCommand);
}

struct Ui {
    context: mltg::Context<mltg::Direct3D12>,
    cmd_queue: CommandQueue,
    desc_heap: ID3D12DescriptorHeap,
    desc_size: usize,
    buffers: Vec<(Texture2D, mltg::d3d12::RenderTarget)>,
    plane: Plane,
    root_signature: ID3D12RootSignature,
    pipeline: ID3D12PipelineState,
    signals: RefCell<Vec<Option<Signal>>>,
    wait_event: Event,
}

impl Ui {
    fn new(
        device: &ID3D12Device,
        count: usize,
        window: &wita::Window,
        compiler: &hlsl::Compiler,
        shader_model: hlsl::ShaderModel,
    ) -> anyhow::Result<Self> {
        unsafe {
            let size = window.inner_size();
            let cmd_queue = CommandQueue::new(device, D3D12_COMMAND_LIST_TYPE_DIRECT)?;
            cmd_queue.queue.SetName("Ui::cmd_queue::queue")?;
            let context = mltg::Context::new(mltg::Direct3D12::new(device, &cmd_queue.queue)?)?;
            context.set_dpi(window.dpi() as _);
            let desc_heap: ID3D12DescriptorHeap =
                device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                    NumDescriptors: count as _,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                    ..Default::default()
                })?;
            desc_heap.SetName("Ui::desc_heap")?;
            let desc_size = device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV)
                as usize;
            let mut buffers = Vec::with_capacity(count);
            let mut handle = desc_heap.GetCPUDescriptorHandleForHeapStart();
            for i in 0..count {
                let buffer = Texture2D::new(
                    &format!("Ui::buffers[{}]", i),
                    device,
                    size.width as _,
                    size.height as _,
                    D3D12_RESOURCE_STATE_COMMON,
                    None,
                    Some(
                        D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET
                            | D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
                    ),
                )?;
                buffer.handle().SetName(format!("Ui::buffer[{}]", i))?;
                let srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
                    ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                    Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                        Texture2D: D3D12_TEX2D_SRV {
                            MipLevels: 1,
                            ..Default::default()
                        },
                    },
                };
                device.CreateShaderResourceView(buffer.handle(), &srv_desc, handle);
                let target = context.create_render_target(&buffer)?;
                buffers.push((buffer, target));
                handle.ptr += desc_size;
            }
            let plane = Plane::new(device, PlaneOrigin::LeftTop)?;
            let root_signature: ID3D12RootSignature = {
                let ranges = [D3D12_DESCRIPTOR_RANGE {
                    RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
                    NumDescriptors: 1,
                    BaseShaderRegister: 0,
                    RegisterSpace: 0,
                    OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
                }];
                let parameters = [D3D12_ROOT_PARAMETER {
                    ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                    ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
                    Anonymous: D3D12_ROOT_PARAMETER_0 {
                        DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                            NumDescriptorRanges: ranges.len() as _,
                            pDescriptorRanges: ranges.as_ptr(),
                        },
                    },
                }];
                let static_samplers = [D3D12_STATIC_SAMPLER_DESC {
                    Filter: D3D12_FILTER_MIN_MAG_MIP_LINEAR,
                    AddressU: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
                    AddressV: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
                    AddressW: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
                    MinLOD: 0.0,
                    MaxLOD: f32::MAX,
                    ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
                    ShaderRegister: 0,
                    RegisterSpace: 0,
                    ..Default::default()
                }];
                let desc = D3D12_ROOT_SIGNATURE_DESC {
                    NumParameters: parameters.len() as _,
                    pParameters: parameters.as_ptr(),
                    NumStaticSamplers: static_samplers.len() as _,
                    pStaticSamplers: static_samplers.as_ptr(),
                    Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT
                        | D3D12_ROOT_SIGNATURE_FLAG_DENY_DOMAIN_SHADER_ROOT_ACCESS
                        | D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS
                        | D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS,
                };
                let mut blob = None;
                let blob: ID3DBlob = D3D12SerializeRootSignature(
                    &desc,
                    D3D_ROOT_SIGNATURE_VERSION_1_0,
                    &mut blob,
                    std::ptr::null_mut(),
                )
                .map(|_| blob.unwrap())?;
                device.CreateRootSignature(
                    0,
                    std::slice::from_raw_parts(
                        blob.GetBufferPointer() as _,
                        blob.GetBufferSize() as _,
                    ),
                )?
            };
            root_signature.SetName("Ui::root_signature")?;
            let pipeline: ID3D12PipelineState = {
                let shader = include_str!("./shader/ui.hlsl");
                let vs = compiler.compile_from_str(
                    shader,
                    "vs_main",
                    hlsl::Target::VS(shader_model),
                    &vec![],
                )?;
                let ps = compiler.compile_from_str(
                    shader,
                    "ps_main",
                    hlsl::Target::PS(shader_model),
                    &vec![],
                )?;
                let input_elements = [
                    D3D12_INPUT_ELEMENT_DESC {
                        SemanticName: PCSTR(b"POSITION\0".as_ptr()),
                        SemanticIndex: 0,
                        Format: DXGI_FORMAT_R32G32B32_FLOAT,
                        InputSlot: 0,
                        AlignedByteOffset: 0,
                        InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                        InstanceDataStepRate: 0,
                    },
                    D3D12_INPUT_ELEMENT_DESC {
                        SemanticName: PCSTR(b"TEXCOORD\0".as_ptr()),
                        SemanticIndex: 0,
                        Format: DXGI_FORMAT_R32G32_FLOAT,
                        InputSlot: 0,
                        AlignedByteOffset: D3D12_APPEND_ALIGNED_ELEMENT,
                        InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                        InstanceDataStepRate: 0,
                    },
                ];
                let mut render_target_blend = [D3D12_RENDER_TARGET_BLEND_DESC::default(); 8];
                render_target_blend[0] = D3D12_RENDER_TARGET_BLEND_DESC {
                    BlendEnable: true.into(),
                    LogicOpEnable: false.into(),
                    SrcBlend: D3D12_BLEND_SRC_ALPHA,
                    DestBlend: D3D12_BLEND_INV_SRC_ALPHA,
                    BlendOp: D3D12_BLEND_OP_ADD,
                    SrcBlendAlpha: D3D12_BLEND_ONE,
                    DestBlendAlpha: D3D12_BLEND_ZERO,
                    BlendOpAlpha: D3D12_BLEND_OP_ADD,
                    LogicOp: D3D12_LOGIC_OP_NOOP,
                    RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as _,
                };
                let mut rtv_formats = [DXGI_FORMAT_UNKNOWN; 8];
                rtv_formats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
                let desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
                    pRootSignature: Some(root_signature.clone()),
                    VS: vs.as_shader_bytecode(),
                    PS: ps.as_shader_bytecode(),
                    PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
                    InputLayout: D3D12_INPUT_LAYOUT_DESC {
                        pInputElementDescs: input_elements.as_ptr(),
                        NumElements: input_elements.len() as _,
                    },
                    BlendState: D3D12_BLEND_DESC {
                        RenderTarget: render_target_blend,
                        ..Default::default()
                    },
                    RasterizerState: D3D12_RASTERIZER_DESC {
                        FillMode: D3D12_FILL_MODE_SOLID,
                        CullMode: D3D12_CULL_MODE_BACK,
                        ..Default::default()
                    },
                    NumRenderTargets: 1,
                    RTVFormats: rtv_formats,
                    SampleMask: u32::MAX,
                    SampleDesc: SampleDesc::default().into(),
                    ..Default::default()
                };
                device.CreateGraphicsPipelineState(&desc)?
            };
            pipeline.SetName("Ui::pipeline")?;
            let signals = RefCell::new(vec![None; count]);
            Ok(Self {
                context,
                cmd_queue,
                desc_heap,
                desc_size,
                buffers,
                plane,
                root_signature,
                pipeline,
                signals,
                wait_event: Event::new()?,
            })
        }
    }

    fn render(&self, index: usize, r: &impl UiRender) -> anyhow::Result<Signal> {
        let buffer = &self.buffers[index];
        self.context.draw(&buffer.1, |cmd| {
            cmd.clear([0.0, 0.0, 0.0, 0.0]);
            r.render(cmd);
        })?;
        let signal = self.cmd_queue.signal()?;
        self.signals.borrow_mut()[index] = Some(signal.clone());
        Ok(signal)
    }

    fn copy(&self, index: usize, cmd_list: &ID3D12GraphicsCommandList) {
        let buffer = &self.buffers[index];
        unsafe {
            let mut srv_handle = self.desc_heap.GetGPUDescriptorHandleForHeapStart();
            srv_handle.ptr += (index * self.desc_size) as u64;
            transition_barriers(
                cmd_list,
                [TransitionBarrier {
                    resource: buffer.0.handle().clone(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_COMMON,
                    state_after: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                }],
            );
            cmd_list.SetDescriptorHeaps(&[Some(self.desc_heap.clone())]);
            cmd_list.SetGraphicsRootSignature(&self.root_signature);
            cmd_list.SetPipelineState(&self.pipeline);
            cmd_list.SetGraphicsRootDescriptorTable(0, srv_handle);
            cmd_list.IASetVertexBuffers(0, &[self.plane.vbv.clone()]);
            cmd_list.IASetIndexBuffer(&self.plane.ibv);
            cmd_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            cmd_list.DrawIndexedInstanced(self.plane.indices_len() as _, 1, 0, 0, 0);
            transition_barriers(
                cmd_list,
                [TransitionBarrier {
                    resource: buffer.0.handle().clone(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                    state_after: D3D12_RESOURCE_STATE_COMMON,
                }],
            );
        }
    }

    fn wait_all_signals(&self) {
        for signal in self.signals.borrow().iter() {
            if let Some(signal) = signal {
                if !signal.is_completed() {
                    signal.set_event(&self.wait_event).unwrap();
                    self.wait_event.wait();
                }
            }
        }
    }
}

pub struct Renderer {
    d3d12_device: ID3D12Device,
    swap_chain: SwapChain,
    pixel_shader: PixelShader,
    cmd_allocators: Vec<ID3D12CommandAllocator>,
    cmd_lists: Vec<ID3D12GraphicsCommandList>,
    wait_event: Event,
    signals: RefCell<Vec<Option<Signal>>>,
    ui: Ui,
}

impl Renderer {
    pub fn new(
        d3d12_device: &ID3D12Device,
        window: &wita::Window,
        compiler: &hlsl::Compiler,
        shader_model: hlsl::ShaderModel,
    ) -> anyhow::Result<Self> {
        const BUFFER_COUNT: usize = 2;
        unsafe {
            let swap_chain = SwapChain::new(d3d12_device, window, BUFFER_COUNT)?;
            let mut cmd_allocators = Vec::with_capacity(4);
            for i in 0..BUFFER_COUNT * 2 {
                let cmd_allocator: ID3D12CommandAllocator =
                    d3d12_device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?;
                cmd_allocator.SetName(format!("Renderer::cmd_allocators[{}]", i))?;
                cmd_allocators.push(cmd_allocator);
            }
            let mut cmd_lists = Vec::with_capacity(2);
            for i in 0..BUFFER_COUNT {
                let cmd_list: ID3D12GraphicsCommandList = d3d12_device.CreateCommandList(
                    0,
                    D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &cmd_allocators[0],
                    None,
                )?;
                cmd_list.SetName(format!("Renderer::cmd_lists[{}]", i))?;
                cmd_list.Close()?;
                cmd_lists.push(cmd_list);
            }
            let pixel_shader = PixelShader::new(&d3d12_device, compiler, shader_model)?;
            let ui = Ui::new(&d3d12_device, BUFFER_COUNT, window, compiler, shader_model)?;
            Ok(Self {
                d3d12_device: d3d12_device.clone(),
                swap_chain,
                pixel_shader,
                cmd_allocators,
                cmd_lists,
                wait_event: Event::new()?,
                signals: RefCell::new(vec![None; 2]),
                ui,
            })
        }
    }

    pub fn mltg_factory(&self) -> mltg::Factory {
        self.ui.context.create_factory()
    }

    pub fn create_pixel_shader_pipeline(
        &self,
        ps: &hlsl::Blob,
    ) -> anyhow::Result<PixelShaderPipeline> {
        self.pixel_shader.create_pipeline(ps)
    }

    pub fn render(
        &self,
        interval: u32,
        clear_color: &[f32],
        ps: Option<&PixelShaderPipeline>,
        parameters: Option<&Parameters>,
        r: &impl UiRender,
    ) -> anyhow::Result<()> {
        let swap_chain_desc = self.swap_chain.desc()?;
        let (handle, back_buffer, index) = self.swap_chain.current_buffer();
        if let Some(signal) = self.signals.borrow_mut()[index].take() {
            if !signal.is_completed() {
                signal.set_event(&self.wait_event)?;
                self.wait_event.wait();
            }
        }
        let cmd_allocators = &self.cmd_allocators[index * 2..=index * 2 + 1];
        unsafe {
            let cmd_list = &self.cmd_lists[0];
            cmd_allocators[0].Reset()?;
            cmd_list.Reset(&cmd_allocators[0], None)?;
            transition_barriers(
                cmd_list,
                [TransitionBarrier {
                    resource: back_buffer.clone(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_PRESENT,
                    state_after: D3D12_RESOURCE_STATE_RENDER_TARGET,
                }],
            );
            cmd_list.ClearRenderTargetView(handle, clear_color.as_ptr(), &[]);
            cmd_list.RSSetViewports(&[D3D12_VIEWPORT {
                Width: swap_chain_desc.Width as _,
                Height: swap_chain_desc.Height as _,
                MaxDepth: 1.0,
                ..Default::default()
            }]);
            cmd_list.RSSetScissorRects(&[RECT {
                right: swap_chain_desc.Width as _,
                bottom: swap_chain_desc.Height as _,
                ..Default::default()
            }]);
            let rtvs = [handle.clone()];
            cmd_list.OMSetRenderTargets(rtvs.len() as _, rtvs.as_ptr(), false, std::ptr::null());
            if let Some(ps) = ps {
                if let Some(parameters) = parameters {
                    self.pixel_shader.execute(&cmd_list, ps, parameters);
                }
            }
            cmd_list.Close()?;
            self.swap_chain
                .cmd_queue
                .execute_command_lists(&[Some(cmd_list.cast().unwrap())])?;
        }
        unsafe {
            let cmd_list = &self.cmd_lists[1];
            cmd_allocators[1].Reset()?;
            cmd_list.Reset(&cmd_allocators[1], None)?;
            cmd_list.RSSetViewports(&[D3D12_VIEWPORT {
                Width: swap_chain_desc.Width as _,
                Height: swap_chain_desc.Height as _,
                MaxDepth: 1.0,
                ..Default::default()
            }]);
            cmd_list.RSSetScissorRects(&[RECT {
                right: swap_chain_desc.Width as _,
                bottom: swap_chain_desc.Height as _,
                ..Default::default()
            }]);
            let rtvs = [handle.clone()];
            cmd_list.OMSetRenderTargets(rtvs.len() as _, rtvs.as_ptr(), false, std::ptr::null());
            self.ui.copy(index, cmd_list);
            transition_barriers(
                cmd_list,
                [TransitionBarrier {
                    resource: back_buffer.clone(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_RENDER_TARGET,
                    state_after: D3D12_RESOURCE_STATE_PRESENT,
                }],
            );
            cmd_list.Close()?;
            let ui_signal = self.ui.render(index, r)?;
            self.swap_chain.cmd_queue.wait(&ui_signal)?;
            self.swap_chain
                .cmd_queue
                .execute_command_lists(&[Some(cmd_list.cast().unwrap())])?;
        }
        let signal = self.swap_chain.present(interval)?;
        self.signals.borrow_mut()[index] = Some(signal);
        Ok(())
    }

    pub fn resize(&mut self, size: wita::PhysicalSize<u32>) -> anyhow::Result<()> {
        self.wait_all_signals();
        self.swap_chain.resize(&self.d3d12_device, size)?;
        Ok(())
    }

    pub fn wait_all_signals(&self) {
        for signal in self.signals.borrow().iter() {
            if let Some(signal) = signal {
                if !signal.is_completed() {
                    signal.set_event(&self.wait_event).unwrap();
                    self.wait_event.wait();
                }
            }
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.ui.wait_all_signals();
        self.wait_all_signals();
    }
}
