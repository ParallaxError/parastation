use parastation_core::sio0::InputProvider;

pub struct DummyInputProvider;
impl InputProvider for DummyInputProvider {
    fn get_joypad_state(&self) -> u16 {
        0xFFFF // All buttons released
    }
}
