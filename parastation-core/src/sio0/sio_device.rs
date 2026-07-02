/*
 * @file /parastation-core/src/sio0/sio_device.rs
 * @brief
 * Trait and shared state for SIO0 (Serial I/O) devices, which are the joypad and memory card ports on the PS1.
 * The trait describes the shared serial communication behaviour of the PS1 and the individual devices implement their
 * specific behaviour.
 *
 * -----
 */

/// Trait implemented by a SIO0 device (joypad or memory card).
/// The PS1 SIO0 controller calls these methods to interact with the device.
pub trait SioDevice {
    /// Exchange one byte with the PS1. Called once per serial clock cycle during a transfer.
    /// Returns (response_byte, dsr) where dsr being true indicates the device will continue the transaction with
    /// another ACK pulse and IRQ after this byte
    fn exchange(&mut self, byte: u8) -> (u8, bool);

    /// Reset the device, called by setting SIO_CTRL bit1
    fn reset(&mut self);

    /// Returns whether the device is currently selected (chip select active)
    fn is_selected(&self) -> bool;

    /// Select or deselect the given device
    fn set_selected(&mut self, selected: bool);
}
