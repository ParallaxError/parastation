/*
 * @file /parastation-core/src/cpu/gte.rs
 * @brief
 * Coprocessor 2 of the PS1, the Geometry Transformation Engine. Handles 3D graphics, and some
 * colour effects.
 * 
 * https://problemkaputt.de/psx-spx.htm#geometrytransformationenginegte for reference
 * 
 * -----
 */

#[derive(Debug)]
pub struct GteRegister(pub u8);

pub struct Gte {
    // Pass for now
}

impl Gte {
    pub fn new() -> Self {
        Self {
            // Nothing to initialize for now
        }
    }
}