#[non_exhaustive]
#[derive(Clone, Debug)]
/// A PS/2 keyboard scancode
pub enum Scancode {
    Char(char),
    Tab,
    Enter,
    Escape,
    Backspace,
    LeftShift, RightShift,
    LeftAlt, RightAlt,
    LeftControl, RightControl,
    CapsLock,
}

/// Error type returned by Scancode::try_from if the given raw code is invalid
pub struct InvalidScancode;

impl TryFrom<u8> for Scancode {
    type Error = InvalidScancode;

    fn try_from(raw_code: u8) -> Result<Self, Self::Error> {
        let scancode = match raw_code {
            0x01 => Scancode::Escape,
            0x02 => Scancode::Char('1'),
            0x03 => Scancode::Char('2'),
            0x04 => Scancode::Char('3'),
            0x05 => Scancode::Char('4'),
            0x06 => Scancode::Char('5'),
            0x07 => Scancode::Char('6'),
            0x08 => Scancode::Char('7'),
            0x09 => Scancode::Char('8'),
            0x0A => Scancode::Char('9'),
            0x0B => Scancode::Char('0'),
            0x0C => Scancode::Char('-'),
            0x0D => Scancode::Char('='),
            0x0E => Scancode::Backspace,
            0x0F => Scancode::Tab,
            0x10 => Scancode::Char('q'),
            0x11 => Scancode::Char('w'),
            0x12 => Scancode::Char('e'),
            0x13 => Scancode::Char('r'),
            0x14 => Scancode::Char('t'),
            0x15 => Scancode::Char('y'),
            0x16 => Scancode::Char('u'),
            0x17 => Scancode::Char('i'),
            0x18 => Scancode::Char('o'),
            0x19 => Scancode::Char('p'),
            0x1A => Scancode::Char('['),
            0x1B => Scancode::Char(']'),
            0x1C => Scancode::Enter,
            0x1E => Scancode::Char('a'),
            0x1F => Scancode::Char('s'),
            0x20 => Scancode::Char('d'),
            0x21 => Scancode::Char('f'),
            0x22 => Scancode::Char('g'),
            0x23 => Scancode::Char('h'),
            0x24 => Scancode::Char('j'),
            0x25 => Scancode::Char('k'),
            0x26 => Scancode::Char('l'),
            0x27 => Scancode::Char(';'),
            0x28 => Scancode::Char('\''),
            0x29 => Scancode::Char('`'),
            0x2A => Scancode::LeftShift,
            0x2B => Scancode::Char('\\'),
            0x2C => Scancode::Char('z'),
            0x2D => Scancode::Char('x'),
            0x2E => Scancode::Char('c'),
            0x2F => Scancode::Char('v'),
            0x30 => Scancode::Char('b'),
            0x31 => Scancode::Char('n'),
            0x32 => Scancode::Char('m'),
            0x33 => Scancode::Char(','),
            0x34 => Scancode::Char('.'),
            0x35 => Scancode::Char('/'),
            0x36 => Scancode::RightShift,
            0x38 => Scancode::LeftAlt,
            0x39 => Scancode::Char(' '),
            0x3A => Scancode::CapsLock,

            _ => return Err(InvalidScancode),
        };

        Ok(scancode)
    }
}
