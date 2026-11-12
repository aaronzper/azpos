use alloc::string::String;

#[derive(Clone)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;
    use alloc::{string::ToString, vec::Vec};

    fn path(raw: &str) -> Option<FilePath> {
        FilePath::new(raw.to_string())
    }

    ktest! {
        fn new_accepts_only_an_absolute_path() {
            check!(path("/").is_some());
            check!(path("/programs/adam.exe").is_some());
            check!(path("programs/adam.exe").is_none());
            check!(path("").is_none());
        }

        fn as_parts_gives_each_component() {
            let p = path("/programs/tools/adam.exe").or_fail("new")?;
            let parts: Vec<&str> = p.as_parts().collect();
            check_eq!(parts, ["programs", "tools", "adam.exe"]);

            let root = path("/").or_fail("new")?;
            check_eq!(root.as_parts().count(), 0);
        }

        fn filename_is_the_last_component() {
            check_eq!(path("/programs/adam.exe").or_fail("new")?.filename(), "adam.exe");
            check_eq!(path("/programs/").or_fail("new")?.filename(), "programs");
            check_eq!(path("/").or_fail("new")?.filename(), "/");
        }

        fn a_final_slash_means_directory() {
            check!(path("/programs/").or_fail("new")?.must_be_dir());
            check!(path("/").or_fail("new")?.must_be_dir());
            check!(!path("/programs").or_fail("new")?.must_be_dir());
        }

        fn path_dirs_removes_the_filename() {
            let p = path("/programs/tools/adam.exe").or_fail("new")?;
            check_eq!(p.path_dirs().as_str(), "/programs/tools");

            let dir = path("/programs/tools/").or_fail("new")?;
            check_eq!(dir.path_dirs().as_str(), "/programs/tools/");
        }

        /// The directory of a file in the root directory is `/`. Today
        /// `path_dirs` builds an empty path and panics on the `unwrap`.
        #[known_bug(unfiled, crash)]
        fn path_dirs_of_a_root_file_is_the_root() {
            let p = path("/adam.exe").or_fail("new")?;
            check_eq!(p.path_dirs().as_str(), "/");
        }
    }
}
