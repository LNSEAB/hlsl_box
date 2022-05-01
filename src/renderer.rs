mod command_queue;
mod copy_texture_shader;
pub mod pixel_shader;
mod plane;
mod render_target_buffer;
mod swap_chain;
mod ui;

use crate::*;
use std::cell::{Cell, RefCell};
use windows::core::{Interface, PCSTR};
use windows::Win32::{
    Foundation::*,
    Graphics::{Direct3D::*, Direct3D12::*, Dxgi::Common::*, Dxgi::*},
};

use command_queue::*;
use copy_texture_shader::*;
pub use pixel_shader::Pipeline;
use pixel_shader::PixelShader;
use render_target_buffer::*;
use swap_chain::*;
pub use ui::RenderUi;
use ui::*;

pub struct Renderer {
    d3d12_device: ID3D12Device,
    swap_chain: SwapChain,
    render_target: RenderTargetBuffer,
    pixel_shader: PixelShader,
    cmd_allocators: Vec<ID3D12CommandAllocator>,
    cmd_list: ID3D12GraphicsCommandList,
    wait_event: Event,
    signals: RefCell<Vec<Option<Signal>>>,
    ui: Ui,
    copy_queue: CommandQueue,
    presentable_queue: PresentableQueue,
    filling_plane: plane::Buffer,
    adjusted_plane: plane::Buffer,
}

impl Renderer {
    const ALLOCATORS_PER_FRAME: usize = 2;
    const BUFFER_COUNT: usize = 2;

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
            let copy_queue = CommandQueue::new(
                "Renderer::copy_queue",
                d3d12_device,
                D3D12_COMMAND_LIST_TYPE_COPY,
            )?;
            let cmd_list: ID3D12GraphicsCommandList = d3d12_device.CreateCommandList(
                0,
                D3D12_COMMAND_LIST_TYPE_DIRECT,
                &cmd_allocators[0],
                None,
            )?;
            cmd_list.SetName("Renderer::cmd_lists")?;
            cmd_list.Close()?;
            let copy_texture = CopyTextureShader::new(d3d12_device, compiler, shader_model)?;
            let render_target = RenderTargetBuffer::new(
                d3d12_device,
                resolution,
                copy_texture.clone(),
                Self::BUFFER_COUNT,
                clear_color,
            )?;
            let pixel_shader = PixelShader::new(d3d12_device, compiler, shader_model)?;
            let ui = Ui::new(d3d12_device, Self::BUFFER_COUNT, window, copy_texture)?;
            let filling_plane = plane::Buffer::new(&d3d12_device, &copy_queue)?;
            let adjusted_plane = plane::Buffer::new(&d3d12_device, &copy_queue)?;
            Ok(Self {
                d3d12_device: d3d12_device.clone(),
                swap_chain,
                render_target,
                pixel_shader,
                cmd_allocators,
                cmd_list,
                wait_event: Event::new()?,
                signals: RefCell::new(vec![None; 2]),
                ui,
                copy_queue,
                presentable_queue,
                filling_plane,
                adjusted_plane,
            })
        }
    }

    pub fn mltg_factory(&self) -> mltg::Factory {
        self.ui.context.create_factory()
    }

    pub fn create_pixel_shader_pipeline(&self, ps: &hlsl::Blob) -> Result<Pipeline, Error> {
        self.pixel_shader.create_pipeline(&self.d3d12_device, ps)
    }

    pub fn render(
        &self,
        interval: u32,
        clear_color: &[f32; 4],
        ps: Option<&Pipeline>,
        parameters: Option<&pixel_shader::Parameters>,
        r: &impl RenderUi,
    ) -> Result<(), Error> {
        let index = self.swap_chain.current_buffer();
        if let Some(signal) = self.signals.borrow_mut()[index].take() {
            if !signal.is_completed() {
                signal.set_event(&self.wait_event)?;
                self.wait_event.wait();
            }
        }
        let current_index = index * Self::ALLOCATORS_PER_FRAME;
        let cmd_allocators =
            &self.cmd_allocators[current_index..current_index + Self::ALLOCATORS_PER_FRAME];
        unsafe {
            cmd_allocators[0].Reset()?;
            self.cmd_list.Reset(&cmd_allocators[0], None)?;
            self.render_target
                .set_target(index, &self.cmd_list, clear_color);
            if let Some(ps) = ps {
                if let Some(parameters) = parameters {
                    self.pixel_shader
                        .execute(&self.cmd_list, ps, parameters, &self.filling_plane);
                }
            }
            self.swap_chain.begin(index, &self.cmd_list, clear_color);
            self.swap_chain.set_target(index, &self.cmd_list);
            self.render_target
                .copy(index, &self.cmd_list, &self.adjusted_plane);
            self.cmd_list.Close()?;
            self.presentable_queue
                .execute_command_lists(&[Some(self.cmd_list.cast().unwrap())])?;

            cmd_allocators[1].Reset()?;
            self.cmd_list.Reset(&cmd_allocators[1], None)?;
            self.swap_chain.set_target(index, &self.cmd_list);
            self.ui.copy(index, &self.cmd_list, &self.adjusted_plane);
            self.swap_chain.end(index, &self.cmd_list);
            self.cmd_list.Close()?;
            let ui_signal = self.ui.render(index, r)?;
            self.presentable_queue.wait(&ui_signal)?;
            self.presentable_queue
                .execute_command_lists(&[Some(self.cmd_list.cast().unwrap())])?;
        }
        let signal = self.presentable_queue.present(interval)?;
        self.signals.borrow_mut()[index] = Some(signal);
        Ok(())
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
        let size = size.cast::<f32>();
        let resolution = self.render_target.size().cast::<f32>();
        let aspect_size = size.width / size.height;
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
        let s = wita::PhysicalSize::new((size.width * s[0]) as u32, (size.height * s[1]) as u32);
        self.ui.resize(&self.d3d12_device, s)?;
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
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.wait_all_signals();
    }
}
