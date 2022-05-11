use crate::error::Error;
use std::mem::ManuallyDrop;
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Graphics::{Direct3D12::*, Dxgi::Common::*},
    System::Threading::{CreateEventW, WaitForSingleObject},
    System::WindowsProgramming::INFINITE,
};

pub struct Event(HANDLE);

impl Event {
    pub fn new() -> Result<Self, Error> {
        unsafe {
            let event = CreateEventW(std::ptr::null(), false, false, None)?;
            Ok(Self(event))
        }
    }

    pub fn wait(&self) {
        unsafe {
            WaitForSingleObject(self.0, INFINITE);
        }
    }

    pub fn handle(&self) -> HANDLE {
        self.0
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub struct TransitionBarrier {
    pub resource: ID3D12Resource,
    pub subresource: u32,
    pub state_before: D3D12_RESOURCE_STATES,
    pub state_after: D3D12_RESOURCE_STATES,
}

pub fn transition_barriers<const N: usize>(
    cmd_list: &ID3D12GraphicsCommandList,
    barriers: [TransitionBarrier; N],
) {
    unsafe {
        let barriers = barriers.map(|b| D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    pResource: Some(b.resource),
                    Subresource: b.subresource,
                    StateBefore: b.state_before,
                    StateAfter: b.state_after,
                }),
            },
            ..Default::default()
        });
        cmd_list.ResourceBarrier(&barriers);
        for mut b in barriers.into_iter() {
            ManuallyDrop::drop(&mut b.Anonymous.Transition);
        }
    }
}

#[repr(transparent)]
pub struct SampleDesc(pub DXGI_SAMPLE_DESC);

impl Default for SampleDesc {
    fn default() -> Self {
        Self(DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        })
    }
}

impl From<SampleDesc> for DXGI_SAMPLE_DESC {
    fn from(src: SampleDesc) -> Self {
        src.0
    }
}

#[repr(transparent)]
pub struct HeapProperties(pub D3D12_HEAP_PROPERTIES);

impl HeapProperties {
    pub fn new(t: D3D12_HEAP_TYPE) -> Self {
        Self(D3D12_HEAP_PROPERTIES {
            Type: t,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
            ..Default::default()
        })
    }
}

impl From<HeapProperties> for D3D12_HEAP_PROPERTIES {
    fn from(src: HeapProperties) -> Self {
        src.0
    }
}

pub struct MappedBuffer<'a, T> {
    buffer: ID3D12Resource,
    ptr: *mut T,
    _lifetime: std::marker::PhantomData<&'a ()>,
}

impl<'a, T> MappedBuffer<'a, T> {
    fn new(buffer: &ID3D12Resource) -> Result<Self, Error> {
        unsafe {
            let mut ptr = std::ptr::null_mut();
            buffer.Map(0, std::ptr::null(), &mut ptr)?;
            Ok(Self {
                buffer: buffer.clone(),
                ptr: ptr as _,
                _lifetime: std::marker::PhantomData,
            })
        }
    }

    pub unsafe fn as_ref(&self) -> &'a T {
        self.ptr.as_ref().unwrap()
    }

    pub unsafe fn as_mut(&self) -> &'a mut T {
        self.ptr.as_mut().unwrap()
    }
}

impl<'a, T> Drop for MappedBuffer<'a, T> {
    fn drop(&mut self) {
        unsafe {
            self.buffer.Unmap(0, std::ptr::null());
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Buffer(ID3D12Resource);

impl Buffer {
    pub fn new(
        name: &str,
        device: &ID3D12Device,
        heap_props: HeapProperties,
        size: u64,
        init_state: D3D12_RESOURCE_STATES,
        heap_flags: Option<D3D12_HEAP_FLAGS>,
    ) -> Result<Self, Error> {
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: size,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            SampleDesc: SampleDesc::default().into(),
            ..Default::default()
        };
        unsafe {
            let mut buffer: Option<ID3D12Resource> = None;
            let buffer = device
                .CreateCommittedResource(
                    &heap_props.into(),
                    heap_flags.unwrap_or(D3D12_HEAP_FLAG_NONE),
                    &desc,
                    init_state,
                    std::ptr::null(),
                    &mut buffer,
                )
                .map(|_| buffer.unwrap())?;
            buffer.SetName(name)?;
            Ok(Self(buffer))
        }
    }

    pub fn gpu_virtual_address(&self) -> u64 {
        unsafe { self.0.GetGPUVirtualAddress() }
    }

    pub fn map<T>(&self) -> Result<MappedBuffer<'_, T>, Error> {
        MappedBuffer::new(&self.0)
    }

    pub fn handle(&self) -> &ID3D12Resource {
        &self.0
    }
}

impl From<Buffer> for ID3D12Resource {
    fn from(src: Buffer) -> ID3D12Resource {
        src.0
    }
}

impl<'a> From<&'a Buffer> for &'a ID3D12Resource {
    fn from(src: &'a Buffer) -> Self {
        &src.0
    }
}

#[derive(Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Texture2D(ID3D12Resource);

impl Texture2D {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        device: &ID3D12Device,
        width: u64,
        height: u32,
        init_state: D3D12_RESOURCE_STATES,
        heap_flags: Option<D3D12_HEAP_FLAGS>,
        flags: Option<D3D12_RESOURCE_FLAGS>,
        clear_color: &[f32; 4],
    ) -> Result<Self, Error> {
        let heap_props = HeapProperties::new(D3D12_HEAP_TYPE_DEFAULT);
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: width,
            Height: height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: flags.unwrap_or(D3D12_RESOURCE_FLAG_NONE),
            SampleDesc: SampleDesc::default().into(),
            ..Default::default()
        };
        unsafe {
            let mut resource: Option<ID3D12Resource> = None;
            let resource = device
                .CreateCommittedResource(
                    &heap_props.into(),
                    heap_flags.unwrap_or(D3D12_HEAP_FLAG_NONE),
                    &desc,
                    init_state,
                    &D3D12_CLEAR_VALUE {
                        Format: desc.Format,
                        Anonymous: D3D12_CLEAR_VALUE_0 {
                            Color: *clear_color,
                        },
                    },
                    &mut resource,
                )
                .map(|_| resource.unwrap())?;
            resource.SetName(name)?;
            Ok(Self(resource))
        }
    }

    pub fn handle(&self) -> &ID3D12Resource {
        &self.0
    }
}

impl From<Texture2D> for ID3D12Resource {
    fn from(src: Texture2D) -> Self {
        src.0
    }
}

impl<'a> From<&'a Texture2D> for &'a ID3D12Resource {
    fn from(src: &'a Texture2D) -> Self {
        &src.0
    }
}
