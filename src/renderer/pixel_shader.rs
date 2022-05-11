use super::*;

#[repr(C)]
pub struct Parameters {
    pub resolution: [f32; 2],
    pub mouse: [f32; 2],
    pub time: f32,
}

#[derive(Clone)]
#[repr(transparent)]
pub struct Pipeline(ID3D12PipelineState);

pub struct PixelShader {
    root_signature: ID3D12RootSignature,
    parameters: Buffer,
    vs: hlsl::Blob,
}

impl PixelShader {
    pub fn new(
        device: &ID3D12Device,
        compiler: &hlsl::Compiler,
        shader_model: hlsl::ShaderModel,
    ) -> Result<Self, Error> {
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
            let vs = compiler.compile_from_str(
                include_str!("../shader/plane.hlsl"),
                "main",
                hlsl::Target::VS(shader_model),
                &[],
            )?;
            Ok(Self {
                root_signature,
                parameters,
                vs,
            })
        }
    }

    pub fn create_pipeline(
        &self,
        name: &str,
        device: &ID3D12Device,
        ps: &hlsl::Blob,
    ) -> Result<Pipeline, Error> {
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
            let pipeline: ID3D12PipelineState = device.CreateGraphicsPipelineState(&desc)?;
            pipeline.SetName(name)?;
            Ok(Pipeline(pipeline))
        }
    }

    pub fn apply<'a, 'b>(&'a self, pipeline: &'b Pipeline, parameters: &Parameters) -> State<'a>
    where
        'b: 'a,
    {
        unsafe {
            let data = self.parameters.map().unwrap();
            std::ptr::copy_nonoverlapping(parameters, data.as_mut(), 1);
        }
        State {
            root_signature: &self.root_signature,
            pipeline,
            parameters: self.parameters.gpu_virtual_address(),
        }
    }
}

pub struct State<'a> {
    root_signature: &'a ID3D12RootSignature,
    pipeline: &'a Pipeline,
    parameters: u64,
}

impl<'a> Shader for State<'a> {
    fn record(&self, cmd_list: &ID3D12GraphicsCommandList) {
        unsafe {
            cmd_list.SetGraphicsRootSignature(self.root_signature);
            cmd_list.SetPipelineState(&self.pipeline.0);
            cmd_list.SetGraphicsRootConstantBufferView(0, self.parameters);
        }
    }
}
