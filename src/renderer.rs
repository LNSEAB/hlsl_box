mod command_list;
mod command_queue;
mod layer_shader;
pub mod pixel_shader;
mod plane;
mod read_back_buffer;
mod render_target;
mod swap_chain;
mod ui;
mod utility;

use crate::*;
use std::cell::{Cell, RefCell};
use windows::core::{Interface, PCSTR};
use windows::Win32::{
    Foundation::*,
    Graphics::{Direct3D::*, Direct3D12::*, Dxgi::Common::*, Dxgi::*},
};

use command_list::*;
use command_queue::*;
use layer_shader::*;
pub use pixel_shader::Pipeline;
use pixel_shader::PixelShader;
use read_back_buffer::*;
use render_target::*;
use swap_chain::*;
pub use ui::RenderUi;
use ui::*;
use utility::*;

trait Target {
    fn enter(&self) -> TransitionBarrier;
    fn leave(&self) -> TransitionBarrier;
    fn clear(&self, cmd_list: &ID3D12GraphicsCommandList, clear_color: [f32; 4]);
    fn record(&self, cmd_list: &ID3D12GraphicsCommandList);
}

trait Source {
    fn enter(&self) -> TransitionBarrier;
    fn leave(&self) -> TransitionBarrier;
    fn record(&self, cmd_list: &ID3D12GraphicsCommandList);
}

trait Shader {
    fn record(&self, cmd_list: &ID3D12GraphicsCommandList);
}

trait CopySource {
    fn enter(&self) -> TransitionBarrier;
    fn leave(&self) -> TransitionBarrier;
    fn record(&self, cmd_lsit: &ID3D12GraphicsCommandList, dest: &ReadBackBuffer);
}

pub struct RenderTarget {
    resource: ID3D12Resource,
    handle: D3D12_CPU_DESCRIPTOR_HANDLE,
    size: wita::PhysicalSize<u32>,
}

impl Target for RenderTarget {
    fn enter(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource.clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_COMMON,
            state_after: D3D12_RESOURCE_STATE_RENDER_TARGET,
        }
    }

    fn leave(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource.clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_RENDER_TARGET,
            state_after: D3D12_RESOURCE_STATE_COMMON,
        }
    }

    fn clear(&self, cmd_list: &ID3D12GraphicsCommandList, clear_color: [f32; 4]) {
        unsafe {
            cmd_list.ClearRenderTargetView(self.handle, clear_color.as_ptr(), &[]);
        }
    }

    fn record(&self, cmd_list: &ID3D12GraphicsCommandList) {
        unsafe {
            cmd_list.RSSetViewports(&[D3D12_VIEWPORT {
                Width: self.size.width as _,
                Height: self.size.height as _,
                MaxDepth: 1.0,
                ..Default::default()
            }]);
            cmd_list.RSSetScissorRects(&[RECT {
                right: self.size.width as _,
                bottom: self.size.height as _,
                ..Default::default()
            }]);
            cmd_list.OMSetRenderTargets(1, [self.handle].as_ptr(), false, std::ptr::null());
        }
    }
}

pub struct CopyResource {
    resource: ID3D12Resource,
}

impl CopySource for CopyResource {
    fn enter(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource.clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_COMMON,
            state_after: D3D12_RESOURCE_STATE_COPY_SOURCE,
        }
    }

    fn leave(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource.clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_COPY_SOURCE,
            state_after: D3D12_RESOURCE_STATE_COMMON,
        }
    }

    fn record(&self, cmd_list: &ID3D12GraphicsCommandList, dest: &ReadBackBuffer) {
        unsafe {
            let device = {
                let mut device: Option<ID3D12Device> = None;
                cmd_list
                    .GetDevice(&mut device)
                    .map(|_| device.unwrap())
                    .unwrap()
            };
            let desc = self.resource.GetDesc();
            let mut foot_print = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
            device.GetCopyableFootprints(
                &desc,
                0,
                1,
                0,
                &mut foot_print,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            let copy_src = D3D12_TEXTURE_COPY_LOCATION {
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                pResource: Some(self.resource.clone()),
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };
            let copy_dest = D3D12_TEXTURE_COPY_LOCATION {
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                pResource: Some(dest.resource().clone()),
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: foot_print,
                },
            };
            cmd_list.CopyTextureRegion(&copy_dest, 0, 0, 0, &copy_src, std::ptr::null());
        }
    }
}

pub struct ShaderResource {
    resource: ID3D12Resource,
    heap: ID3D12DescriptorHeap,
    handle: D3D12_GPU_DESCRIPTOR_HANDLE,
}

impl Source for ShaderResource {
    fn enter(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource.clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_COMMON,
            state_after: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
        }
    }

    fn leave(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource.clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            state_after: D3D12_RESOURCE_STATE_COMMON,
        }
    }

    fn record(&self, cmd_list: &ID3D12GraphicsCommandList) {
        unsafe {
            cmd_list.SetDescriptorHeaps(&[Some(self.heap.clone())]);
            cmd_list.SetGraphicsRootDescriptorTable(0, self.handle);
        }
    }
}

