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
                    &[0.0, 0.0, 0.0, 0.0],
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

    pub fn copy_resource(&self, index: usize) -> CopyResource {
        CopyResource {
            resource: self.buffers[index].handle().clone(),
        }
    }
}

impl TargetableBuffers for RenderTargetBuffers {
    fn len(&self) -> usize {
        self.buffers.len()
    }

    fn target(&self, index: usize) -> RenderTarget {
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
}

impl PixelShaderResourceBuffers for RenderTargetBuffers {
    fn len(&self) -> usize {
        self.buffers.len()
    }

    fn source(&self, index: usize) -> PixelShaderResource {
        unsafe {
            let mut handle = self.desc_heap.GetGPUDescriptorHandleForHeapStart();
            handle.ptr += (index * self.desc_size) as u64;
            PixelShaderResource {
                resource: self.buffers[index].handle().clone(),
                heap: self.desc_heap.clone(),
                handle,
            }
        }
    }
}

#[derive(Clone)]
pub struct ReadBackBuffer {
    buffer: Buffer,
    size: wita::PhysicalSize<u32>,
}

impl ReadBackBuffer {
    pub fn new(device: &ID3D12Device, size: wita::PhysicalSize<u32>) -> Result<Self, Error> {
        let s = (size.width * size.height * 4) as u64;
        let buffer = Buffer::new(
            "ReadBackBuffer",
            device,
            HeapProperties::new(D3D12_HEAP_TYPE_READBACK),
            s + (16 - s % 16) % 16,
            D3D12_RESOURCE_STATE_COPY_DEST,
            None,
        )?;
        Ok(Self { buffer, size })
    }

    pub fn to_image(&self) -> Result<image::RgbaImage, Error> {
        let data = self.buffer.map::<u8>()?;
        let mut img = image::RgbaImage::new(self.size.width, self.size.height);
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ref(), img.as_mut_ptr(), img.len());
        }
        Ok(img)
    }
}

impl Resource for ReadBackBuffer {
    fn resource(&self) -> &ID3D12Resource {
        self.buffer.handle()
    }
}

unsafe impl Send for ReadBackBuffer {}
unsafe impl Sync for ReadBackBuffer {}

#[derive(Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct DefaultBuffer(pub Buffer);

impl DefaultBuffer {
    pub fn new(name: &str, device: &ID3D12Device, size: u64) -> Result<Self, Error> {
        let buffer = utility::Buffer::new(
            name,
            device,
            HeapProperties::new(D3D12_HEAP_TYPE_DEFAULT),
            size,
            D3D12_RESOURCE_STATE_COMMON,
            None,
        )?;
        Ok(Self(buffer))
    }
}

impl Resource for DefaultBuffer {
    fn resource(&self) -> &ID3D12Resource {
        self.0.handle()
    }
}

impl CopyDest for DefaultBuffer {}

#[derive(Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct UploadBuffer(pub Buffer);

impl UploadBuffer {
    pub fn new(name: &str, device: &ID3D12Device, size: u64) -> Result<Self, Error> {
        let buffer = Buffer::new(
            name,
            device,
            HeapProperties::new(D3D12_HEAP_TYPE_UPLOAD),
            size + (16 - size % 16) % 16,
            D3D12_RESOURCE_STATE_GENERIC_READ,
            None,
        )?;
        Ok(Self(buffer))
    }
}

impl Resource for UploadBuffer {
    fn resource(&self) -> &ID3D12Resource {
        self.0.handle()
    }
}

impl CopySource for UploadBuffer {}
