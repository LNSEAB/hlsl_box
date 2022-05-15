use super::*;

pub trait CommandList {
    const LIST_TYPE: D3D12_COMMAND_LIST_TYPE;

    fn handle(&self) -> ID3D12CommandList;
}

pub(super) struct DirectCommand<'a>(&'a DirectCommandList);

impl<'a> DirectCommand<'a> {
    pub fn barrier<const N: usize>(&self, barriers: [TransitionBarrier; N]) {
        transition_barriers(&self.0.cmd_list, barriers);
    }

    pub fn clear(&self, target: &impl Target, clear_color: [f32; 4]) {
        target.clear(&self.0.cmd_list, clear_color);
    }

    pub fn layer(&self, src: &impl Source, dest: &impl Target, plane: &plane::Buffer) {
        self.0.layer_shader.record(&self.0.cmd_list);
        src.record(&self.0.cmd_list);
        dest.record(&self.0.cmd_list);
        self.draw_plane(plane);
    }

    pub fn draw(&self, shader: &impl Shader, target: &impl Target, plane: &plane::Buffer) {
        shader.record(&self.0.cmd_list);
        target.record(&self.0.cmd_list);
        self.draw_plane(plane);
    }

    fn draw_plane(&self, plane: &plane::Buffer) {
        unsafe {
            self.0
                .cmd_list
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.0.cmd_list.IASetVertexBuffers(0, &[plane.vbv]);
            self.0.cmd_list.IASetIndexBuffer(&plane.ibv);
            self.0
                .cmd_list
                .DrawIndexedInstanced(plane.indices_len() as _, 1, 0, 0, 0);
        }
    }
}

pub(super) struct DirectCommandList {
    cmd_list: ID3D12GraphicsCommandList,
    layer_shader: LayerShader,
}

impl DirectCommandList {
    pub fn new(
        name: &str,
        device: &ID3D12Device,
        allocator: &ID3D12CommandAllocator,
        layer_shader: LayerShader,
    ) -> Result<Self, Error> {
        unsafe {
            let cmd_list: ID3D12GraphicsCommandList =
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, allocator, None)?;
            cmd_list.SetName(name)?;
            cmd_list.Close()?;
            Ok(Self {
                cmd_list,
                layer_shader,
            })
        }
    }

    pub fn record(
        &self,
        allocator: &ID3D12CommandAllocator,
        f: impl FnOnce(DirectCommand),
    ) -> Result<(), Error> {
        unsafe {
            allocator.Reset()?;
            self.cmd_list.Reset(allocator, None)?;
        }
        f(DirectCommand(self));
        unsafe {
            self.cmd_list.Close()?;
        }
        Ok(())
    }
}

impl CommandList for DirectCommandList {
    const LIST_TYPE: D3D12_COMMAND_LIST_TYPE = D3D12_COMMAND_LIST_TYPE_DIRECT;

    fn handle(&self) -> ID3D12CommandList {
        self.cmd_list.cast().unwrap()
    }
}

pub(super) struct CopyCommand<'a, T, U> {
    cmd_list: &'a CopyCommandList,
    _t: std::marker::PhantomData<T>,
    _u: std::marker::PhantomData<U>,
}

impl<'a, T, U> CopyCommand<'a, T, U> {
    pub fn new(cmd_list: &'a CopyCommandList) -> Self {
        Self {
            cmd_list,
            _t: std::marker::PhantomData,
            _u: std::marker::PhantomData,
        }
    }

    pub fn barrier<const N: usize>(&self, barriers: [TransitionBarrier; N]) {
        transition_barriers(&self.cmd_list.0, barriers);
    }
}

impl<'a> CopyCommand<'a, UploadBuffer, DefaultBuffer> {
    pub fn copy(&self, src: &UploadBuffer, dest: &DefaultBuffer) {
        unsafe {
            self.cmd_list
                .0
                .CopyBufferRegion(dest.resource(), 0, src.resource(), 0, dest.0.size());
        }
    }
}

impl<'a, T> CopyCommand<'a, T, ReadBackBuffer>
where
    T: CopySource,
{
    pub fn copy(&self, src: &T, dest: &ReadBackBuffer) {
        unsafe {
            let cmd_list = &self.cmd_list.0;
            let device = {
                let mut device: Option<ID3D12Device> = None;
                cmd_list
                    .GetDevice(&mut device)
                    .map(|_| device.unwrap())
                    .unwrap()
            };
            let desc = src.resource().GetDesc();
            let mut foot_print = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
            device.GetCopyableFootprints(
                &desc,
                0,
                1,
                0,
                &mut foot_print,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            let copy_src = D3D12_TEXTURE_COPY_LOCATION {
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                pResource: Some(src.resource().clone()),
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };
            let copy_dest = D3D12_TEXTURE_COPY_LOCATION {
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                pResource: Some(dest.resource().clone()),
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: foot_print,
                },
            };
            cmd_list.CopyTextureRegion(&copy_dest, 0, 0, 0, &copy_src, std::ptr::null());
        }
    }
}

pub(super) struct CopyCommandList(ID3D12GraphicsCommandList);

impl CopyCommandList {
    pub fn new(
        name: &str,
        device: &ID3D12Device,
        allocator: &ID3D12CommandAllocator,
    ) -> Result<Self, Error> {
        unsafe {
            let cmd_list: ID3D12GraphicsCommandList =
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_COPY, allocator, None)?;
            cmd_list.SetName(name)?;
            cmd_list.Close()?;
            Ok(Self(cmd_list))
        }
    }

    pub fn record<T, U>(
        &self,
        allocator: &ID3D12CommandAllocator,
        f: impl FnOnce(CopyCommand<T, U>),
    ) -> Result<(), Error> {
        unsafe {
            allocator.Reset()?;
            self.0.Reset(allocator, None)?;
        }
        f(CopyCommand::new(self));
        unsafe {
            self.0.Close()?;
        }
        Ok(())
    }
}

impl CommandList for CopyCommandList {
    const LIST_TYPE: D3D12_COMMAND_LIST_TYPE = D3D12_COMMAND_LIST_TYPE_COPY;

    fn handle(&self) -> ID3D12CommandList {
        self.0.cast().unwrap()
    }
}
