use super::*;

pub trait RenderUi {
    fn render(&self, cmd: &mltg::DrawCommand);
}

pub struct Ui {
    pub context: mltg::Context<mltg::Direct3D12>,
    cmd_queue: CommandQueue,
    desc_heap: ID3D12DescriptorHeap,
    desc_size: usize,
    buffers: Vec<(Texture2D, mltg::d3d12::RenderTarget)>,
    copy_texture: CopyTextureShader,
    signals: RefCell<Vec<Option<Signal>>>,
    wait_event: Event,
}

impl Ui {
    pub fn new(
        device: &ID3D12Device,
        count: usize,
        window: &wita::Window,
        copy_texture: CopyTextureShader,
    ) -> anyhow::Result<Self> {
        unsafe {
            let size = window.inner_size();
            let cmd_queue = CommandQueue::new("Ui", device, D3D12_COMMAND_LIST_TYPE_DIRECT)?;
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
            let signals = RefCell::new(vec![None; count]);
            Ok(Self {
                context,
                cmd_queue,
                desc_heap,
                desc_size,
                buffers,
                copy_texture,
                signals,
                wait_event: Event::new()?,
            })
        }
    }

    pub fn render(&self, index: usize, r: &impl RenderUi) -> anyhow::Result<Signal> {
        let buffer = &self.buffers[index];
        self.context.draw(&buffer.1, |cmd| {
            cmd.clear([0.0, 0.0, 0.0, 0.0]);
            r.render(cmd);
        })?;
        let signal = self.cmd_queue.signal()?;
        self.signals.borrow_mut()[index] = Some(signal.clone());
        Ok(signal)
    }

    pub fn copy(&self, index: usize, cmd_list: &ID3D12GraphicsCommandList) {
        let buffer = &self.buffers[index];
        unsafe {
            let mut srv_handle = self.desc_heap.GetGPUDescriptorHandleForHeapStart();
            srv_handle.ptr += (index * self.desc_size) as u64;
            transition_barriers(
                cmd_list,
                [TransitionBarrier {
                    resource: buffer.0.handle().clone(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_COMMON,
                    state_after: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                }],
            );
            cmd_list.SetDescriptorHeaps(&[Some(self.desc_heap.clone())]);
            cmd_list.SetGraphicsRootSignature(&self.copy_texture.root_signature);
            cmd_list.SetPipelineState(&self.copy_texture.pipeline);
            cmd_list.SetGraphicsRootDescriptorTable(0, srv_handle);
            cmd_list.IASetVertexBuffers(0, &[self.copy_texture.plane.vbv]);
            cmd_list.IASetIndexBuffer(&self.copy_texture.plane.ibv);
            cmd_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            cmd_list.DrawIndexedInstanced(self.copy_texture.plane.indices_len() as _, 1, 0, 0, 0);
            transition_barriers(
                cmd_list,
                [TransitionBarrier {
                    resource: buffer.0.handle().clone(),
                    subresource: 0,
                    state_before: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                    state_after: D3D12_RESOURCE_STATE_COMMON,
                }],
            );
        }
    }

    pub fn resize(
        &mut self,
        device: &ID3D12Device,
        size: wita::PhysicalSize<u32>,
    ) -> anyhow::Result<()> {
        let len = self.buffers.len();
        self.wait_all_signals();
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

    pub fn change_dpi(&self, dpi: u32) -> anyhow::Result<()> {
        self.context.set_dpi(dpi as _);
        Ok(())
    }

    pub fn wait_all_signals(&self) {
        for signal in self.signals.borrow().iter().flatten() {
            if !signal.is_completed() {
                signal.set_event(&self.wait_event).unwrap();
                self.wait_event.wait();
            }
        }
    }

    fn create_buffers(
        device: &ID3D12Device,
        context: &mltg::Context<mltg::Direct3D12>,
        desc_heap: &ID3D12DescriptorHeap,
        desc_size: usize,
        count: usize,
        size: wita::PhysicalSize<u32>,
        buffers: &mut Vec<(Texture2D, mltg::d3d12::RenderTarget)>,
    ) -> anyhow::Result<()> {
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
        self.wait_all_signals();
    }
}