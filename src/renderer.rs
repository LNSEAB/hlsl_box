mod buffers;
mod command_list;
mod command_queue;
mod layer_shader;
pub mod pixel_shader;
mod plane;
mod swap_chain;
mod ui;
mod utility;
mod video;

use crate::*;
use std::cell::{Cell, RefCell};
use std::path::Path;
use windows::core::{Interface, PCSTR};
use windows::Win32::{
    Foundation::*,
    Graphics::{Direct3D::*, Direct3D12::*, Dxgi::Common::*, Dxgi::*},
};

use buffers::*;
use command_list::*;
use command_queue::*;
use layer_shader::*;
pub use pixel_shader::Pipeline;
use pixel_shader::PixelShader;
use swap_chain::*;
pub use ui::RenderUi;
use ui::*;
use utility::*;

trait Resource {
    fn resource(&self) -> &ID3D12Resource;
}

trait Target: Resource {
    fn enter(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource().clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_COMMON,
            state_after: D3D12_RESOURCE_STATE_RENDER_TARGET,
        }
    }

    fn leave(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource().clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_RENDER_TARGET,
            state_after: D3D12_RESOURCE_STATE_COMMON,
        }
    }

    fn clear(&self, cmd_list: &ID3D12GraphicsCommandList, clear_color: [f32; 4]);
    fn record(&self, cmd_list: &ID3D12GraphicsCommandList);
}

trait Source: Resource {
    fn enter(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource().clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_COMMON,
            state_after: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
        }
    }

    fn leave(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource().clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            state_after: D3D12_RESOURCE_STATE_COMMON,
        }
    }

    fn record(&self, cmd_list: &ID3D12GraphicsCommandList);
}

trait CopySource: Resource {
    fn enter(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource().clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_COMMON,
            state_after: D3D12_RESOURCE_STATE_COPY_SOURCE,
        }
    }

    fn leave(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource().clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_COPY_SOURCE,
            state_after: D3D12_RESOURCE_STATE_COMMON,
        }
    }
}

trait CopyDest: Resource {
    fn enter(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource().clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_COMMON,
            state_after: D3D12_RESOURCE_STATE_COPY_DEST,
        }
    }

    fn leave(&self) -> TransitionBarrier {
        TransitionBarrier {
            resource: self.resource().clone(),
            subresource: 0,
            state_before: D3D12_RESOURCE_STATE_COPY_DEST,
            state_after: D3D12_RESOURCE_STATE_COMMON,
        }
    }
}

pub struct RenderTarget {
    resource: ID3D12Resource,
    handle: D3D12_CPU_DESCRIPTOR_HANDLE,
    size: wita::PhysicalSize<u32>,
}

impl Resource for RenderTarget {
    fn resource(&self) -> &ID3D12Resource {
        &self.resource
    }
}

impl Target for RenderTarget {
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

impl Resource for CopyResource {
    fn resource(&self) -> &ID3D12Resource {
        &self.resource
    }
}

impl CopySource for CopyResource {}

pub struct PixelShaderResource {
    resource: ID3D12Resource,
    heap: ID3D12DescriptorHeap,
    handle: D3D12_GPU_DESCRIPTOR_HANDLE,
}

impl Resource for PixelShaderResource {
    fn resource(&self) -> &ID3D12Resource {
        &self.resource
    }
}

impl Source for PixelShaderResource {
    fn record(&self, cmd_list: &ID3D12GraphicsCommandList) {
        unsafe {
            cmd_list.SetDescriptorHeaps(&[Some(self.heap.clone())]);
            cmd_list.SetGraphicsRootDescriptorTable(0, self.handle);
        }
    }
}

trait TargetableBuffers {
    fn len(&self) -> usize;
    fn target(&self, index: usize) -> RenderTarget;
}

trait PixelShaderResourceBuffers {
    fn len(&self) -> usize;
    fn source(&self, index: usize) -> PixelShaderResource;
}

trait Shader {
    fn record(&self, cmd_list: &ID3D12GraphicsCommandList);
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
    read_back_buffers: Arc<Pool<ReadBackBuffer>>,
    video: video::Video,
}

impl Renderer {
    const ALLOCATORS_PER_FRAME: usize = 2;
    const BUFFER_COUNT: usize = 2;
    const COPY_ALLOCATOR: usize = Self::ALLOCATORS_PER_FRAME * Self::BUFFER_COUNT;
    const READ_BACK_BUFFER_COUNT: usize = 3;

