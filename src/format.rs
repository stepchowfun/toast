use colored::{ColoredString, Colorize, control::SHOULD_COLORIZE};

// This trait has a function for formatting "code-like" text, such as a file path. The reason it's
// implemented as a trait and not just a function is so we can use it with method syntax, as in
// `x.code_str()`. Rust does not allow us to implement methods on primitive types such as `str`.
pub trait CodeStr {
    fn code_str(&self) -> ColoredString;
}

impl CodeStr for str {
    fn code_str(&self) -> ColoredString {
        // If colored output is enabled, format the text in magenta. Otherwise, surround it in
        // backticks.
        if SHOULD_COLORIZE.should_colorize() {
            self.magenta()
        } else {
            ColoredString::from(&format!("`{self}`") as &Self)
        }
    }
}

// This function takes a number and a noun and returns a string representing the noun with the
// given multiplicity (pluralizing if necessary). For example, (3, "cow") becomes "3 cows".
pub fn number(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("{n} {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

// This function takes an array of strings and returns a comma-separated list with the word "and"
// (and an Oxford comma, if applicable) between the last two items.
pub fn series(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => format!(
            "{}, and {}",
            items[..items.len() - 1].join(", "),
            items[items.len() - 1],
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::format::{CodeStr, number, series};

    #[test]
    fn code_str_display() {
        // This test, like many others, depends on colors being disabled [ref:colorless_tests].
        assert_eq!(format!("{}", "foo".code_str()), "`foo`");
    }

    #[test]
    fn number_zero() {
        assert_eq!(number(0, "cow"), "0 cows");
    }

    #[test]
    fn number_one() {
        assert_eq!(number(1, "cow"), "1 cow");
    }

    #[test]
    fn number_two() {
        assert_eq!(number(2, "cow"), "2 cows");
    }

    #[test]
    fn series_empty() {
        assert_eq!(series(&[]), "");
    }

    #[test]
    fn series_one() {
        assert_eq!(series(&["foo".to_owned()]), "foo");
    }

    #[test]
    fn series_two() {
        assert_eq!(series(&["foo".to_owned(), "bar".to_owned()]), "foo and bar");
    }

    #[test]
    fn series_three() {
        assert_eq!(
            series(&["foo".to_owned(), "bar".to_owned(), "baz".to_owned()]),
            "foo, bar, and baz",
        );
    }
}