pub struct Renderer {
    d3d12_device: ID3D12Device,
    swap_chain: SwapChain,
    render_target: RenderTargetBuffers,
    pixel_shader: PixelShader,
    cmd_allocators: Vec<ID3D12CommandAllocator>,
    cmd_list: DirectCommandList,
    signals: Signals,
    ui: Ui,
    copy_queue: CommandQueue<CopyCommandList>,
    main_queue: PresentableQueue,
    filling_plane: plane::Buffer,
    adjusted_plane: plane::Buffer,
    read_back_buffer: ReadBackBuffer,
}

impl Renderer {
    const ALLOCATORS_PER_FRAME: usize = 2;
    const BUFFER_COUNT: usize = 2;
    const COPY_ALLOCATOR: usize = Self::ALLOCATORS_PER_FRAME * Self::BUFFER_COUNT;

    pub fn new(
        d3d12_device: &ID3D12Device,
        window: &wita::Window,
        resolution: wita::PhysicalSize<u32>,
        compiler: &hlsl::Compiler,
        shader_model: hlsl::ShaderModel,
        clear_color: &[f32; 4],
    ) -> Result<Self, Error> {
        unsafe {
            let (swap_chain, presentable_queue) =
                SwapChain::new(d3d12_device, window, Self::BUFFER_COUNT)?;
            let mut cmd_allocators =
                Vec::with_capacity(Self::BUFFER_COUNT * Self::ALLOCATORS_PER_FRAME);
            for i in 0..Self::BUFFER_COUNT * Self::ALLOCATORS_PER_FRAME {
                let cmd_allocator: ID3D12CommandAllocator =
                    d3d12_device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?;
                cmd_allocator.SetName(format!("Renderer::cmd_allocators[{}]", i))?;
                cmd_allocators.push(cmd_allocator);
            }
            cmd_allocators.push(d3d12_device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_COPY)?);
            cmd_allocators[Self::COPY_ALLOCATOR].SetName(format!(
                "Renderer::cmd_allocators[{}]",
                Self::COPY_ALLOCATOR
            ))?;
            let copy_queue = CommandQueue::new(
                "Renderer::copy_queue",
                d3d12_device,
            )?;
            let render_target = RenderTargetBuffers::new(
                d3d12_device,
                resolution,
                Self::BUFFER_COUNT,
                clear_color,
            )?;
            let pixel_shader = PixelShader::new(d3d12_device, compiler, shader_model)?;
            let ui = Ui::new(d3d12_device, Self::BUFFER_COUNT, window)?;
            let filling_plane = plane::Buffer::new(d3d12_device, &copy_queue)?;
            let adjusted_plane = plane::Buffer::new(d3d12_device, &copy_queue)?;
            let layer_shader = LayerShader::new(d3d12_device, compiler, shader_model)?;
            let cmd_list = DirectCommandList::new(
                "Renderer::cmd_list",
                d3d12_device,
                &cmd_allocators[0],
                layer_shader,
            )?;
            let signals = Signals::new(cmd_allocators.len());
            let read_back_buffer = ReadBackBuffer::new(d3d12_device, resolution)?;
            Ok(Self {
                d3d12_device: d3d12_device.clone(),
                swap_chain,
                render_target,
                pixel_shader,
                cmd_allocators,
                cmd_list,
                signals,
                ui,
                copy_queue,
                main_queue: presentable_queue,
                filling_plane,
                adjusted_plane,
                read_back_buffer,
            })
        }
    }

    pub fn mltg_factory(&self) -> mltg::Factory {
        self.ui.create_factory()
    }

    pub fn create_pixel_shader_pipeline(
        &self,
        name: &str,
        ps: &hlsl::Blob,
    ) -> Result<Pipeline, Error> {
        self.pixel_shader
            .create_pipeline(name, &self.d3d12_device, ps)
    }

    pub fn render(
        &self,
        interval: u32,
        clear_color: [f32; 4],
        ps: Option<&Pipeline>,
        parameters: Option<&pixel_shader::Parameters>,
        r: &impl RenderUi,
    ) -> Result<(), Error> {
        let index = self.swap_chain.current_buffer();
        self.signals.wait(index);
        let current_index = index * Self::ALLOCATORS_PER_FRAME;
        let cmd_allocators =
            &self.cmd_allocators[current_index..current_index + Self::ALLOCATORS_PER_FRAME];
        let ps_result = self.render_target.source(index);
        let back_buffer = self.swap_chain.back_buffer(index);
        let ui_buffer = self.ui.source(index);
        let cmd_list = &self.cmd_list;
        cmd_list.record(&cmd_allocators[0], |cmd| {
            if let Some(ps) = ps {
                if let Some(parameters) = parameters {
                    let shader = self.pixel_shader.apply(ps, parameters);
                    let target = self.render_target.target(index);
                    cmd.barrier([target.enter()]);
                    cmd.clear(&target, clear_color);
                    cmd.draw(&shader, &target, &self.filling_plane);
                    cmd.barrier([target.leave()]);
                }
            }
            cmd.barrier([ps_result.enter(), back_buffer.enter()]);
            cmd.clear(&back_buffer, clear_color);
            cmd.layer(&ps_result, &back_buffer, &self.adjusted_plane);
        })?;
        self.main_queue.execute([cmd_list])?;
        cmd_list.record(&cmd_allocators[1], |cmd| {
            cmd.barrier([ui_buffer.enter()]);
            cmd.layer(&ui_buffer, &back_buffer, &self.filling_plane);
            cmd.barrier([ps_result.leave(), back_buffer.leave(), ui_buffer.leave()]);
        })?;
        let ui_signal = self.ui.render(index, r)?;
        self.main_queue.wait(&ui_signal)?;
        self.main_queue.execute([cmd_list])?;
        let signal = self.main_queue.present(interval)?;
        self.signals.set(index, signal);
        Ok(())
    }

    pub fn wait_all_signals(&self) {
        self.signals.wait_all();
    }

    pub fn screen_shot(&self) -> anyhow::Result<Option<image::RgbaImage>> {
        let index = self.signals.last_frame_index();
        if index.is_none() {
            return Ok(None);
        }
        let index = index.unwrap();
        let cmd_list = CopyCommandList::new(
            "Renderer::screen_shot",
            &self.d3d12_device,
            &self.cmd_allocators[Self::COPY_ALLOCATOR],
        )?;
        let src = self.render_target.copy_resource(index);
        cmd_list.record(&self.cmd_allocators[Self::COPY_ALLOCATOR], |cmd| {
            cmd.barrier([src.enter()]);
            cmd.copy(&src, &self.read_back_buffer);
            cmd.barrier([src.leave()]);
        })?;
        self.copy_queue.execute([&cmd_list])?.wait()?;
        let img = self.read_back_buffer.to_image()?;
        Ok(Some(img))
    }

    pub fn resize(&mut self, size: wita::PhysicalSize<u32>) -> Result<(), Error> {
        self.wait_all_signals();
        self.swap_chain.resize(&self.d3d12_device, size)?;
        self.ui.resize(&self.d3d12_device, size)?;
        Ok(())
    }

    pub fn change_dpi(&mut self, dpi: u32) -> Result<(), Error> {
        self.ui.change_dpi(dpi)?;
        Ok(())
    }

    pub fn restore(&mut self, size: wita::PhysicalSize<u32>) -> Result<(), Error> {
        self.wait_all_signals();
        self.swap_chain.resize(&self.d3d12_device, size)?;
        self.ui.resize(&self.d3d12_device, size)?;
        self.adjusted_plane.replace(
            &self.d3d12_device,
            &self.copy_queue,
            &plane::Meshes::new(1.0, 1.0),
        )?;
        Ok(())
    }

    pub fn maximize(&mut self, size: wita::PhysicalSize<u32>) -> Result<(), Error> {
        self.wait_all_signals();
        self.swap_chain.resize(&self.d3d12_device, size)?;
        let size_f = size.cast::<f32>();
        let resolution = self.render_target.size().cast::<f32>();
        let aspect_size = size_f.width / size_f.height;
        let aspect_resolution = resolution.width / resolution.height;
        let s = if aspect_resolution > aspect_size {
            [1.0, aspect_size / aspect_resolution]
        } else {
            [aspect_resolution / aspect_size, 1.0]
        };
        self.adjusted_plane.replace(
            &self.d3d12_device,
            &self.copy_queue,
            &plane::Meshes::new(s[0], s[1]),
        )?;
        self.ui.resize(&self.d3d12_device, size)?;
        Ok(())
    }

    pub fn recreate(
        &mut self,
        resolution: settings::Resolution,
        compiler: &hlsl::Compiler,
        shader_model: hlsl::ShaderModel,
        clear_color: [f32; 4],
    ) -> Result<(), Error> {
        self.wait_all_signals();
        let render_target = RenderTargetBuffers::new(
            &self.d3d12_device,
            wita::PhysicalSize::new(resolution.width, resolution.height),
            Self::BUFFER_COUNT,
            &clear_color,
        )?;
        let pixel_shader = PixelShader::new(&self.d3d12_device, compiler, shader_model)?;
        let layer_shader = LayerShader::new(&self.d3d12_device, compiler, shader_model)?;
        let cmd_list = DirectCommandList::new(
            "Renderer::cmd_list",
            &self.d3d12_device,
            &self.cmd_allocators[0],
            layer_shader,
        )?;
        self.render_target = render_target;
        self.pixel_shader = pixel_shader;
        self.cmd_list = cmd_list;
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.wait_all_signals();
    }
}
