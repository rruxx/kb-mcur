// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Device-independent bitmap holding premultiplied BGRA pixels, matching what
//! `UpdateLayeredWindow` expects for per-pixel alpha (see `window::update_window`).

use anyhow::Result;
use tiny_skia::Pixmap as SkiaPixmap;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, HBITMAP, HDC, HGDIOBJ, RGBQUAD, SelectObject,
};

pub struct Dib {
    bitmap: HBITMAP,
    dc: HDC,
    bits: *mut std::ffi::c_void,
    w: i32,
    h: i32,
}

impl Dib {
    /// Create a top-down 32-bit BGRA DIB section.
    pub fn new(w: u16, h: u16) -> Result<Self> {
        let (w, h) = (i32::from(w), i32::from(h));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };
        let dc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
        if dc.is_null() {
            anyhow::bail!("CreateCompatibleDC failed");
        }
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap = unsafe {
            CreateDIBSection(
                dc,
                &raw const bmi,
                DIB_RGB_COLORS,
                &raw mut bits,
                std::ptr::null_mut(),
                0,
            )
        };
        if bitmap.is_null() {
            unsafe { DeleteDC(dc) };
            anyhow::bail!("CreateDIBSection failed");
        }
        unsafe { SelectObject(dc, bitmap as HGDIOBJ) };
        Ok(Self {
            bitmap,
            dc,
            bits,
            w,
            h,
        })
    }

    /// Copy a pixmap (premultiplied RGBA) into the DIB as BGRA.
    /// Writes through the DIB pixel pointer; interior-mutable by design.
    pub fn upload(&self, pixmap: &SkiaPixmap) -> Result<()> {
        if pixmap.width() != u32::try_from(self.w).unwrap_or(0)
            || pixmap.height() != u32::try_from(self.h).unwrap_or(0)
        {
            anyhow::bail!(
                "pixmap size {}x{} does not match DIB {}x{}",
                pixmap.width(),
                pixmap.height(),
                self.w,
                self.h
            );
        }
        let dst = self.bits.cast::<u32>();
        for (i, px) in pixmap.data().chunks_exact(4).enumerate() {
            let b = u32::from(px[0]);
            let g = u32::from(px[1]);
            let r = u32::from(px[2]);
            let a = u32::from(px[3]);
            unsafe { *dst.add(i) = b | (g << 8) | (r << 16) | (a << 24) };
        }
        Ok(())
    }

    #[must_use]
    pub fn dc(&self) -> HDC {
        self.dc
    }

    #[must_use]
    pub fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.bitmap as HGDIOBJ);
            DeleteDC(self.dc);
        }
    }
}
