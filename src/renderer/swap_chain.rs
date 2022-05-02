use super::*;

pub(super) struct PresentableQueue {
    queue: CommandQueue,
    swap_chain: IDXGISwapChain4,
}

impl PresentableQueue {
    fn new(queue: CommandQueue, swap_chain: &IDXGISwapChain4) -> Self {
        Self {
            queue,
            swap_chain: swap_chain.clone(),
        }
    }

    pub fn execute<const N: usize>(&self, cmd_lists: [&CommandList; N]) -> Result<Signal, Error> {
        self.queue.execute(cmd_lists)
    }

    pub fn wait(&self, signal: &Signal) -> Result<(), Error> {
        self.queue.wait(signal)
    }

    pub fn present(&self, interval: u32) -> Result<Signal, Error> {
        unsafe {
            self.swap_chain.Present(interval, 0)?;
            self.queue.signal()
        }
    }
}

pub(super) struct SwapChain {
    swap_chain: IDXGISwapChain4,
    back_buffers: Vec<ID3D12Resource>,
    rtv_heap: ID3D12DescriptorHeap,
    rtv_size: usize,
}

impl SwapChain {
    pub fn new(
        device: &ID3D12Device,
        window: &wita::Window,
        count: usize,
    ) -> Result<(Self, PresentableQueue), Error> {
        unsafe {
            let cmd_queue = CommandQueue::new(
                "PresentableQueue::cmd_queue",
                device,
                D3D12_COMMAND_LIST_TYPE_DIRECT,
            )?;
            let window_size = window.inner_size();
            let dxgi_factory: IDXGIFactory5 = CreateDXGIFactory1()?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: window_size.width,
                Height: window_size.height,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                BufferCount: count as _,
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                Scaling: DXGI_SCALING_NONE,
                SampleDesc: SampleDesc::default().into(),
                ..Default::default()
            };
            let swap_chain: IDXGISwapChain4 = {
                dxgi_factory
                    .CreateSwapChainForHwnd(
                        cmd_queue.handle(),
                        HWND(window.raw_handle() as _),
                        &desc,
                        std::ptr::null(),
                        None,
                    )?
                    .cast()?
            };
            let rtv_heap: ID3D12DescriptorHeap =
                device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                    NumDescriptors: desc.BufferCount,
                    ..Default::default()
                })?;
            rtv_heap.SetName("SwapChain::rtv_heap")?;
            let rtv_size =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV) as usize;
            let back_buffers = Self::create_back_buffers(device, &swap_chain, &rtv_heap, rtv_size)?;
            let queue = PresentableQueue::new(cmd_queue, &swap_chain);
            Ok((
                Self {
                    swap_chain,
                    back_buffers,
                    rtv_heap,
                    rtv_size,
                },
                queue,
            ))
        }
    }

    pub fn back_buffer(&self, index: usize) -> RenderTarget {
        unsafe {
            let desc = self.swap_chain.GetDesc1().unwrap();
            let mut handle = self.rtv_heap.GetCPUDescriptorHandleForHeapStart();
            handle.ptr += self.rtv_size * index;
            RenderTarget {
                resource: self.back_buffers[index].clone(),
                handle,
                size: wita::PhysicalSize::new(desc.Width, desc.Height),
            }
        }
    }

    pub fn current_buffer(&self) -> usize {
        unsafe { self.swap_chain.GetCurrentBackBufferIndex() as usize }
    }
    pub fn resize(
        &mut self,
        device: &ID3D12Device,
        size: wita::PhysicalSize<u32>,
    ) -> Result<(), Error> {
        self.back_buffers.clear();
        unsafe {
            self.swap_chain
                .ResizeBuffers(0, size.width, size.height, DXGI_FORMAT_UNKNOWN, 0)?;
            self.back_buffers =
                Self::create_back_buffers(device, &self.swap_chain, &self.rtv_heap, self.rtv_size)?;
        }
        Ok(())
    }

    fn create_back_buffers(
        device: &ID3D12Device,
        swap_chain: &IDXGISwapChain4,
        rtv_heap: &ID3D12DescriptorHeap,
        rtv_size: usize,
    ) -> Result<Vec<ID3D12Resource>, Error> {
        unsafe {
            let desc = swap_chain.GetDesc1()?;
            let mut back_buffers = Vec::with_capacity(desc.BufferCount as _);
            let mut handle = rtv_heap.GetCPUDescriptorHandleForHeapStart();
            for i in 0..desc.BufferCount {
                let buffer: ID3D12Resource = swap_chain.GetBuffer(i)?;
                buffer.SetName(format!("SwapChain::back_buffers[{}]", i))?;
                device.CreateRenderTargetView(&buffer, std::ptr::null(), handle);
                back_buffers.push(buffer);
                handle.ptr += rtv_size;
            }
            Ok(back_buffers)
        }
    }
}
