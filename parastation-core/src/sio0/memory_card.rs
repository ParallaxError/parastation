/*
 * @file /parastation-core/src/sio0/memory_card.rs
 * @brief
 * Implementation for the memory card's communication sequence with the PS1.
 * Holds 128KB of data, and implements the SioDevice trait to implement Read and Write operations.
 *
 * -----
 */

// Imports
use crate::sio0::sio_device::SioDevice;

#[derive(Debug, Clone, Copy, PartialEq)]
enum MemCardCommand {
    AwaitingAddress, // Waiting for the address byte to be selected
    Idle,            // No command in progress
    Read,  // 0x52: Reads 128 bytes from the current frame address and sends them to the PS1
    Write, // 0x57: Writes 128 bytes to the current frame address from the PS1
    GetId, // 0x53: Sends the memory card ID to the PS1
}

// Individual command states

// Read
/*
Reading Data from Memory Card
Send Reply Comment
52h  FLAG  Send Read Command (ASCII "R"), Receive FLAG Byte
00h  5Ah   Receive Memory Card ID1
00h  5Dh   Receive Memory Card ID2
MSB  (00h) Send Address MSB  ;\sector number (0..3FFh)
LSB  (pre) Send Address LSB  ;/
00h  5Ch   Receive Command Acknowledge 1  ;<-- late /ACK after this byte-pair
00h  5Dh   Receive Command Acknowledge 2
00h  MSB   Receive Confirmed Address MSB
00h  LSB   Receive Confirmed Address LSB
00h  ...   Receive Data Sector (128 bytes)
00h  CHK   Receive Checksum (MSB xor LSB xor Data bytes)
00h  47h   Receive Memory End Byte (should be always 47h="G"=Good for Read)
*/
#[derive(Debug, Clone, Copy, PartialEq)]
enum MemCardReadState {
    SendReadCommand,
    ReceiveId1,
    ReceiveId2,
    SendAddressMSB,
    SendAddressLSB,
    ReceiveCommandAck1,
    ReceiveCommandAck2,
    ReceiveConfirmedAddressMSB,
    ReceiveConfirmedAddressLSB,
    ReceiveDataSector,
    ReceiveMemoryEndByte,
}

// Write
/*
Send Reply Comment
57h  FLAG  Send Write Command (ASCII "W"), Receive FLAG Byte
00h  5Ah   Receive Memory Card ID1
00h  5Dh   Receive Memory Card ID2
MSB  (00h) Send Address MSB  ;\sector number (0..3FFh)
LSB  (pre) Send Address LSB  ;/
...  (pre) Send Data Sector (128 bytes)
CHK  (pre) Send Checksum (MSB xor LSB xor Data bytes)
00h  5Ch   Receive Command Acknowledge 1
00h  5Dh   Receive Command Acknowledge 2
00h  4xh   Receive Memory End Byte (47h=Good, 4Eh=BadChecksum, FFh=BadSector)
*/
#[derive(Debug, Clone, Copy, PartialEq)]
enum MemCardWriteState {
    SendWriteCommand,
    ReceiveId1,
    ReceiveId2,
    SendAddressMSB,
    SendAddressLSB,
    SendDataSector,
    ReceiveCommandAck1,
    ReceiveCommandAck2,
    ReceiveMemoryEndByte,
}

// GetID
/*
Send Reply Comment
53h  FLAG  Send Get ID Command (ASCII "S"), Receive FLAG Byte
00h  5Ah   Receive Memory Card ID1
00h  5Dh   Receive Memory Card ID2
00h  5Ch   Receive Command Acknowledge 1
00h  5Dh   Receive Command Acknowledge 2
00h  04h   Receive 04h
00h  00h   Receive 00h
00h  00h   Receive 00h
00h  80h   Receive 80h
*/
#[derive(Debug, Clone, Copy, PartialEq)]
enum MemCardGetIdState {
    SendGetIdCommand,
    ReceiveId1,
    ReceiveId2,
    ReceiveCommandAck1,
    ReceiveCommandAck2,
    Receive04h,
    Receive00h1,
    Receive00h2,
    Receive80h,
}

enum MemCardCommandState {
    Read(MemCardReadState),
    Write(MemCardWriteState),
    GetId(MemCardGetIdState),
    Idle,
}

pub struct MemoryCard {
    data: Box<[u8; 131072]>, // 128KB allocated on the heap
    command: MemCardCommand,
    command_state: MemCardCommandState,
    selected: bool,

    checksum: u8,                // XOR checksum accumulated during transfer
    sector_number: u16,          // Current sector being read/written
    sector_buffer: [u8; 128],    // Buffer for the current sector being read/written
    sector_buffer_cursor: usize, // Cursor for the current position in the sector buffer
}

impl MemoryCard {
    pub fn new() -> Self {
        Self {
            data: Box::new([0; 131072]),
            command: MemCardCommand::AwaitingAddress,
            command_state: MemCardCommandState::Idle,
            selected: false,
            checksum: 0,
            sector_number: 0,
            sector_buffer: [0; 128],
            sector_buffer_cursor: 0,
        }
    }
}

