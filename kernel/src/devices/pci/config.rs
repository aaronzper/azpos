use crate::devices::{read_port, write_port};

const CONFIG_ADDRESS_PORT: u16 = 0xCF8; // 32 bit register
const CONFIG_DATA_PORT: u16 = 0xCFC;    // 32 bit register

fn set_config_addr(bus: u8, device: u8, func: u8, word: u8) {
    if device > 0b11111 { // Device must be 5 bits
        panic!("Device number {:#X} too large, must be five bits", device);
    }

    if func > 0b111 { // Device must be 3 bits
        panic!("Function number {:#X} too large, must be three bits", func);
    }

    if word >= 64 {
        panic!("Only 64 words can be addressed, tried to read word {}", word);
    }

    let address = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((func as u32) << 8)
        | ((word << 2) as u32);

    write_port(CONFIG_ADDRESS_PORT, address);
}

pub fn read_config(bus: u8, device: u8, func: u8, word: u8) -> u32 {
    set_config_addr(bus, device, func, word);
    read_port(CONFIG_DATA_PORT)
}
