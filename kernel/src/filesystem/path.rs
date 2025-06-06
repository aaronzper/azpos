use alloc::string::{String, ToString};

/// A path to a file
pub struct FilePath {
    raw_path: String,
}

impl FilePath {
    /// Constructs a `FilePath` from an owned string containining the raw
    /// path. Directories are delimited by `/`. Should start with a `/` to
    /// denote the root directory.
    ///
    /// Returns `None` if the path is invalid.
    pub fn new(path: String) -> Option<Self> {
        if !path.starts_with("/") {
            return None;
        }

        Some(Self {
            raw_path: path
        })
    }

    /// Splits the path into its constituent parts, returning a iter of `str`s
    /// corresponding to each directory in the path, ending with the filename
    /// itself
    pub fn as_parts(&self) -> impl Iterator<Item = &str> {
        // Skip the / at the start
        self.raw_path[1..].split_terminator("/")
    }

    /// Returns the filename at the end of the path.
    ///
    /// Drops a `/` at the end, if there is one.
    ///
    /// If the path is just to the root directory, returns `/`
    pub fn filename(&self) -> &str {
        match self.as_parts().last() {
            Some(name) => name,
            None => "/",
        }
    }

    pub fn as_str(&self) -> &str {
        &self.raw_path
    }

    /// If the path ends with a /, it must be a directory, and matching the path
    /// should fail on a normal file with the same name. If it doesn't, it may
    /// be either a dir or a regular file.
    pub fn must_be_dir(&self) -> bool {
        self.raw_path.ends_with("/")
    }
}