impl MemoryCard {
    // Command exchanges
    fn exchange_read(&mut self, byte: u8) -> (u8, bool) {
        match self.command_state {
            MemCardCommandState::Read(state) => {
                match state {
                    MemCardReadState::SendReadCommand => {
                        if byte == 0x52 {
                            self.command_state =
                                MemCardCommandState::Read(MemCardReadState::ReceiveId1);
                            // TODO flag byte
                            (0x5A, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardReadState::ReceiveId1 => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::Read(MemCardReadState::ReceiveId2);
                            (0x5A, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardReadState::ReceiveId2 => {
                        if byte == 0x00 {
                            // Update checksum and prepare to receive the sector address
                            self.checksum = 0;
                            self.command_state =
                                MemCardCommandState::Read(MemCardReadState::SendAddressMSB);
                            (0x5D, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardReadState::SendAddressMSB => {
                        self.command_state =
                            MemCardCommandState::Read(MemCardReadState::SendAddressLSB);
                        self.checksum ^= byte; // Update checksum
                        self.sector_number = (byte as u16) << 8; // Store MSB of sector number
                        (0x00, true)
                    }
                    MemCardReadState::SendAddressLSB => {
                        self.command_state =
                            MemCardCommandState::Read(MemCardReadState::ReceiveCommandAck1);
                        self.checksum ^= byte; // Update checksum
                        self.sector_number |= byte as u16; // Store LSB of sector number
                        self.sector_number &= 0x03FF; // Mask to valid sector range (0-0x3FF)
                        ((self.sector_number >> 8) as u8, true) // Echo back MSB of sector number
                    }
                    MemCardReadState::ReceiveCommandAck1 => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::Read(MemCardReadState::ReceiveCommandAck2);
                            (0x5C, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardReadState::ReceiveCommandAck2 => {
                        if byte == 0x00 {
                            self.command_state = MemCardCommandState::Read(
                                MemCardReadState::ReceiveConfirmedAddressMSB,
                            );
                            (0x5D, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardReadState::ReceiveConfirmedAddressMSB => {
                        if byte == 0x00 {
                            self.command_state = MemCardCommandState::Read(
                                MemCardReadState::ReceiveConfirmedAddressLSB,
                            );
                            ((self.sector_number >> 8) as u8, true) // Echo back MSB of sector number
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardReadState::ReceiveConfirmedAddressLSB => {
                        if byte == 0x00 {
                            // Reset cursor in preparation for reading the sector data
                            self.sector_buffer_cursor = 0;
                            self.command_state =
                                MemCardCommandState::Read(MemCardReadState::ReceiveDataSector);
                            ((self.sector_number & 0xFF) as u8, true) // Echo back LSB of sector number
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardReadState::ReceiveDataSector => {
                        if self.sector_buffer_cursor < 128 {
                            let data_byte = self.data
                                [(self.sector_number as usize) * 128 + self.sector_buffer_cursor];
                            self.checksum ^= data_byte; // Update checksum
                            self.sector_buffer_cursor += 1;
                            (data_byte, true)
                        } else {
                            self.command_state =
                                MemCardCommandState::Read(MemCardReadState::ReceiveMemoryEndByte);
                            (self.checksum, true) // Send checksum after all data bytes
                        }
                    }
                    MemCardReadState::ReceiveMemoryEndByte => {
                        self.command_state = MemCardCommandState::Idle;
                        (0x47, false) // 'G" for Good'
                    }
                }
            }
            _ => unreachable!(), // TODO run shell and ull see that it hits this for some reason
        }
    }

    fn exchange_write(&mut self, byte: u8) -> (u8, bool) {
        match self.command_state {
            MemCardCommandState::Write(state) => {
                match state {
                    MemCardWriteState::SendWriteCommand => {
                        if byte == 0x57 {
                            self.command_state =
                                MemCardCommandState::Write(MemCardWriteState::ReceiveId1);
                            (0x5A, true) // TODO flag
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardWriteState::ReceiveId1 => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::Write(MemCardWriteState::ReceiveId2);
                            (0x5A, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardWriteState::ReceiveId2 => {
                        if byte == 0x00 {
                            self.checksum = 0; // Reset checksum for writing data
                            self.command_state =
                                MemCardCommandState::Write(MemCardWriteState::SendAddressMSB);
                            (0x5D, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardWriteState::SendAddressMSB => {
                        self.sector_number = (byte as u16) << 8; // Store MSB of sector number
                        self.checksum ^= byte; // Update checksum
                        self.command_state =
                            MemCardCommandState::Write(MemCardWriteState::SendAddressLSB);
                        (0x00, true)
                    }
                    MemCardWriteState::SendAddressLSB => {
                        self.sector_number |= byte as u16; // Store LSB of sector number
                        self.sector_number &= 0x03FF; // Mask to valid sector range (0-0x3FF)
                        self.checksum ^= byte; // Update checksum
                        self.sector_buffer_cursor = 0; // Reset cursor for writing data
                        self.command_state =
                            MemCardCommandState::Write(MemCardWriteState::SendDataSector);

                        ((self.sector_number >> 8) as u8, true) // Echo back MSB of sector number
                    }
                    MemCardWriteState::SendDataSector => {
                        if self.sector_buffer_cursor < 128 {
                            self.sector_buffer[self.sector_buffer_cursor] = byte;
                            self.checksum ^= byte; // Update checksum
                            self.sector_buffer_cursor += 1;
                            (0x00, true) // Acknowledge receipt of data byte
                        } else {
                            self.command_state =
                                MemCardCommandState::Write(MemCardWriteState::ReceiveCommandAck1);
                            (self.checksum, true) // Send checksum after all data bytes
                        }
                    }
                    MemCardWriteState::ReceiveCommandAck1 => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::Write(MemCardWriteState::ReceiveCommandAck2);
                            (0x5C, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardWriteState::ReceiveCommandAck2 => {
                        if byte == 0x00 {
                            // Write the buffered data to the memory card
                            let start_index = (self.sector_number as usize) * 128;
                            self.data[start_index..start_index + 128]
                                .copy_from_slice(&self.sector_buffer);
                            self.command_state =
                                MemCardCommandState::Write(MemCardWriteState::ReceiveMemoryEndByte);
                            (0x5D, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardWriteState::ReceiveMemoryEndByte => {
                        self.command_state = MemCardCommandState::Idle;
                        (0x47, false) // 'G" for Good'
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    fn exchange_get_id(&mut self, byte: u8) -> (u8, bool) {
        match self.command_state {
            MemCardCommandState::GetId(state) => {
                match state {
                    MemCardGetIdState::SendGetIdCommand => {
                        if byte == 0x53 {
                            self.command_state =
                                MemCardCommandState::GetId(MemCardGetIdState::ReceiveId1);
                            (0x5A, true) // TODO flag
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardGetIdState::ReceiveId1 => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::GetId(MemCardGetIdState::ReceiveId2);
                            (0x5A, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardGetIdState::ReceiveId2 => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::GetId(MemCardGetIdState::ReceiveCommandAck1);
                            (0x5D, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardGetIdState::ReceiveCommandAck1 => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::GetId(MemCardGetIdState::ReceiveCommandAck2);
                            (0x5C, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardGetIdState::ReceiveCommandAck2 => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::GetId(MemCardGetIdState::Receive04h);
                            (0x5D, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardGetIdState::Receive04h => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::GetId(MemCardGetIdState::Receive00h1);
                            (0x04, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardGetIdState::Receive00h1 => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::GetId(MemCardGetIdState::Receive00h2);
                            (0x00, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardGetIdState::Receive00h2 => {
                        if byte == 0x00 {
                            self.command_state =
                                MemCardCommandState::GetId(MemCardGetIdState::Receive80h);
                            (0x00, true)
                        } else {
                            (0xFF, false)
                        }
                    }
                    MemCardGetIdState::Receive80h => {
                        if byte == 0x00 {
                            self.command_state = MemCardCommandState::Idle;
                            (0x80, false)
                        } else {
                            (0xFF, false)
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl SioDevice for MemoryCard {
    fn exchange(&mut self, byte: u8) -> (u8, bool) {
        match self.command {
            MemCardCommand::AwaitingAddress => {
                if byte == 0x81 {
                    self.command = MemCardCommand::Idle;
                    (0xFF, true)
                } else {
                    (0xFF, false)
                }
            }
            MemCardCommand::Idle => match byte {
                0x52 => {
                    self.command = MemCardCommand::Read;
                    self.command_state =
                        MemCardCommandState::Read(MemCardReadState::SendReadCommand);
                    self.exchange_read(byte)
                }
                0x57 => {
                    self.command = MemCardCommand::Write;
                    self.command_state =
                        MemCardCommandState::Write(MemCardWriteState::SendWriteCommand);
                    self.exchange_write(byte)
                }
                0x53 => {
                    self.command = MemCardCommand::GetId;
                    self.command_state =
                        MemCardCommandState::GetId(MemCardGetIdState::SendGetIdCommand);
                    self.exchange_get_id(byte)
                }
                _ => (0xFF, false), // Invalid command, ignore
            },
            MemCardCommand::Read => self.exchange_read(byte),
            MemCardCommand::Write => self.exchange_write(byte),
            MemCardCommand::GetId => self.exchange_get_id(byte),
        }
    }

    fn reset(&mut self) {
        self.command = MemCardCommand::AwaitingAddress;
        self.command_state = MemCardCommandState::Idle;
        self.checksum = 0;
        self.sector_number = 0;
    }

    fn is_selected(&self) -> bool {
        self.selected
    }

    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}
