use super::*;
use std::path::Path;
use windows::Win32::Media::MediaFoundation::*;

const MF_VERSION: u32 = (MF_SDK_VERSION << 16) | 0x0070;

pub struct Context;

impl Context {
    pub fn new() -> anyhow::Result<Self> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
            Ok(Self)
        }
    }

    pub fn create_writer(
        &self,
        path: impl AsRef<Path>,
        resolution: wita::PhysicalSize<u32>,
        fps: u32,
        bit_rate: u32,
    ) -> anyhow::Result<Writer<'_>> {
        unsafe {
            let handle = MFCreateSinkWriterFromURL(path.as_ref().to_str().unwrap(), None, None)?;
            Writer::new(handle, resolution, fps, bit_rate)
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

pub struct Writer<'a> {
    handle: IMFSinkWriter,
    resolution: wita::PhysicalSize<u32>,
    fps: u32,
    stream_index: u32,
    _lifetime: std::marker::PhantomData<&'a ()>,
}

impl<'a> Writer<'a> {
    fn new(handle: IMFSinkWriter, resolution: wita::PhysicalSize<u32>, fps: u32, bit_rate: u32) -> anyhow::Result<Self> {
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
                handle: handle.clone(),
                resolution,
                fps,
                stream_index,
                _lifetime: std::marker::PhantomData,
            })
        }
    }

    pub fn write(&self, frame: &image::RgbaImage, time: u64) -> anyhow::Result<()> {
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
                    frame.as_ptr(),
                    stride as _,
                    stride,
                    self.resolution.height,
                )?;
                buffer.Unlock()?;
            }
            buffer.SetCurrentLength(buffer_size)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(buffer)?;
            sample.SetSampleTime(time as _)?;
            sample.SetSampleDuration(10 * 1000 * 1000 / self.fps as i64)?;
            self.handle.WriteSample(self.stream_index, &sample)?;
            Ok(())
        }
    }
}

impl<'a> Drop for Writer<'a> {
    fn drop(&mut self) {
        unsafe {
            if let Err(e) = self.handle.Finalize() {
                error!("Finalize: {}", e);
            }
        }
    }
}
