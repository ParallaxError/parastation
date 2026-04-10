/*
 * @file /parastation-core/src/gpu/backend.rs
 * @brief
 * Backend trait that defines the implementations for GPU commands.
 * The trait isn't implemented by the core as it's frontend dependent, but the appropriate
 * frontend must implement the GPU commands defined by
 * https://problemkaputt.de/psx-spx.htm#gpuioportsdmachannelscommandsvram
 * 
 * -----
 */

pub trait GpuBackend {
    
}