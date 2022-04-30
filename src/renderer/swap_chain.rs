use super::*;

pub struct SwapChain {
    pub cmd_queue: CommandQueue,
    swap_chain: IDXGISwapChain4,
    back_buffers: Vec<ID3D12Resource>,
    rtv_heap: ID3D12DescriptorHeap,
    rtv_size: usize,
}

impl SwapChain {
    pub fn new(device: &ID3D12Device, window: &wita::Window, count: usize) -> anyhow::Result<Self> {
        unsafe {
            let cmd_queue = CommandQueue::new(
                "SwapChain::cmd_queue",
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
            Ok(Self {
                cmd_queue,
                swap_chain,
                back_buffers,
                rtv_heap,
                rtv_size,
            })
        }
    }

    pub fn current_buffer(&self) -> usize {
        unsafe { self.swap_chain.GetCurrentBackBufferIndex() as usize }
    }

    pub fn begin(&self, index: usize, cmd_list: &ID3D12GraphicsCommandList, clear_color: &[f32; 4]) {
        transition_barriers(
            cmd_list,
            [TransitionBarrier {
                resource: self.back_buffers[index].clone(),
                subresource: 0,
                state_before: D3D12_RESOURCE_STATE_PRESENT,
                state_after: D3D12_RESOURCE_STATE_RENDER_TARGET,
            }],
        );
        unsafe {
            let mut handle = self.rtv_heap.GetCPUDescriptorHandleForHeapStart();
            handle.ptr += self.rtv_size * index;
            cmd_list.ClearRenderTargetView(handle, clear_color.as_ptr(), &[]);
        }
    }

    pub fn set_target(&self, index: usize, cmd_list: &ID3D12GraphicsCommandList) {
        unsafe {
            let mut handle = self.rtv_heap.GetCPUDescriptorHandleForHeapStart();
            handle.ptr += self.rtv_size * index;
            let desc = self.swap_chain.GetDesc1().unwrap();
            let rtvs = [handle];
            cmd_list.RSSetViewports(&[D3D12_VIEWPORT {
                Width: desc.Width as _,
                Height: desc.Height as _,
                MaxDepth: 1.0,
                ..Default::default()
            }]);
            cmd_list.RSSetScissorRects(&[RECT {
                right: desc.Width as _,
                bottom: desc.Height as _,
                ..Default::default()
            }]);
            cmd_list.OMSetRenderTargets(rtvs.len() as _, rtvs.as_ptr(), false, std::ptr::null());
        }
    }

    pub fn end(&self, index: usize, cmd_list: &ID3D12GraphicsCommandList) {
        transition_barriers(
            cmd_list,
            [TransitionBarrier {
                resource: self.back_buffers[index].clone(),
                subresource: 0,
                state_before: D3D12_RESOURCE_STATE_RENDER_TARGET,
                state_after: D3D12_RESOURCE_STATE_PRESENT,
            }],
        );
    }

    pub fn present(&self, interval: u32) -> anyhow::Result<Signal> {
        unsafe {
            self.swap_chain.Present(interval, 0)?;
            self.cmd_queue.signal()
        }
    }

    pub fn resize(
        &mut self,
        device: &ID3D12Device,
        size: wita::PhysicalSize<u32>,
    ) -> anyhow::Result<()> {
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
    ) -> anyhow::Result<Vec<ID3D12Resource>> {
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