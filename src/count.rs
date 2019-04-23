// This function takes a number and a noun and returns a string representing
// the noun with the given multiplicity (pluralizing if necessary). For
// example, (3, "cow") becomes "3 cows".
pub fn count(n: usize, noun: &str) -> String {
  if n == 1 {
    format!("{} {}", n, noun)
  } else {
    format!("{} {}s", n, noun)
  }
}

#[cfg(test)]
mod tests {
  use crate::count::count;

  #[test]
  fn count_zero() {
    assert_eq!(count(0, "cow"), "0 cows");
  }

  #[test]
  fn count_one() {
    assert_eq!(count(1, "cow"), "1 cow");
  }

  #[test]
  fn count_two() {
    assert_eq!(count(2, "cow"), "2 cows");
  }
}
