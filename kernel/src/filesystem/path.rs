use alloc::string::String;

#[derive(Clone)]
/// A path to a file
pub struct FilePath {
    raw_path: String,
}

impl FilePath {
    /// Constructs a `FilePath` from an owned string containing the raw
    /// path. Directories are delimited by `/`. Should start with a `/` to
    /// denote the root directory.
    ///
    /// Returns `None` if the path is invalid.  Invalid paths include those
    /// that don't start with `/`, contain empty components (`//`), or contain
    /// `.` or `..` components.
    pub fn new(path: String) -> Option<Self> {
        if !path.starts_with("/") {
            return None;
        }

        // Validate components (split_terminator ignores a trailing /)
        for part in path[1..].split_terminator('/') {
            if part.is_empty() || part == "." || part == ".." {
                return None;
            }
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

    /// Returns a `FilePath` containing just the directories of the path; that 
    /// is, everything except the final filename. If `self.must_be_dir` is
    /// `true`, then returns a copy of `self`.
    pub fn path_dirs(&self) -> Self {
        if self.must_be_dir() {
            return self.clone();
        }

        let mut new_path = String::default();
        let mut parts = self.as_parts().peekable();
        loop {
            // If we're just `/`, we'll catch it at the top of the function, so
            // this will always be `Some`
            let part = parts.next().unwrap();

            if parts.peek().is_none() {
                break Self::new(new_path).unwrap();
            }

            // Only allocate one time, instead of once for `/` and once for `part`
            new_path.reserve(part.len() + 1);

            new_path.push('/');
            new_path.push_str(part);
        }
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
