use super::*;

#[derive(Clone)]
pub struct CopyTextureShader {
    pub root_signature: ID3D12RootSignature,
    pub pipeline: ID3D12PipelineState,
}

impl CopyTextureShader {
    pub fn new(
        device: &ID3D12Device,
        compiler: &hlsl::Compiler,
        shader_model: hlsl::ShaderModel,
    ) -> Result<Self, Error> {
        unsafe {
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
            let pipeline: ID3D12PipelineState = {
                let shader = include_str!("../shader/copy_texture.hlsl");
                let vs = compiler.compile_from_str(
                    shader,
                    "vs_main",
                    hlsl::Target::VS(shader_model),
                    &[],
                )?;
                let ps = compiler.compile_from_str(
                    shader,
                    "ps_main",
                    hlsl::Target::PS(shader_model),
                    &[],
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
            Ok(Self {
                root_signature,
                pipeline,
            })
        }
    }
}
