use crate::bakefile::Task;
use sha2::{Digest, Sha256};

// Determine the cache ID of a prefix of a schedule.
pub fn key(from_image: &str, schedule_prefix: &[&Task]) -> String {
  let mut cache_key = hash(from_image);

  for task in schedule_prefix {
    cache_key = combine(&cache_key, &task.location);
    if let Some(c) = &task.command {
      cache_key = combine(&cache_key, &c);
    }
  }

  cache_key[..48].to_owned()
}

fn hash(input: &str) -> String {
  hex::encode(Sha256::digest(input.as_bytes()))
}

fn combine(x: &str, y: &str) -> String {
  hash(&format!("{}{}", x, y))
}

#[cfg(test)]
mod tests {
  use crate::cache::{combine, hash};

  #[test]
  fn hash_pure() {
    assert_eq!(hash("foo"), hash("foo"));
  }

  #[test]
  fn hash_not_constant() {
    assert_ne!(hash("foo"), hash("bar"));
  }

  #[test]
  fn combine_pure() {
    assert_eq!(combine("foo", "bar"), combine("foo", "bar"));
  }

  #[test]
  fn combine_not_constant() {
    assert_ne!(combine("foo", "bar"), combine("foo", "baz"));
    assert_ne!(combine("foo", "bar"), combine("baz", "bar"));
  }
}
