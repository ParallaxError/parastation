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

pub struct GteRegister {
    index: u8,
}

pub struct Gte {
    // Pass for now
}