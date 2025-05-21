use pic8259::ChainedPics;

/// Which interrupt vector to start at for PIC usage (0-31 are used by the CPU)
const PIC_1_OFFSET: u8 = 32;
/// Ditto
const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub struct PIC {
    inner: ChainedPics
}

impl PIC {
    /// Create the PIC
    pub const fn new() -> PIC {
        PIC {
            inner: unsafe {
                ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET)
            },
        }
    }

    /// Initialize the PIC!
    pub fn initialize(&mut self) {
        unsafe {
            self.inner.initialize();
        }
    }

    /// Signal to the PIC that we're done handling the given interrupt and are
    /// ready for another
    pub fn end_interrupt(&mut self, int: PICInterrupt) {
        unsafe {
            self.inner.notify_end_of_interrupt(int as u8);
        }
    }
}

#[non_exhaustive]
#[repr(u8)]
/// Each interrupt that the PIC might spit out
pub enum PICInterrupt {
    Timer = PIC_1_OFFSET,
}
