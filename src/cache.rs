use crate::bakefile::Task;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

// Determine the cache ID of a prefix of a schedule.
pub fn key(
  from_image: &str,
  schedule_prefix: &[&Task],
  args: &HashMap<String, String>,
) -> String {
  let mut cache_key = hash(from_image);

  for task in schedule_prefix {
    for arg in task.args.keys() {
      cache_key = combine(&cache_key, arg);
      cache_key = combine(&cache_key, &args[arg]); // [ref:args_valid]
    }
    cache_key = combine(&cache_key, &task.location);
    cache_key = combine(&cache_key, &task.user);
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
  use crate::{
    bakefile::{Task, DEFAULT_LOCATION, DEFAULT_USER},
    cache::{combine, hash, key},
  };
  use std::collections::HashMap;

  #[test]
  fn key_simple() {
    let from_image = "ubuntu:18.04";
    let schedule_prefix = vec![];
    let all_args = HashMap::new();

    assert_eq!(
      key(from_image, &schedule_prefix, &all_args),
      key(from_image, &schedule_prefix, &all_args)
    );
  }

  #[test]
  fn key_pure() {
    let mut args: HashMap<String, Option<String>> = HashMap::new();
    args.insert("foo".to_owned(), None);

    let task = Task {
      dependencies: vec![],
      cache: true,
      args,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix = vec![&task];
    let mut all_args = HashMap::new();
    all_args.insert("foo".to_owned(), "qux".to_owned());

    assert_eq!(
      key(from_image, &schedule_prefix, &all_args),
      key(from_image, &schedule_prefix, &all_args)
    );
  }

  #[test]
  fn key_args_keys() {
    let mut args1: HashMap<String, Option<String>> = HashMap::new();
    args1.insert("foo".to_owned(), None);

    let mut args2: HashMap<String, Option<String>> = HashMap::new();
    args2.insert("bar".to_owned(), None);

    let task1 = Task {
      dependencies: vec![],
      cache: true,
      args: args1,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      args: args2,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![&task1];
    let schedule_prefix2 = vec![&task2];
    let mut all_args = HashMap::new();
    all_args.insert("foo".to_owned(), "qux".to_owned());
    all_args.insert("bar".to_owned(), "fum".to_owned());

    assert_ne!(
      key(from_image, &schedule_prefix1, &all_args),
      key(from_image, &schedule_prefix2, &all_args)
    );
  }

  #[test]
  fn key_args_values() {
    let mut args: HashMap<String, Option<String>> = HashMap::new();
    args.insert("foo".to_owned(), None);

    let task = Task {
      dependencies: vec![],
      cache: true,
      args,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix = vec![&task];
    let mut all_args1 = HashMap::new();
    all_args1.insert("foo".to_owned(), "bar".to_owned());
    let mut all_args2 = HashMap::new();
    all_args2.insert("foo".to_owned(), "baz".to_owned());

    assert_ne!(
      key(from_image, &schedule_prefix, &all_args1),
      key(from_image, &schedule_prefix, &all_args2)
    );
  }

  #[test]
  fn key_location() {
    let task1 = Task {
      dependencies: vec![],
      cache: true,
      args: HashMap::new(),
      paths: vec![],
      location: "/foo".to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      args: HashMap::new(),
      paths: vec![],
      location: "/bar".to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![&task1];
    let schedule_prefix2 = vec![&task2];
    let all_args = HashMap::new();

    assert_ne!(
      key(from_image, &schedule_prefix1, &all_args),
      key(from_image, &schedule_prefix2, &all_args)
    );
  }

  #[test]
  fn key_user() {
    let task1 = Task {
      dependencies: vec![],
      cache: true,
      args: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: "foo".to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      args: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: "bar".to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![&task1];
    let schedule_prefix2 = vec![&task2];
    let all_args = HashMap::new();

    assert_ne!(
      key(from_image, &schedule_prefix1, &all_args),
      key(from_image, &schedule_prefix2, &all_args)
    );
  }

  #[test]
  fn key_command_different() {
    let task1 = Task {
      dependencies: vec![],
      cache: true,
      args: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      args: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wobble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![&task1];
    let schedule_prefix2 = vec![&task2];
    let all_args = HashMap::new();

    assert_ne!(
      key(from_image, &schedule_prefix1, &all_args),
      key(from_image, &schedule_prefix2, &all_args)
    );
  }

  #[test]
  fn key_command_some_none() {
    let task1 = Task {
      dependencies: vec![],
      cache: true,
      args: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      args: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: None,
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![&task1];
    let schedule_prefix2 = vec![&task2];
    let all_args = HashMap::new();

    assert_ne!(
      key(from_image, &schedule_prefix1, &all_args),
      key(from_image, &schedule_prefix2, &all_args)
    );
  }

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
