/*
 * @file /parastation-core/src/gpu/gpu_commands.rs
 * @brief
 * Enumerations for the GPU commands and encoding how many parameters they take (for GP0 commands).
 *
 * The parameters of the GPU commands are not encapsulated in the enum since variable length
 * commands need the whole buffer before decoding, but instead just act as an identity for each
 * command.
 *
 * -----
 */

/// GP0 commands, identified by the high byte of the first word in the command buffer.
///
/// https://problemkaputt.de/psx-spx.htm#gpuioportsdmachannelscommandsvram
#[derive(Debug)]
pub enum Gp0Command {
    Nop,         // 0x00
    ClearCache,  // 0x01
    FillRect,    // 0x02
    Unknown0x03, // 0x03 (takes up FIFO space)
    RaiseIrq,    // 0x1F

    MonochromeTri,      // 0x20, 0x22
    MonochromeQuad,     // 0x28, 0x2A
    TexturedTri,        // 0x24, 0x25, 0x26, 0x27
    TexturedQuad,       // 0x2C, 0x2D, 0x2E, 0x2F
    ShadedTri,          // 0x30, 0x32
    ShadedQuad,         // 0x38, 0x3A
    ShadedTexturedTri,  // 0x34, 0x36
    ShadedTexturedQuad, // 0x3C, 0x3E

    MonochromeLine,     // 0x40, 0x42
    MonochromePolyline, // 0x48, 0x4A
    ShadedLine,         // 0x50, 0x52
    ShadedPolyline,     // 0x58, 0x5A

    VariableMonochromeRectangle, // 0x60, 0x62
    MonochromeRectangle,         // 0x68, 0x6A, 0x70, 0x72, 0x78, 0x7A
    VariableTexturedRectangle,   // 0x64, 0x65, 0x66, 0x67
    TexturedRectangle,           // 0x6C to 0x7F

    CopyRect,       // 0x80
    SendRectToVram, // 0xA0
    CopyRectToCpu,  // 0xC0

    SetRenderingAttribute, // 0xE1 to 0xE6

    Unknown(u8), // Unrecognised command
}

/// Decode a GP0 command from the first word of the command buffer
pub fn decode_gp0_command(word: u32) -> Gp0Command {
    // TODO please use updated psx-spx for dispatch, this is kind of bad
    let command_id = (word >> 24) as u8;
    match command_id {
        0x00 => Gp0Command::Nop,
        0x01 => Gp0Command::ClearCache,
        0x02 => Gp0Command::FillRect,
        0x03 => Gp0Command::Unknown0x03,
        0x1F => Gp0Command::RaiseIrq,

        0x20 | 0x21 | 0x22 | 0x23 => Gp0Command::MonochromeTri,
        0x28 | 0x29 | 0x2A | 0x2B => Gp0Command::MonochromeQuad,
        0x24 | 0x25 | 0x26 | 0x27 => Gp0Command::TexturedTri,
        0x2C | 0x2D | 0x2E | 0x2F => Gp0Command::TexturedQuad,
        0x30 | 0x31 | 0x32 | 0x33 => Gp0Command::ShadedTri,
        0x38 | 0x39 | 0x3A | 0x3B => Gp0Command::ShadedQuad,
        0x34 | 0x35 | 0x36 | 0x37 => Gp0Command::ShadedTexturedTri,
        0x3C | 0x3D | 0x3E | 0x3F => Gp0Command::ShadedTexturedQuad,

        0x40 | 0x42 => Gp0Command::MonochromeLine,
        0x48 | 0x4A => Gp0Command::MonochromePolyline,
        0x50 | 0x52 => Gp0Command::ShadedLine,
        0x58 | 0x5A => Gp0Command::ShadedPolyline,

        0x60 | 0x62 => Gp0Command::VariableMonochromeRectangle,
        0x68 | 0x6A | 0x70 | 0x72 | 0x78 | 0x7A => Gp0Command::MonochromeRectangle,
        0x64 | 0x65 | 0x66 | 0x67 => Gp0Command::VariableTexturedRectangle,
        0x6C..=0x6F | 0x74..=0x7F => Gp0Command::TexturedRectangle,

        0x80 => Gp0Command::CopyRect,
        0xA0 => Gp0Command::SendRectToVram,
        0xC0 => Gp0Command::CopyRectToCpu,

        0xE1..=0xE6 => Gp0Command::SetRenderingAttribute,

        _ => Gp0Command::Unknown(command_id),
    }
}

/// Get how many words following the initial command word are parameters for the given GP0 command, based on the command type and
pub fn gp0_command_parameter_count(command: &Gp0Command) -> usize {
    match command {
        Gp0Command::Nop
        | Gp0Command::ClearCache
        | Gp0Command::Unknown0x03
        | Gp0Command::RaiseIrq => 0,
        Gp0Command::FillRect => 2,

        Gp0Command::MonochromeTri => 3,
        Gp0Command::MonochromeQuad => 4,

        Gp0Command::TexturedTri => 6,
        Gp0Command::TexturedQuad => 8,

        Gp0Command::ShadedTri => 5,
        Gp0Command::ShadedQuad => 7,

        Gp0Command::ShadedTexturedTri => 8,
        Gp0Command::ShadedTexturedQuad => 11,

        Gp0Command::MonochromeLine => 2,
        Gp0Command::MonochromePolyline => usize::MAX, // Variable length, terminated by 0x55555555

        Gp0Command::ShadedLine => 3,
        Gp0Command::ShadedPolyline => usize::MAX, // Variable length, terminated by 0x55555555

        Gp0Command::VariableMonochromeRectangle => 2,
        Gp0Command::MonochromeRectangle => 1,

        Gp0Command::VariableTexturedRectangle => 3,
        Gp0Command::TexturedRectangle => 2,

        Gp0Command::CopyRect => 3,
        Gp0Command::SendRectToVram => 2,
        Gp0Command::CopyRectToCpu => 2,

        Gp0Command::SetRenderingAttribute => 0,

        Gp0Command::Unknown(_) => 0,
    }
}
