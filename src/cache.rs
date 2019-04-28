use crate::bakefile::Task;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io, io::Read};

// Determine the cache ID of a prefix of a schedule.
pub fn key(
  from_image: &str,
  schedule_prefix: &[(&Task, String)],
  env: &HashMap<String, String>,
) -> String {
  let mut cache_key = hash(from_image);

  for (task, files_hash) in schedule_prefix {
    for var in task.env.keys() {
      cache_key = combine(&cache_key, var);
      cache_key = combine(&cache_key, &env[var]); // [ref:env_valid]
    }
    cache_key = combine(&cache_key, &files_hash);
    cache_key = combine(&cache_key, &task.location);
    cache_key = combine(&cache_key, &task.user);
    if let Some(c) = &task.command {
      cache_key = combine(&cache_key, &c);
    }
  }

  cache_key[..48].to_owned()
}

// Compute the hash of a string.
pub fn hash(input: &str) -> String {
  hex::encode(Sha256::digest(input.as_bytes()))
}

// Compute the hash of a readable object.
pub fn hash_read<R: Read>(input: &mut R) -> Result<String, String> {
  let mut hasher = Sha256::new();
  io::copy(input, &mut hasher)
    .map_err(|e| format!("Unable to compute hash. Details: {}", e))?;
  Ok(hex::encode(hasher.result()))
}

// Combine two hashes.
pub fn combine(x: &str, y: &str) -> String {
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
    let full_env = HashMap::new();

    assert_eq!(
      key(from_image, &schedule_prefix, &full_env),
      key(from_image, &schedule_prefix, &full_env)
    );
  }

  #[test]
  fn key_pure() {
    let mut env: HashMap<String, Option<String>> = HashMap::new();
    env.insert("foo".to_owned(), None);

    let task = Task {
      dependencies: vec![],
      cache: true,
      env,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix = vec![(&task, "foo".to_owned())];
    let mut full_env = HashMap::new();
    full_env.insert("foo".to_owned(), "qux".to_owned());

    assert_eq!(
      key(from_image, &schedule_prefix, &full_env),
      key(from_image, &schedule_prefix, &full_env)
    );
  }

  #[test]
  fn key_env_keys() {
    let mut env1: HashMap<String, Option<String>> = HashMap::new();
    env1.insert("foo".to_owned(), None);

    let mut env2: HashMap<String, Option<String>> = HashMap::new();
    env2.insert("bar".to_owned(), None);

    let task1 = Task {
      dependencies: vec![],
      cache: true,
      env: env1,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      env: env2,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![(&task1, "foo".to_owned())];
    let schedule_prefix2 = vec![(&task2, "foo".to_owned())];
    let mut full_env = HashMap::new();
    full_env.insert("foo".to_owned(), "qux".to_owned());
    full_env.insert("bar".to_owned(), "fum".to_owned());

    assert_ne!(
      key(from_image, &schedule_prefix1, &full_env),
      key(from_image, &schedule_prefix2, &full_env)
    );
  }

  #[test]
  fn key_env_values() {
    let mut env: HashMap<String, Option<String>> = HashMap::new();
    env.insert("foo".to_owned(), None);

    let task = Task {
      dependencies: vec![],
      cache: true,
      env,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix = vec![(&task, "foo".to_owned())];
    let mut full_env1 = HashMap::new();
    full_env1.insert("foo".to_owned(), "bar".to_owned());
    let mut full_env2 = HashMap::new();
    full_env2.insert("foo".to_owned(), "baz".to_owned());

    assert_ne!(
      key(from_image, &schedule_prefix, &full_env1),
      key(from_image, &schedule_prefix, &full_env2)
    );
  }

  #[test]
  fn key_files_hash() {
    let task = Task {
      dependencies: vec![],
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![(&task, "foo".to_owned())];
    let schedule_prefix2 = vec![(&task, "bar".to_owned())];
    let full_env = HashMap::new();

    assert_ne!(
      key(from_image, &schedule_prefix1, &full_env),
      key(from_image, &schedule_prefix2, &full_env)
    );
  }

  #[test]
  fn key_location() {
    let task1 = Task {
      dependencies: vec![],
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: "/foo".to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: "/bar".to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![(&task1, "foo".to_owned())];
    let schedule_prefix2 = vec![(&task2, "foo".to_owned())];
    let full_env = HashMap::new();

    assert_ne!(
      key(from_image, &schedule_prefix1, &full_env),
      key(from_image, &schedule_prefix2, &full_env)
    );
  }

  #[test]
  fn key_user() {
    let task1 = Task {
      dependencies: vec![],
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: "foo".to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: "bar".to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![(&task1, "foo".to_owned())];
    let schedule_prefix2 = vec![(&task2, "foo".to_owned())];
    let full_env = HashMap::new();

    assert_ne!(
      key(from_image, &schedule_prefix1, &full_env),
      key(from_image, &schedule_prefix2, &full_env)
    );
  }

  #[test]
  fn key_command_different() {
    let task1 = Task {
      dependencies: vec![],
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wobble".to_owned()),
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![(&task1, "foo".to_owned())];
    let schedule_prefix2 = vec![(&task2, "foo".to_owned())];
    let full_env = HashMap::new();

    assert_ne!(
      key(from_image, &schedule_prefix1, &full_env),
      key(from_image, &schedule_prefix2, &full_env)
    );
  }

  #[test]
  fn key_command_some_none() {
    let task1 = Task {
      dependencies: vec![],
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: None,
    };

    let from_image = "ubuntu:18.04";
    let schedule_prefix1 = vec![(&task1, "foo".to_owned())];
    let schedule_prefix2 = vec![(&task2, "foo".to_owned())];
    let full_env = HashMap::new();

    assert_ne!(
      key(from_image, &schedule_prefix1, &full_env),
      key(from_image, &schedule_prefix2, &full_env)
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
