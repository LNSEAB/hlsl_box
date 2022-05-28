use super::*;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use windows::Win32::Media::MediaFoundation::*;

const MF_VERSION: u32 = (MF_SDK_VERSION << 16) | 0x0070;

struct Context;

impl Context {
    fn new() -> anyhow::Result<Self> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
            Ok(Self)
        }
    }

    fn create_writer(
        &self,
        path: impl AsRef<Path>,
        resolution: wita::PhysicalSize<u32>,
        fps: u32,
        bit_rate: u32,
    ) -> anyhow::Result<Writer> {
        unsafe {
            let handle = MFCreateSinkWriterFromURL(path.as_ref().to_str().unwrap(), None, None)?;
            Writer::new(path.as_ref(), handle, resolution, fps, bit_rate)
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            if let Err(e) = MFShutdown() {
                error!("MFShutdown: {}", e);
            }
        }
    }
}

struct Writer {
    path: PathBuf,
    handle: IMFSinkWriter,
    resolution: wita::PhysicalSize<u32>,
    fps: u32,
    stream_index: u32,
}

impl Writer {
    fn new(
        path: &Path,
        handle: IMFSinkWriter,
        resolution: wita::PhysicalSize<u32>,
        fps: u32,
        bit_rate: u32,
    ) -> anyhow::Result<Self> {
        unsafe {
            let out_type = MFCreateMediaType()?;
            out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            out_type.SetUINT32(&MF_MT_AVG_BITRATE, bit_rate)?;
            out_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as _)?;
            out_type.SetUINT64(
                &MF_MT_FRAME_SIZE,
                ((resolution.width as u64) << 32) | resolution.height as u64,
            )?;
            out_type.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1)?;
            out_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1 << 32) | 1)?;
            let stream_index = handle.AddStream(&out_type)?;
            let in_type = MFCreateMediaType()?;
            in_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            in_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            in_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as _)?;
            in_type.SetUINT64(
                &MF_MT_FRAME_SIZE,
                ((resolution.width as u64) << 32) | resolution.height as u64,
            )?;
            in_type.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1)?;
            in_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1 << 32) | 1)?;
            handle.SetInputMediaType(stream_index, &in_type, None)?;
            handle.BeginWriting()?;
            Ok(Self {
                path: path.to_path_buf(),
                handle,
                resolution,
                fps,
                stream_index,
            })
        }
    }

    fn write(&self, img: &image::RgbaImage, frame: u64) -> anyhow::Result<()> {
        unsafe {
            let stride = 4 * self.resolution.width;
            let buffer_size = stride * self.resolution.height;
            let buffer = MFCreateMemoryBuffer(buffer_size)?;
            {
                let mut p = std::ptr::null_mut();
                buffer.Lock(&mut p, std::ptr::null_mut(), std::ptr::null_mut())?;
                MFCopyImage(
                    p,
                    stride as _,
                    img.as_ptr(),
                    stride as _,
                    stride,
                    self.resolution.height,
                )?;
                buffer.Unlock()?;
            }
            buffer.SetCurrentLength(buffer_size)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(buffer)?;
            sample.SetSampleTime(frame as i64 * 10_000_000 / self.fps as i64)?;
            sample.SetSampleDuration(10_000_000 / self.fps as i64)?;
            self.handle.WriteSample(self.stream_index, &sample)?;
            Ok(())
        }
    }

    fn finalize(&self) -> Result<(), Error> {
        unsafe {
            if let Err(e) = self.handle.Finalize() {
                std::fs::remove_file(&self.path)
                    .map_err(|_| Error::RemoveFile(self.path.to_path_buf()))?;
                return Err(e.into());
            }
            Ok(())
        }
    }
}

unsafe impl Send for Writer {}

struct Worker {
    tx: mpsc::UnboundedSender<(PoolElement<ReadBackBuffer>, Signal)>,
}

impl Worker {
    fn new(writer: Writer, end_frame: Option<u64>) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<(PoolElement<ReadBackBuffer>, Signal)>();
        tokio::task::spawn(async move {
            let mut frame = 0;
            loop {
                let (buffer, signal) = match rx.recv().await {
                    Some(v) => v,
                    None => {
                        match writer.finalize() {
                            Ok(_) => info!("Video::worker: finalized"),
                            Err(e) => error!("Video::worker: {}", e),
                        }
                        break;
                    }
                };
                let img = {
                    if let Err(e) = signal.wait().await {
                        error!("Video::worker: {}", e);
                        break;
                    }
                    let img = match buffer.to_image() {
                        Ok(img) => img,
                        Err(e) => {
                            error!("Video::worker: {}", e);
                            break;
                        }
                    };
                    img
                };
                if let Err(e) = writer.write(&img, frame) {
                    error!("Video::worker: {}", e);
                    break;
                }
                frame += 1;
                if end_frame.map_or(false, |ef| frame >= ef) {
                    match writer.finalize() {
                        Ok(_) => info!("Video::worker: finalized"),
                        Err(e) => error!("Video::worker: {}", e),
                    }
                    break;
                }
            }
        });
        Self { tx }
    }
}

struct Timer {
    t: RefCell<std::time::Instant>,
    per_frame: std::time::Duration,
}

impl Timer {
    fn new(fps: u32) -> Self {
        Self {
            t: RefCell::new(std::time::Instant::now()),
            per_frame: std::time::Duration::from_micros(1_000_000 / fps as u64),
        }
    }

    fn signal(&self) -> bool {
        let now = std::time::Instant::now();
        if now - *self.t.borrow() >= self.per_frame {
            *self.t.borrow_mut() = now;
            true
        } else {
            false
        }
    }
}

pub struct Video {
    context: Context,
    worker: Option<Worker>,
    timer: Option<Timer>,
}

impl Video {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            context: Context::new()?,
            worker: None,
            timer: None,
        })
    }

    pub fn is_writing(&self) -> bool {
        self.worker
            .as_ref()
            .map_or(false, |worker| !worker.tx.is_closed())
    }

    pub fn start(
        &mut self,
        path: impl AsRef<Path>,
        resolution: wita::PhysicalSize<u32>,
        fps: u32,
        bit_rate: u32,
        end_frame: Option<u64>,
    ) -> anyhow::Result<()> {
        self.worker = Some(Worker::new(
            self.context
                .create_writer(path, resolution, fps, bit_rate)?,
            end_frame,
        ));
        self.timer = Some(Timer::new(fps));
        Ok(())
    }

    pub fn signal(&self) -> bool {
        self.is_writing() && self.timer.as_ref().map_or(false, |timer| timer.signal())
    }

    pub fn write(&self, buffer: PoolElement<ReadBackBuffer>, signal: Signal) -> anyhow::Result<()> {
        if self.is_writing() {
            if let Some(worker) = self.worker.as_ref() {
                worker.tx.send((buffer, signal)).unwrap_or(());
            }
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        self.worker = None;
        self.timer = None;
    }
}
