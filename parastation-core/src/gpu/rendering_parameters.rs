/*
 * @file /parastation-core/src/gpu/rendering_parameters.rs
 * @brief
 * Details the extra parameters passed to drawing methods on the GPU backend, encoded in
 * GPU instructions. These include colours, texture references, geometry data, etc
 * 
 * -----
 */

// Imports
use crate::gpu::gpu_state::*;

/// 15-bit BGR555 colour as used in VRAM
#[derive(Debug, Clone, Copy)]
pub struct Colour {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Colour {
    pub fn from_word(word: u32) -> Self {
        Self {
            r: (word & 0xFF) as u8,
            g: ((word >> 8) & 0xFF) as u8,
            b: ((word >> 16) & 0xFF) as u8,
        }
    }

    /// Convert 8-bit per channel to 15-bit BGR555 for VRAM
    pub fn to_u16(self) -> u16 {
        ((self.r as u16 >> 3))
        | ((self.g as u16 >> 3) << 5)
        | ((self.b as u16 >> 3) << 10)
    }
}

/// 2D vector representing a vertex with signed 10 bit integral coordinates
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub x: i16,
    pub y: i16,
}

impl Vertex {
    pub fn from_word(word: u32) -> Self {
        Self {
            x: (word & 0x3FF) as i16,
            y: ((word >> 16) & 0x3FF) as i16,
        }
    }
}

/// Texture page attribute embedded in polygon commands
pub struct TexPageAttr {
    pub x: u8,
    pub y: bool,
    pub semi_transparency: u8,
    pub colour_depth: u8,
    pub texture_disable: bool,
}

impl TexPageAttr {
    pub fn from_word(word: u32) -> Self {
        Self {
            x: (word & 0xF) as u8,
            y: ((word >> 4) & 0x1) != 0,
            semi_transparency: ((word >> 5) & 0x3) as u8,
            colour_depth: ((word >> 7) & 0x3) as u8,
            texture_disable: ((word >> 11) & 0x1) != 0,
        }
    }
}

/// Encodes the UV coordinates of a vertex for texture mapping, with 8 bits per channel
#[derive(Debug, Clone, Copy)]
pub struct Texcoord {
    pub u: u8,
    pub v: u8,
}

impl Texcoord {
    pub fn from_word(word: u32) -> Self {
        Self {
            u: (word & 0xFF) as u8,
            v: ((word >> 8) & 0xFF) as u8,
        }
    }
}

/// The palette for 4/8 bit textures, encoded as a colour look up table
pub struct Clut {
    pub x: u8,
    pub y: u16,
}

impl Clut {
    pub fn from_word(word: u16) -> Self {
        Self {
            x: (word & 0x3F) as u8,
            y: ((word >> 6) & 0x1FF) as u16,
        }
    }
}

// Polygon drawing parameters
/// Vertex for monochrome flat-shaded polygons
pub struct FlatVertex {
    pub vertex: Vertex,
}

/// Vertex for shaded polygons
pub struct ShadedVertex {
    pub vertex: Vertex,
    pub colour: Colour,
}

/// Vertex for textured polygons
pub struct TexturedVertex {
    pub vertex: Vertex,
    pub texcoord: Texcoord,
}

/// Vertex for textured and shaded polygons
pub struct ShadedTexturedVertex {
    pub vertex: Vertex,
    pub colour: Colour,
    pub texcoord: Texcoord,
}

/// Represents the vertices of a primitive polygon to be drawn, either a tri or a quad.
/// Quads are internally broken down into two triangles by the backend on the original PS1 hardware
pub enum PolygonVertices<V> {
    Tri(V, V, V),
    Quad(V, V, V, V),
}

/// Represents the texture parameters for a textured polygon, one per polygon
pub struct TextureParams {
    pub clut: Clut,
    pub tex_page: TexPageAttr,
    pub raw_texture: bool,
}

/// A polygon that can be drawn by the GPU, with all the necessary parameters for rendering, 
/// including vertices and texture information
pub enum Polygon {
    Monochrome {
        colour: Colour,
        vertices: PolygonVertices<FlatVertex>,
        semi_transparent: bool,
    },

    Textured {
        colour: Colour,
        texture_params: TextureParams,
        vertices: PolygonVertices<TexturedVertex>,
        semi_transparent: bool,
    },

    Shaded {
        vertices: PolygonVertices<ShadedVertex>,
        semi_transparent: bool,
    },

    ShadedTextured {
        texture_params: TextureParams,
        vertices: PolygonVertices<ShadedTexturedVertex>,
        semi_transparent: bool,
    },
}

// Line drawing parameters
/// Represents a point for monochrome line drawing, with a vertex
pub struct LinePoint {
    pub vertex: Vertex,
}

/// Represents a point for coloured line drawing, with a vertex and colour
pub struct ColouredLinePoint {
    pub vertex: Vertex,
    pub colour: Colour,
}

/// A line that can be drawn by the GPU, with all the necessary parameters for rendering,
/// including vertices and colour information
pub enum Line {
    Monochrome {
        vertices: Vec<LinePoint>,
        semi_transparent: bool,
    },

    Coloured {
        vertices: Vec<ColouredLinePoint>,
        semi_transparent: bool,
    },
}

/// Represents the size of a rectangle to be drawn, either variable or fixed sizes of 1x1, 8x8, or 
/// 16x16
pub enum RectSize {
    Variable { w: u16, h: u16 },
    Fixed1x1,
    Fixed8x8,
    Fixed16x16,
}

/// A rectangle that can be drawn by the GPU, with all the necessary parameters for rendering, 
/// including position, size, colour, and texture information
pub enum Rect {
    Monochrome {
        colour:            Colour,
        pos:              Vertex,
        size:             RectSize,
        semi_transparent: bool,
    },

    Textured {
        colour: Colour,
        pos: Vertex,
        texcoord: Texcoord,
        clut: Clut,
        raw: bool,
        size: RectSize,
        semi_transparent: bool,
    },
}

/// The subset of the GPU state to be passed to the backend for draw calls.
pub struct DrawParams {
    pub drawing_area: DrawingArea,
    pub drawing_offset: DrawingOffset,
    pub mask: Mask,
    pub draw_mode: DrawMode,
    pub semi_transparency: u8,
}