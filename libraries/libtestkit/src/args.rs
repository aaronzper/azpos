/// The arguments of one guest test run.
///
/// The text form is a list of `key=value` tokens with spaces between them.
/// Unknown keys are ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args<'a> {
    /// Run only the cases with a name that contains this text
    pub filter: Option<&'a str>,
    /// Ignore the cases with an index below this value
    pub start: usize,
    /// Select at most this number of cases, after `start` and `filter`
    pub limit: Option<usize>,
    /// Also run the cases that stop the boot (see `Case::ends_boot`)
    pub full: bool,
    /// Print the cases and run none
    pub list: bool,
    /// Kernel only: run the kernel suite
    pub kernel: bool,
    /// Kernel only: start the user test program after the kernel suite
    pub user: bool,
}

impl<'a> Default for Args<'a> {
    fn default() -> Self {
        Self {
            filter: None,
            start: 0,
            limit: None,
            full: false,
            list: false,
            kernel: true,
            user: true,
        }
    }
}

impl<'a> Args<'a> {
    pub fn parse(text: &'a str) -> Self {
        let mut args = Self::default();

        for token in text.split_ascii_whitespace() {
            let Some((key, value)) = token.split_once('=') else {
                continue;
            };

            match key {
                "filter" => args.filter = Some(value),
                "start" => args.start = value.parse().unwrap_or(0),
                "limit" => args.limit = value.parse().ok(),
                "full" => args.full = value == "1",
                "list" => args.list = value == "1",
                "kernel" => args.kernel = value == "1",
                "user" => args.user = value == "1",
                _ => (),
            }
        }

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_gives_defaults() {
        assert_eq!(Args::parse(""), Args::default());
    }

    #[test]
    fn parses_each_key() {
        let args = Args::parse("filter=fat:: start=3 limit=1 full=1 list=1 kernel=0 user=0");
        assert_eq!(args.filter, Some("fat::"));
        assert_eq!(args.start, 3);
        assert_eq!(args.limit, Some(1));
        assert!(args.full);
        assert!(args.list);
        assert!(!args.kernel);
        assert!(!args.user);
    }

    #[test]
    fn ignores_unknown_and_malformed_tokens() {
        let args = Args::parse("bogus start=x other=1 start=2");
        assert_eq!(args.start, 2);
    }
}
