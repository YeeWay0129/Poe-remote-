use crate::{CaptureError, CapturedFrame, FrameSource, PixelFormat};
use host_core::stream::StreamConfig;
use std::mem::size_of;
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
};

#[derive(Debug, Default)]
pub struct WindowsGdiFrameSource {
    next_frame_index: u64,
}

impl FrameSource for WindowsGdiFrameSource {
    fn capture_next(&mut self, config: &StreamConfig) -> Result<CapturedFrame, CaptureError> {
        if !config.is_supported_v1() {
            return Err(CaptureError::UnsupportedConfig);
        }

        let width = config.width as i32;
        let height = config.height as i32;
        let screen_dc = unsafe { GetDC(null_mut()) };
        if screen_dc.is_null() {
            return Err(CaptureError::BackendUnavailable);
        }

        let result = capture_screen_bgra(screen_dc, width, height, self.next_frame_index);
        unsafe {
            ReleaseDC(null_mut(), screen_dc);
        }

        if result.is_ok() {
            self.next_frame_index += 1;
        }

        result
    }
}

fn capture_screen_bgra(
    screen_dc: windows_sys::Win32::Graphics::Gdi::HDC,
    width: i32,
    height: i32,
    frame_index: u64,
) -> Result<CapturedFrame, CaptureError> {
    let memory_dc = unsafe { CreateCompatibleDC(screen_dc) };
    if memory_dc.is_null() {
        return Err(CaptureError::BackendUnavailable);
    }

    let bitmap = unsafe { CreateCompatibleBitmap(screen_dc, width, height) };
    if bitmap.is_null() {
        unsafe {
            DeleteDC(memory_dc);
        }
        return Err(CaptureError::BackendUnavailable);
    }

    let previous_object = unsafe { SelectObject(memory_dc, bitmap) };
    let bitblt_ok =
        unsafe { BitBlt(memory_dc, 0, 0, width, height, screen_dc, 0, 0, SRCCOPY) } != 0;
    let mut data = vec![0u8; width as usize * height as usize * 4];
    let mut bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: data.len() as u32,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [Default::default(); 1],
    };

    let copied_lines = if bitblt_ok {
        unsafe {
            GetDIBits(
                memory_dc,
                bitmap,
                0,
                height as u32,
                data.as_mut_ptr().cast(),
                &mut bitmap_info,
                DIB_RGB_COLORS,
            )
        }
    } else {
        0
    };

    unsafe {
        if !previous_object.is_null() {
            SelectObject(memory_dc, previous_object);
        }
        DeleteObject(bitmap);
        DeleteDC(memory_dc);
    }

    if copied_lines != height {
        return Err(CaptureError::BackendUnavailable);
    }

    Ok(CapturedFrame {
        width: width as u32,
        height: height as u32,
        pixel_format: PixelFormat::Bgra8,
        timestamp_nanos: frame_index.saturating_mul(16_666_667),
        data,
    })
}
