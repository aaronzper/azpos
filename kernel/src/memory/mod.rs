use bootloader_api::info::{MemoryRegionKind, MemoryRegions};

pub const KERNEL_START_ADDR: u64 = 0xFFFF_8000_0000_0000;

static mut PHYS_MAP_ADDR: u64 = 0;
static mut PHYS_SIZE: u64 = 0;

pub fn get_phys_size() -> u64 {
    unsafe {
        PHYS_SIZE
    }
}

fn resolve_phys_addr(paddr: u64) -> Option<u64> {
    if paddr >= get_phys_size() {
        None
    } else {
        unsafe {
            Some(PHYS_MAP_ADDR + paddr)
        }
    }
}

pub fn init_memory(p_map_addr: u64, regions: &MemoryRegions) {
    unsafe {
        PHYS_MAP_ADDR = p_map_addr;
        PHYS_SIZE = regions.last().unwrap().end;
    }

    println!("Total: {} bytes", get_phys_size());
    let mut usable = 0;
    for region in regions.iter() {
        if region.kind == MemoryRegionKind::Usable {
            let va_start = resolve_phys_addr(region.start).unwrap();
            let va_end = resolve_phys_addr(region.end - 1).unwrap();
            for va in va_start..=va_end {
                let p = va as *mut u8;
                unsafe {
                    *p = 0x69;
                }
            }
            usable += va_end - va_start;
            print!("\rUsable: {} bytes", usable);
        }
    }
    let pcnt = ((usable as f32) / (get_phys_size() as f32)) * 100.0;

    println!("\n{}% is usable!", pcnt);
}