    pub async fn new(
        d3d12_device: &ID3D12Device,
        window: &wita::Window,
        resolution: wita::PhysicalSize<u32>,
        compiler: &hlsl::Compiler,
        shader_model: hlsl::ShaderModel,
    ) -> anyhow::Result<Self> {
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
            let copy_queue = CommandQueue::new("Renderer::copy_queue", d3d12_device)?;
            let render_target =
                RenderTargetBuffers::new(d3d12_device, resolution, Self::BUFFER_COUNT)?;
            let pixel_shader = PixelShader::new(d3d12_device, compiler, shader_model)?;
            let ui = Ui::new(d3d12_device, Self::BUFFER_COUNT, window)?;
            let filling_plane = plane::Buffer::new(d3d12_device, &copy_queue).await?;
            let adjusted_plane = plane::Buffer::new(d3d12_device, &copy_queue).await?;
            let layer_shader = LayerShader::new(d3d12_device, compiler, shader_model)?;
            let cmd_list = DirectCommandList::new(
                "Renderer::cmd_list",
                d3d12_device,
                &cmd_allocators[0],
                layer_shader,
            )?;
            let signals = Signals::new(cmd_allocators.len());
            let read_back_buffers = Pool::with_initializer(Self::READ_BACK_BUFFER_COUNT, || {
                ReadBackBuffer::new(d3d12_device, resolution)
            })?;
            let video = video::Video::new()?;
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
                read_back_buffers,
                video,
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

    pub async fn render(
        &self,
        interval: u32,
        clear_color: [f32; 4],
        ps: Option<&Pipeline>,
        parameters: Option<&pixel_shader::Parameters>,
        r: &impl RenderUi,
    ) -> anyhow::Result<()> {
        let index = self.swap_chain.current_buffer();
        self.signals.wait(index).await;
        let current_index = index * Self::ALLOCATORS_PER_FRAME;
        let cmd_allocators =
            &self.cmd_allocators[current_index..current_index + Self::ALLOCATORS_PER_FRAME];
        let ps_result = self.render_target.source(index);
        let back_buffer = self.swap_chain.target(index);
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
        let main_signal = self.main_queue.execute([cmd_list])?;
        let mut copy_signal = None;
        if self.video.signal() {
            let read_back_buffer = self.read_back_buffers.pop().await;
            let cmd_list = CopyCommandList::new(
                "Renderer::render write video",
                &self.d3d12_device,
                &self.cmd_allocators[Self::COPY_ALLOCATOR],
            )?;
            let src = self.render_target.copy_resource(index);
            cmd_list.record(
                &self.cmd_allocators[Self::COPY_ALLOCATOR],
                |cmd: CopyCommand<CopyResource, ReadBackBuffer>| {
                    cmd.barrier([src.enter()]);
                    cmd.copy(&src, &*read_back_buffer);
                    cmd.barrier([src.leave()]);
                },
            )?;
            self.copy_queue.wait(&main_signal)?;
            let signal = self.copy_queue.execute([&cmd_list])?;
            copy_signal = Some(signal.clone());
            self.video.write(read_back_buffer, signal)?;
        }
        cmd_list.record(&cmd_allocators[1], |cmd| {
            cmd.barrier([ui_buffer.enter()]);
            cmd.layer(&ui_buffer, &back_buffer, &self.filling_plane);
            cmd.barrier([ps_result.leave(), back_buffer.leave(), ui_buffer.leave()]);
        })?;
        let ui_signal = self.ui.render(index, r)?;
        self.main_queue.wait(&ui_signal)?;
        self.main_queue.execute([cmd_list])?;
        let signal = self.main_queue.present(interval)?;
        if let Some(copy_signal) = copy_signal.as_ref() {
            self.main_queue.wait(copy_signal)?;
        }
        self.signals.set(index, signal);
        Ok(())
    }

    pub async fn wait_all_signals(&self) {
        self.signals.wait_all().await;
    }

    pub fn start_video(
        &mut self,
        path: impl AsRef<Path>,
        frame_rate: u32,
        end_frame: Option<u64>,
    ) -> anyhow::Result<()> {
        self.video.start(
            path,
            self.render_target.size(),
            frame_rate,
            1_500_000,
            end_frame,
        )
    }

    pub fn is_writing_video(&self) -> bool {
        self.video.is_writing()
    }

    pub fn stop_video(&mut self) {
        self.video.stop();
    }

    pub async fn screen_shot(&self) -> anyhow::Result<Option<image::RgbaImage>> {
        let frame = self.signals.last_frame();
        if frame.is_none() {
            return Ok(None);
        }
        let (index, frame) = frame.unwrap();
        let cmd_list = CopyCommandList::new(
            "Renderer::screen_shot",
            &self.d3d12_device,
            &self.cmd_allocators[Self::COPY_ALLOCATOR],
        )?;
        let src = self.render_target.copy_resource(index);
        let read_back_buffer = self.read_back_buffers.pop().await;
        cmd_list.record(
            &self.cmd_allocators[Self::COPY_ALLOCATOR],
            |cmd: CopyCommand<CopyResource, ReadBackBuffer>| {
                cmd.barrier([src.enter()]);
                cmd.copy(&src, &*read_back_buffer);
                cmd.barrier([src.leave()]);
            },
        )?;
        self.copy_queue.wait(&frame)?;
        self.copy_queue.execute([&cmd_list])?.wait().await?;
        let img = read_back_buffer.to_image()?;
        Ok(Some(img))
    }

    pub async fn resize(&mut self, size: wita::PhysicalSize<u32>) -> Result<(), Error> {
        self.wait_all_signals().await;
        self.swap_chain.resize(&self.d3d12_device, size)?;
        self.ui.resize(&self.d3d12_device, size).await?;
        Ok(())
    }

    pub fn change_dpi(&mut self, dpi: u32) -> Result<(), Error> {
        self.ui.change_dpi(dpi)?;
        Ok(())
    }

    pub async fn restore(&mut self, size: wita::PhysicalSize<u32>) -> Result<(), Error> {
        self.wait_all_signals().await;
        self.swap_chain.resize(&self.d3d12_device, size)?;
        self.ui.resize(&self.d3d12_device, size).await?;
        self.adjusted_plane
            .replace(
                &self.d3d12_device,
                &self.copy_queue,
                &plane::Meshes::new(1.0, 1.0),
            )
            .await?;
        Ok(())
    }

    pub async fn maximize(&mut self, size: wita::PhysicalSize<u32>) -> Result<(), Error> {
        self.wait_all_signals().await;
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
        self.adjusted_plane
            .replace(
                &self.d3d12_device,
                &self.copy_queue,
                &plane::Meshes::new(s[0], s[1]),
            )
            .await?;
        self.ui.resize(&self.d3d12_device, size).await?;
        Ok(())
    }

    pub async fn recreate(
        &mut self,
        resolution: settings::Resolution,
        compiler: &hlsl::Compiler,
        shader_model: hlsl::ShaderModel,
    ) -> Result<(), Error> {
        self.wait_all_signals().await;
        let render_target = RenderTargetBuffers::new(
            &self.d3d12_device,
            wita::PhysicalSize::new(resolution.width, resolution.height),
            Self::BUFFER_COUNT,
        )?;
        let pixel_shader = PixelShader::new(&self.d3d12_device, compiler, shader_model)?;
        let layer_shader = LayerShader::new(&self.d3d12_device, compiler, shader_model)?;
        let cmd_list = DirectCommandList::new(
            "Renderer::cmd_list",
            &self.d3d12_device,
            &self.cmd_allocators[0],
            layer_shader,
        )?;
        self.read_back_buffers = Pool::with_initializer(Self::READ_BACK_BUFFER_COUNT, || {
            ReadBackBuffer::new(&self.d3d12_device, resolution.into())
        })?;
        self.render_target = render_target;
        self.pixel_shader = pixel_shader;
        self.cmd_list = cmd_list;
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.wait_all_signals().await;
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn render_fill_test() {
        let device: ID3D12Device = unsafe {
            let mut device = None;
            D3D12CreateDevice(None, D3D_FEATURE_LEVEL_12_1, &mut device).unwrap();
            device.unwrap()
        };
        let cmd_allocator: ID3D12CommandAllocator = unsafe {
            device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                .unwrap()
        };
        let compiler = hlsl::Compiler::new().unwrap();
        let shader_model = hlsl::ShaderModel::new(&device, Option::<&String>::None).unwrap();
        let layer_shader = LayerShader::new(&device, &compiler, shader_model).unwrap();
        let cmd_list = DirectCommandList::new(
            "render_test::cmd_list",
            &device,
            &cmd_allocator,
            layer_shader,
        )
        .unwrap();
        let cmd_queue =
            CommandQueue::<DirectCommandList>::new("render_test::cmd_queue", &device).unwrap();
        let copy_queue =
            CommandQueue::<CopyCommandList>::new("render_test::copy_queue", &device).unwrap();
        let plane = plane::Buffer::new(&device, &copy_queue).await.unwrap();
        let pixel_shader = PixelShader::new(&device, &compiler, shader_model).unwrap();
        let resolution = wita::PhysicalSize::new(640, 480);
        let blob = compiler
            .compile_from_file(
                "examples/fill.hlsl",
                "main",
                hlsl::Target::PS(shader_model),
                &[],
            )
            .unwrap();
        let ps = pixel_shader
            .create_pipeline("render_test::ps", &device, &blob)
            .unwrap();
        let parameters = pixel_shader::Parameters {
            resolution: [resolution.width as f32, resolution.height as f32],
            mouse: [0.0, 0.0],
            time: 0.0,
        };
        let buffers = RenderTargetBuffers::new(&device, resolution, 1).unwrap();
        let shader = pixel_shader.apply(&ps, &parameters);
        let target = buffers.target(0);
        cmd_list
            .record(&cmd_allocator, |cmd| {
                cmd.barrier([target.enter()]);
                cmd.clear(&target, [0.0, 0.0, 0.0, 0.0]);
                cmd.draw(&shader, &target, &plane);
                cmd.barrier([target.leave()]);
            })
            .unwrap();
        cmd_queue
            .execute([&cmd_list])
            .unwrap()
            .wait()
            .await
            .unwrap();
        let copy_allocator: ID3D12CommandAllocator = unsafe {
            device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_COPY)
                .unwrap()
        };
        let copy_list =
            CopyCommandList::new("render_test::copy_list", &device, &copy_allocator).unwrap();
        let read_back_buffer = ReadBackBuffer::new(&device, resolution).unwrap();
        let src = buffers.copy_resource(0);
        copy_list
            .record(
                &copy_allocator,
                |cmd: CopyCommand<CopyResource, ReadBackBuffer>| {
                    cmd.barrier([src.enter()]);
                    cmd.copy(&src, &read_back_buffer);
                    cmd.barrier([src.leave()]);
                },
            )
            .unwrap();
        copy_queue
            .execute([&copy_list])
            .unwrap()
            .wait()
            .await
            .unwrap();
        let ret = read_back_buffer.to_image().unwrap();
        let img = image::open("test_resource/fill.png").unwrap().to_rgba8();
        assert!(ret.iter().zip(img.iter()).all(|(a, b)| {
            let a = *a as i16;
            let b = *b as i16;
            (a - b).abs() <= 1
        }));
    }
}
