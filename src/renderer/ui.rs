use super::*;

pub trait RenderUi {
    fn render(&self, cmd: &mltg::DrawCommand, size: wita::LogicalSize<f32>);
}

pub struct Ui {
    context: mltg::Context<mltg::Direct3D12>,
    window: wita::Window,
    cmd_queue: CommandQueue<DirectCommandList>,
    desc_heap: ID3D12DescriptorHeap,
    desc_size: usize,
    buffers: Vec<(Texture2D, mltg::d3d12::RenderTarget)>,
    signals: Signals,
}

impl Ui {
    pub fn new(device: &ID3D12Device, count: usize, window: &wita::Window) -> Result<Self, Error> {
        unsafe {
            let size = window.inner_size();
            let cmd_queue = CommandQueue::new("Ui", device)?;
            let context = mltg::Context::new(mltg::Direct3D12::new(device, cmd_queue.handle())?)?;
            context.set_dpi(window.dpi() as _);
            let desc_heap: ID3D12DescriptorHeap =
                device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                    NumDescriptors: count as _,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                    ..Default::default()
                })?;
            desc_heap.SetName("Ui::desc_heap")?;
            let desc_size = device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV)
                as usize;
            let mut buffers = Vec::with_capacity(count);
            Self::create_buffers(
                device,
                &context,
                &desc_heap,
                desc_size,
                count,
                size,
                &mut buffers,
            )?;
            let signals = Signals::new(count);
            Ok(Self {
                context,
                window: window.clone(),
                cmd_queue,
                desc_heap,
                desc_size,
                buffers,
                signals,
            })
        }
    }

    pub fn render(&self, index: usize, r: &impl RenderUi) -> Result<Signal, Error> {
        let buffer = &self.buffers[index];
        let size = self
            .window
            .inner_size()
            .to_logical(self.window.dpi() as _)
            .cast::<f32>();
        self.context.draw(&buffer.1, |cmd| {
            cmd.clear([0.0, 0.0, 0.0, 0.0]);
            r.render(cmd, size);
        })?;
        let signal = self.cmd_queue.signal()?;
        self.signals.set(index, signal.clone());
        Ok(signal)
    }

    pub fn source(&self, index: usize) -> ShaderResource {
        unsafe {
            let mut handle = self.desc_heap.GetGPUDescriptorHandleForHeapStart();
            handle.ptr += (index * self.desc_size) as u64;
            ShaderResource {
                resource: self.buffers[index].0.handle().clone(),
                heap: self.desc_heap.clone(),
                handle,
            }
        }
    }

    pub fn resize(
        &mut self,
        device: &ID3D12Device,
        size: wita::PhysicalSize<u32>,
    ) -> Result<(), Error> {
        let len = self.buffers.len();
        self.signals.wait_all();
        self.buffers.clear();
        self.context.flush();
        Self::create_buffers(
            device,
            &self.context,
            &self.desc_heap,
            self.desc_size,
            len,
            size,
            &mut self.buffers,
        )?;
        Ok(())
    }

    pub fn create_factory(&self) -> mltg::Factory {
        self.context.create_factory()
    }

    pub fn change_dpi(&self, dpi: u32) -> Result<(), Error> {
        self.context.set_dpi(dpi as _);
        Ok(())
    }

    fn create_buffers(
        device: &ID3D12Device,
        context: &mltg::Context<mltg::Direct3D12>,
        desc_heap: &ID3D12DescriptorHeap,
        desc_size: usize,
        count: usize,
        size: wita::PhysicalSize<u32>,
        buffers: &mut Vec<(Texture2D, mltg::d3d12::RenderTarget)>,
    ) -> Result<(), Error> {
        unsafe {
            let mut handle = desc_heap.GetCPUDescriptorHandleForHeapStart();
            for i in 0..count {
                let buffer = Texture2D::new(
                    &format!("Ui::buffers[{}]", i),
                    device,
                    size.width as _,
                    size.height as _,
                    D3D12_RESOURCE_STATE_COMMON,
                    None,
                    Some(
                        D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET
                            | D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
                    ),
                    &[0.0, 0.0, 0.0, 0.0],
                )?;
                buffer.handle().SetName(format!("Ui::buffer[{}]", i))?;
                let srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
                    ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                    Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                        Texture2D: D3D12_TEX2D_SRV {
                            MipLevels: 1,
                            ..Default::default()
                        },
                    },
                };
                device.CreateShaderResourceView(buffer.handle(), &srv_desc, handle);
                let target = context.create_render_target(&buffer)?;
                buffers.push((buffer, target));
                handle.ptr += desc_size;
            }
            Ok(())
        }
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        self.signals.wait_all();
    }
}
