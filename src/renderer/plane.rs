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
pub struct Buffer {
    buffer: utility::Buffer,
    pub vbv: D3D12_VERTEX_BUFFER_VIEW,
    pub ibv: D3D12_INDEX_BUFFER_VIEW,
}

impl Buffer {
    const BUFFER_SIZE: u64 = std::mem::size_of::<Meshes>() as _;

    pub fn new(device: &ID3D12Device, copy_queue: &CommandQueue) -> Result<Self, Error> {
        let buffer = utility::Buffer::new(
            "Plane::buffer",
            device,
            HeapProperties::new(D3D12_HEAP_TYPE_DEFAULT),
            Self::BUFFER_SIZE,
            D3D12_RESOURCE_STATE_COMMON,
            None,
        )?;
        Self::copy_buffer(device, copy_queue, &buffer, &Meshes::new(1.0, 1.0))?;
        let vbv = D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: buffer.gpu_virtual_address(),
            SizeInBytes: Meshes::vertices_size() as _,
            StrideInBytes: std::mem::size_of::<Vertex>() as _,
        };
        let ibv = D3D12_INDEX_BUFFER_VIEW {
            BufferLocation: buffer.gpu_virtual_address() + Meshes::vertices_size() as u64,
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
        copy_queue: &CommandQueue,
        plane: &Meshes,
    ) -> Result<(), Error> {
        Self::copy_buffer(device, copy_queue, &self.buffer, plane)
    }

    fn copy_buffer(
        device: &ID3D12Device,
        copy_queue: &CommandQueue,
        buffer: &utility::Buffer,
        plane: &Meshes,
    ) -> Result<(), Error> {
        unsafe {
            let uploader = {
                let uploader = utility::Buffer::new(
                    "Plane::uploader",
                    device,
                    HeapProperties::new(D3D12_HEAP_TYPE_UPLOAD),
                    Self::BUFFER_SIZE + (16 - Self::BUFFER_SIZE % 16) % 16,
                    D3D12_RESOURCE_STATE_GENERIC_READ,
                    None,
                )?;
                {
                    let data = uploader.map()?;
                    data.copy(plane);
                }
                uploader
            };
            let cmd_allocator: ID3D12CommandAllocator =
                device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_COPY)?;
            let cmd_list: ID3D12GraphicsCommandList =
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_COPY, &cmd_allocator, None)?;
            transition_barriers(
                &cmd_list,
                [TransitionBarrier {
                    resource: buffer.clone().into(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_COMMON,
                    state_after: D3D12_RESOURCE_STATE_COPY_DEST,
                }],
            );
            cmd_list.CopyBufferRegion(
                buffer.handle(),
                0,
                uploader.handle(),
                0,
                Self::BUFFER_SIZE as _,
            );
            transition_barriers(
                &cmd_list,
                [TransitionBarrier {
                    resource: buffer.clone().into(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_COPY_DEST,
                    state_after: D3D12_RESOURCE_STATE_COMMON,
                }],
            );
            cmd_list.Close()?;
            let signal = copy_queue.execute_command_lists(&[Some(cmd_list.cast()?)])?;
            if !signal.is_completed() {
                let event = Event::new()?;
                signal.set_event(&event)?;
                event.wait();
            }
            Ok(())
        }
    }
}
