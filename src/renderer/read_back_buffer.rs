use super::*;

pub struct ReadBackBuffer {
    buffer: Buffer,
    size: wita::PhysicalSize<u32>,
}

impl ReadBackBuffer {
    pub fn new(device: &ID3D12Device, size: wita::PhysicalSize<u32>) -> Result<Self, Error> {
        let s = (size.width * size.height * 4) as u64;
        let buffer = Buffer::new(
            "ReadBackBuffer",
            device,
            HeapProperties::new(D3D12_HEAP_TYPE_READBACK),
            s + (16 - s % 16) % 16,
            D3D12_RESOURCE_STATE_COPY_DEST,
            None,
        )?;
        Ok(Self { buffer, size })
    }

    pub fn resource(&self) -> &ID3D12Resource {
        self.buffer.handle()
    }

    pub fn to_image(&self) -> Result<image::RgbaImage, Error> {
        let data = self.buffer.map::<u8>()?;
        let mut img = image::RgbaImage::new(self.size.width, self.size.height);
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ref(), img.as_mut_ptr(), img.len());
        }
        Ok(img)
    }
}
