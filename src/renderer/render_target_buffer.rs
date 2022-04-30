use super::*;

pub struct RenderTargetBuffer {
    rtv_heap: ID3D12DescriptorHeap,
    rtv_size: usize,
    desc_heap: ID3D12DescriptorHeap,
    desc_size: usize,
    buffers: Vec<Texture2D>,
    copy_texture: CopyTextureShader,
    size: wita::PhysicalSize<u32>,
}

impl RenderTargetBuffer {
    pub fn new(
        device: &ID3D12Device,
        size: wita::PhysicalSize<u32>,
        copy_texture: CopyTextureShader,
        count: usize,
        clear_color: &[f32; 4],
    ) -> anyhow::Result<Self> {
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
                copy_texture,
                size,
            })
        }
    }

    pub fn set_target(
        &self,
        index: usize,
        cmd_list: &ID3D12GraphicsCommandList,
        clear_color: &[f32],
    ) {
        unsafe {
            let mut handle = self.rtv_heap.GetCPUDescriptorHandleForHeapStart();
            handle.ptr += index * self.rtv_size;
            let rtvs = [handle];
            transition_barriers(
                cmd_list,
                [TransitionBarrier {
                    resource: self.buffers[index].handle().clone(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_COMMON,
                    state_after: D3D12_RESOURCE_STATE_RENDER_TARGET,
                }],
            );
            cmd_list.ClearRenderTargetView(handle, clear_color.as_ptr(), &[]);
            cmd_list.RSSetViewports(&[D3D12_VIEWPORT {
                Width: self.size.width as _,
                Height: self.size.height as _,
                MaxDepth: 1.0,
                ..Default::default()
            }]);
            cmd_list.RSSetScissorRects(&[RECT {
                right: self.size.width as _,
                bottom: self.size.height as _,
                ..Default::default()
            }]);
            cmd_list.OMSetRenderTargets(rtvs.len() as _, rtvs.as_ptr(), false, std::ptr::null());
        }
    }

    pub fn copy(&self, index: usize, cmd_list: &ID3D12GraphicsCommandList) {
        unsafe {
            let mut handle = self.desc_heap.GetGPUDescriptorHandleForHeapStart();
            handle.ptr += (index * self.desc_size) as u64;
            transition_barriers(
                cmd_list,
                [TransitionBarrier {
                    resource: self.buffers[index].handle().clone(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_RENDER_TARGET,
                    state_after: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                }],
            );
            cmd_list.SetDescriptorHeaps(&[Some(self.desc_heap.clone())]);
            cmd_list.SetGraphicsRootSignature(&self.copy_texture.root_signature);
            cmd_list.SetGraphicsRootDescriptorTable(0, handle);
            cmd_list.SetPipelineState(&self.copy_texture.pipeline);
            cmd_list.IASetVertexBuffers(0, &[self.copy_texture.plane.vbv]);
            cmd_list.IASetIndexBuffer(&self.copy_texture.plane.ibv);
            cmd_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            cmd_list.DrawIndexedInstanced(self.copy_texture.plane.indices_len() as _, 1, 0, 0, 0);
            transition_barriers(
                cmd_list,
                [TransitionBarrier {
                    resource: self.buffers[index].handle().clone(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                    state_after: D3D12_RESOURCE_STATE_COMMON,
                }],
            );
        }
    }

    pub fn size(&self) -> wita::PhysicalSize<u32> {
        self.size
    }

    pub fn resize_plane(
        &mut self,
        device: &ID3D12Device,
        copy_queue: &CommandQueue,
        size: [f32; 2],
    ) -> anyhow::Result<()> {
        self.copy_texture.resize_plane(device, copy_queue, size)
    }
}
