use super::*;

pub trait CommandList {
    fn handle(&self) -> ID3D12CommandList;
}

pub(super) struct GraphicsCommand<'a>(&'a GraphicsCommandList);

impl<'a> GraphicsCommand<'a> {
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

pub(super) struct GraphicsCommandList {
    cmd_list: ID3D12GraphicsCommandList,
    layer_shader: LayerShader,
}

impl GraphicsCommandList {
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
        f: impl FnOnce(GraphicsCommand),
    ) -> Result<(), Error> {
        unsafe {
            allocator.Reset()?;
            self.cmd_list.Reset(allocator, None)?;
        }
        f(GraphicsCommand(self));
        unsafe {
            self.cmd_list.Close()?;
        }
        Ok(())
    }
}

impl CommandList for GraphicsCommandList {
    fn handle(&self) -> ID3D12CommandList {
        self.cmd_list.cast().unwrap()
    }
}

pub(super) struct CopyCommand<'a>(&'a CopyCommandList);

impl<'a> CopyCommand<'a> {
    pub fn barrier<const N: usize>(&self, barriers: [TransitionBarrier; N]) {
        transition_barriers(&self.0 .0, barriers);
    }

    pub fn copy(&self, src: &impl CopySource, dest: &ReadBackBuffer) {
        src.record(&self.0 .0, dest);
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

    pub fn record(
        &self,
        allocator: &ID3D12CommandAllocator,
        f: impl FnOnce(CopyCommand),
    ) -> Result<(), Error> {
        unsafe {
            allocator.Reset()?;
            self.0.Reset(allocator, None)?;
        }
        f(CopyCommand(self));
        unsafe {
            self.0.Close()?;
        }
        Ok(())
    }
}

impl CommandList for CopyCommandList {
    fn handle(&self) -> ID3D12CommandList {
        self.0.cast().unwrap()
    }
}
