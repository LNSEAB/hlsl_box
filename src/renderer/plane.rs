use super::*;

#[repr(C)]
struct Vertex {
    position: [f32; 3],
    _coord: [f32; 2],
}

impl Vertex {
    const fn new(position: [f32; 3], coord: [f32; 2]) -> Self {
        Self {
            position,
            _coord: coord,
        }
    }
}

type PlaneVertices = [Vertex; 4];
type PlaneIndices = [u32; 6];

#[repr(C)]
pub struct Meshes {
    vertices: PlaneVertices,
    indices: PlaneIndices,
}

impl Meshes {
    pub fn new(w: f32, h: f32) -> Self {
        Self {
            vertices: [
                Vertex::new([-1.0 * w, 1.0 * h, 0.0], [0.0, 0.0]),
                Vertex::new([1.0 * w, 1.0 * h, 0.0], [1.0, 0.0]),
                Vertex::new([-1.0 * w, -1.0 * h, 0.0], [0.0, 1.0]),
                Vertex::new([1.0 * w, -1.0 * h, 0.0], [1.0, 1.0]),
            ],
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

#[derive(Clone)]
pub(super) struct Buffer {
    buffer: DefaultBuffer,
    pub vbv: D3D12_VERTEX_BUFFER_VIEW,
    pub ibv: D3D12_INDEX_BUFFER_VIEW,
}

impl Buffer {
    const BUFFER_SIZE: u64 = std::mem::size_of::<Meshes>() as _;

    pub fn new(
        device: &ID3D12Device,
        copy_queue: &CommandQueue<CopyCommandList>,
    ) -> Result<Self, Error> {
        let buffer = DefaultBuffer::new("plane::Buffer::buffer", device, Self::BUFFER_SIZE)?;
        Self::copy_buffer(device, copy_queue, &buffer, &Meshes::new(1.0, 1.0))?;
        let vbv = D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: buffer.0.gpu_virtual_address(),
            SizeInBytes: Meshes::vertices_size() as _,
            StrideInBytes: std::mem::size_of::<Vertex>() as _,
        };
        let ibv = D3D12_INDEX_BUFFER_VIEW {
            BufferLocation: buffer.0.gpu_virtual_address() + Meshes::vertices_size() as u64,
            SizeInBytes: Meshes::indicies_size() as _,
            Format: DXGI_FORMAT_R32_UINT,
        };
        Ok(Self { buffer, vbv, ibv })
    }

    pub fn indices_len(&self) -> usize {
        Meshes::new(1.0, 1.0).indices_len()
    }

    pub fn replace(
        &self,
        device: &ID3D12Device,
        copy_queue: &CommandQueue<CopyCommandList>,
        plane: &Meshes,
    ) -> Result<(), Error> {
        Self::copy_buffer(device, copy_queue, &self.buffer, plane)
    }

    fn copy_buffer(
        device: &ID3D12Device,
        copy_queue: &CommandQueue<CopyCommandList>,
        buffer: &DefaultBuffer,
        plane: &Meshes,
    ) -> Result<(), Error> {
        unsafe {
            let uploader = {
                let uploader =
                    UploadBuffer::new("plane::Buffer::uploader", device, Self::BUFFER_SIZE)?;
                let data = uploader.0.map()?;
                std::ptr::copy_nonoverlapping(plane, data.as_mut(), 1);
                std::mem::drop(data);
                uploader
            };
            let cmd_allocator: ID3D12CommandAllocator =
                device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_COPY)?;
            let cmd_list = CopyCommandList::new("plane::Buffer::cmd_list", device, &cmd_allocator)?;
            cmd_list.record(
                &cmd_allocator,
                |cmd: CopyCommand<UploadBuffer, DefaultBuffer>| {
                    cmd.barrier([buffer.enter()]);
                    cmd.copy(&uploader, buffer);
                    cmd.barrier([buffer.leave()]);
                },
            )?;
            copy_queue.execute([&cmd_list])?.wait()?;
            Ok(())
        }
    }
}
