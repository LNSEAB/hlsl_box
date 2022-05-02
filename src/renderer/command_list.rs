use super::*;

pub(super) struct CommandList<'a> {
    cmd_list: &'a ID3D12GraphicsCommandList,
    layer_shader: &'a LayerShader,
}

impl<'a> CommandList<'a> {
    pub fn new(
        cmd_list: &'a ID3D12GraphicsCommandList,
        allocator: &ID3D12CommandAllocator,
        layer_shader: &'a LayerShader,
    ) -> Result<Self, Error> {
        unsafe {
            allocator.Reset()?;
            cmd_list.Reset(allocator, None)?;
        }
        Ok(Self {
            cmd_list,
            layer_shader,
        })
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

impl<'a> From<CommandList<'a>> for ID3D12CommandList {
    fn from(src: CommandList<'a>) -> ID3D12CommandList {
        src.cmd_list.cast().unwrap()
    }
}

impl<'a> From<&CommandList<'a>> for ID3D12CommandList {
    fn from(src: &CommandList<'a>) -> ID3D12CommandList {
        src.cmd_list.cast().unwrap()
    }
}
