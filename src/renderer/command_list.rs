use super::*;

pub(super) struct CommandList {
    cmd_list: ID3D12GraphicsCommandList,
    layer_shader: LayerShader,
}

impl CommandList {
    pub fn new(
        name: &str,
        device: &ID3D12Device,
        allocator: &ID3D12CommandAllocator,
        layer_shader: LayerShader,
    ) -> Result<Self, Error> {
        unsafe {
            let cmd_list: ID3D12GraphicsCommandList = device.CreateCommandList(
                0,
                D3D12_COMMAND_LIST_TYPE_DIRECT,
                allocator,
                None,
            )?; 
            cmd_list.SetName(name)?;
            cmd_list.Close()?;
            Ok(Self {
                cmd_list,
                layer_shader,
            })
        }
    }

    pub fn reset(&self, allocator: &ID3D12CommandAllocator) -> Result<(), Error> {
        unsafe {
            allocator.Reset()?;
            self.cmd_list.Reset(allocator, None)?;
            Ok(())
        }
    }

    pub fn barrier<const N: usize>(&self, barriers: [TransitionBarrier; N]) {
        transition_barriers(&self.cmd_list, barriers);
    }

    pub fn clear(&self, target: &impl Target, clear_color: [f32; 4]) {
        target.clear(&self.cmd_list, clear_color);
    }

    pub fn layer(&self, src: &impl Source, dest: &impl Target, plane: &plane::Buffer) {
        self.layer_shader.record(&self.cmd_list);
        src.record(&self.cmd_list);
        dest.record(&self.cmd_list);
        self.draw_plane(plane);
    }

    pub fn draw(&self, shader: &impl Shader, target: &impl Target, plane: &plane::Buffer) {
        shader.record(&self.cmd_list);
        target.record(&self.cmd_list);
        self.draw_plane(plane);
    }

    pub fn close(&self) -> Result<(), Error> {
        unsafe {
            self.cmd_list.Close()?;
            Ok(())
        }
    }

    fn draw_plane(&self, plane: &plane::Buffer) {
        unsafe {
            self.cmd_list
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.cmd_list.IASetVertexBuffers(0, &[plane.vbv]);
            self.cmd_list.IASetIndexBuffer(&plane.ibv);
            self.cmd_list
                .DrawIndexedInstanced(plane.indices_len() as _, 1, 0, 0, 0);
        }
    }
}

impl From<CommandList> for ID3D12CommandList {
    fn from(src: CommandList) -> ID3D12CommandList {
        src.cmd_list.cast().unwrap()
    }
}

impl From<&CommandList> for ID3D12CommandList {
    fn from(src: &CommandList) -> ID3D12CommandList {
        src.cmd_list.cast().unwrap()
    }
}
