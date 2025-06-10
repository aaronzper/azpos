use core::i64;

pub type ResourceID = u32;

/// An abstract resource that is exposed by the kernel and used by a process.
///
/// Could be a file, a device, a socket, a pipe, or something else.
pub trait Resource {
    /// Reads up to `buffer.len()` bytes from the resource, into `buffer` and 
    /// advances the seek head by that amount. Returns the amount read.
    fn read(&mut self, buffer: &mut [u8]) -> ResourceResult;

    /// Writes the buffer into the resource, advancing the seek head to the end
    /// of the write. May not write the whole buffer. Returns the number of
    /// bytes written.
    fn write(&mut self, buffer: &[u8]) -> ResourceResult;

    /// Sets the seek head to the given offset from the beginning of the
    /// resource.
    fn seek(&mut self, offset: usize) -> ResourceResult;
}

pub type ResourceResult = Result<i64, ResourceError>;

#[derive(Debug)]
/// The errors that can be returned by resource-related syscalls.
///
/// For such calls, a return value of 0 or above indicates success, and
/// potentially something else on a per-syscall basis (e.g., how many bytes were
/// read/written). A negative return value indicates an error. The rustdocs on
/// each error type below denote which negative value encoding indicate which.
pub enum ResourceError {
    /// The resource ID was not found
    ///
    /// Encoded as a `-1`
    ResourceNotFound,

    /// The attempted operation is not supported on the given resource
    ///
    /// Encoded as a `-2`
    Unsupported,

    /// The given input data is invalid. See description for either the syscall
    /// or the resource type's implementation thereof for what may cause this.
    ///
    /// Encoded as a `-3`
    InvalidInput,

    /// A non-standard error, the meaning of which may vary on a syscall-by-syscall
    /// basis
    ///
    /// Such an error can be encoded as `-0x10000` or below. Error code `-0xFFFF`
    /// and above is reserved for standard resource error values.
    Misc(i64),
}

/// Converts a syscall return value into a `ResourceResult`.
///
/// Panics if the error code is reserved or invalid.
pub fn rax_to_result(return_val: i64) -> ResourceResult {
    match return_val {
        0..=i64::MAX => {
            Ok(return_val)
        },

        -1 => Err(ResourceError::ResourceNotFound),

        -2 => Err(ResourceError::Unsupported),

        i64::MIN..-0xFFFF => Err(ResourceError::Misc(return_val)),

        _ => panic!("Invalid resource syscall return value encoutered"),
    }
}

/// Converts a `ResourceResult<T>` into a syscall return value
///
/// `T` must be convertable into an `i64`. Panics if that value is less than 0.
pub fn result_to_rax(result: ResourceResult) -> i64 {
    match result {
        Ok(val) => {
            let val_int = val.into();
            assert!(val_int >= 0);
            val_int
        }
       
        Err(ResourceError::ResourceNotFound) => -1,

        Err(ResourceError::Unsupported) => -2,

        Err(ResourceError::InvalidInput) => -3,

        Err(ResourceError::Misc(c)) => c,
    }
}
