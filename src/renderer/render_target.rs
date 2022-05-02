use super::*;

pub struct RenderTargetBuffers {
    rtv_heap: ID3D12DescriptorHeap,
    rtv_size: usize,
    desc_heap: ID3D12DescriptorHeap,
    desc_size: usize,
    buffers: Vec<Texture2D>,
    size: wita::PhysicalSize<u32>,
}

impl RenderTargetBuffers {
    pub fn new(
        device: &ID3D12Device,
        size: wita::PhysicalSize<u32>,
        count: usize,
        clear_color: &[f32; 4],
    ) -> Result<Self, Error> {
        unsafe {
            let rtv_heap: ID3D12DescriptorHeap =
                device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                    NumDescriptors: count as _,
                    ..Default::default()
                })?;
            let rtv_size =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV) as usize;
            let desc_heap: ID3D12DescriptorHeap =
                device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                    NumDescriptors: count as _,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                    ..Default::default()
                })?;
            let desc_size = device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV)
                as usize;
            let mut buffers = Vec::with_capacity(count);
            let mut rtv_handle = rtv_heap.GetCPUDescriptorHandleForHeapStart();
            let mut srv_handle = desc_heap.GetCPUDescriptorHandleForHeapStart();
            for i in 0..count {
                let texture = Texture2D::new(
                    &format!("RenderTarget::texture[{}]", i),
                    device,
                    size.width as _,
                    size.height,
                    D3D12_RESOURCE_STATE_COMMON,
                    None,
                    Some(D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET),
                    clear_color,
                )?;
                let rtv_desc = D3D12_RENDER_TARGET_VIEW_DESC {
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    ViewDimension: D3D12_RTV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D12_RENDER_TARGET_VIEW_DESC_0 {
                        Texture2D: D3D12_TEX2D_RTV::default(),
                    },
                };
                let srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                    Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                    Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                        Texture2D: D3D12_TEX2D_SRV {
                            MipLevels: 1,
                            ..Default::default()
                        },
                    },
                };
                device.CreateRenderTargetView(texture.handle(), &rtv_desc, rtv_handle);
                device.CreateShaderResourceView(texture.handle(), &srv_desc, srv_handle);
                buffers.push(texture);
                rtv_handle.ptr += rtv_size;
                srv_handle.ptr += desc_size;
            }
            Ok(Self {
                rtv_heap,
                rtv_size,
                desc_heap,
                desc_size,
                buffers,
                size,
            })
        }
    }

    pub fn size(&self) -> wita::PhysicalSize<u32> {
        self.size
    }

    pub fn target(&self, index: usize) -> RenderTarget {
        unsafe {
            let mut handle = self.rtv_heap.GetCPUDescriptorHandleForHeapStart();
            handle.ptr += index * self.rtv_size;
            RenderTarget {
                resource: self.buffers[index].handle().clone(),
                handle,
                size: self.size,
            }
        }
    }

    pub fn source(&self, index: usize) -> ShaderResource {
        unsafe {
            let mut handle = self.desc_heap.GetGPUDescriptorHandleForHeapStart();
            handle.ptr += (index * self.desc_size) as u64;
            ShaderResource {
                resource: self.buffers[index].handle().clone(),
                heap: self.desc_heap.clone(),
                handle,
            }
        }
    }
}
