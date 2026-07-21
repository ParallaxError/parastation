/*
 * @file /parastation-frontend/src/gl_texture-barrier.rs
 * @brief
 * Glow does not provide a binding for glTextureBarrier, which is required for R16UI texture writes to be visible to
 * subsequent reads. This file provides a raw binding to the function, and a wrapper struct to load it from a GL
 * context.
 *
 * Again, I dont know anything about this interface so Claude told me to do this. I hope it works.
 * -----
 */

use std::ffi::c_void;

type GlTextureBarrierFn = unsafe extern "system" fn();

pub struct RawGlExt {
    texture_barrier: Option<GlTextureBarrierFn>,
}

impl RawGlExt {
    pub fn load(loader: impl Fn(&std::ffi::CStr) -> *const c_void) -> Self {
        let name = std::ffi::CString::new("glTextureBarrier").unwrap();
        let ptr = loader(&name);
        let texture_barrier = if ptr.is_null() {
            None
        } else {
            Some(unsafe { std::mem::transmute::<*const c_void, GlTextureBarrierFn>(ptr) })
        };
        Self { texture_barrier }
    }

    pub unsafe fn texture_barrier(&self) {
        if let Some(f) = self.texture_barrier {
            f();
        } else {
            debug_assert!(false, "glTextureBarrier not available");
        }
    }
}
