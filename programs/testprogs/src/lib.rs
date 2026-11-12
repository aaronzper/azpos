//! Shared code for the fixture programs that do not use `libsystem`.
//!
//! Such a program has no allocator, so it loads fast.

#![no_std]

/// Defines `_start` with the given body and a panic handler.
///
/// `_start` spins after the body. The panic handler also spins.
#[macro_export]
macro_rules! bare_program {
    ($($body:tt)*) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn _start() -> ! {
            $($body)*
            #[allow(unreachable_code)]
            loop {}
        }

        #[panic_handler]
        fn panic(_info: &core::panic::PanicInfo) -> ! {
            loop {}
        }
    };
}
