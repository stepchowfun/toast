use crate::bakefile::Task;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io, io::Read};

// Determine the cache ID of a prefix of a schedule.
pub fn key(
  previous_key: &str,
  task: &Task,
  files_hash: &str,
  environment: &HashMap<String, String>,
) -> String {
  let mut cache_key = previous_key.to_owned();
  for var in task.environment.keys() {
    cache_key = extend(&cache_key, var);
    cache_key = extend(&cache_key, &environment[var]); // [ref:environment_valid]
  }
  cache_key = extend(&cache_key, &files_hash);
  cache_key = extend(&cache_key, &task.location.to_string_lossy());
  cache_key = extend(&cache_key, &task.user);
  if let Some(c) = &task.command {
    cache_key = extend(&cache_key, &c);
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

// Combine a hash with another string to form a new hash.
pub fn extend(x: &str, y: &str) -> String {
  hash(&format!("{}{}", x, y))
}

#[cfg(test)]
mod tests {
  use crate::{
    bakefile::{Task, DEFAULT_LOCATION, DEFAULT_USER},
    cache::{extend, hash, key},
  };
  use std::{collections::HashMap, path::Path};

  #[test]
  fn key_pure() {
    let mut environment: HashMap<String, Option<String>> = HashMap::new();
    environment.insert("foo".to_owned(), None);

    let previous_key = "foo";

    let task = Task {
      dependencies: vec![],
      cache: true,
      environment,
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let files_hash = "bar";

    let mut full_environment = HashMap::new();
    full_environment.insert("foo".to_owned(), "qux".to_owned());

    assert_eq!(
      key(previous_key, &task, files_hash, &full_environment),
      key(previous_key, &task, files_hash, &full_environment)
    );
  }

  #[test]
  fn key_environment_keys() {
    let mut environment1: HashMap<String, Option<String>> = HashMap::new();
    environment1.insert("foo".to_owned(), None);

    let mut environment2: HashMap<String, Option<String>> = HashMap::new();
    environment2.insert("bar".to_owned(), None);

    let previous_key = "foo";

    let task1 = Task {
      dependencies: vec![],
      cache: true,
      environment: environment1,
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      environment: environment2,
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let files_hash = "bar";

    let mut full_environment = HashMap::new();
    full_environment.insert("foo".to_owned(), "qux".to_owned());
    full_environment.insert("bar".to_owned(), "fum".to_owned());

    assert_ne!(
      key(previous_key, &task1, files_hash, &full_environment),
      key(previous_key, &task2, files_hash, &full_environment)
    );
  }

  #[test]
  fn key_environment_values() {
    let mut environment: HashMap<String, Option<String>> = HashMap::new();
    environment.insert("foo".to_owned(), None);

    let previous_key = "foo";

    let task = Task {
      dependencies: vec![],
      cache: true,
      environment,
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let files_hash = "bar";

    let mut full_environment1 = HashMap::new();
    full_environment1.insert("foo".to_owned(), "bar".to_owned());
    let mut full_environment2 = HashMap::new();
    full_environment2.insert("foo".to_owned(), "baz".to_owned());

    assert_ne!(
      key(previous_key, &task, files_hash, &full_environment1),
      key(previous_key, &task, files_hash, &full_environment2)
    );
  }

  #[test]
  fn key_files_hash() {
    let previous_key = "foo";

    let task = Task {
      dependencies: vec![],
      cache: true,
      environment: HashMap::new(),
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let files_hash1 = "foo";
    let files_hash2 = "bar";

    let full_environment = HashMap::new();

    assert_ne!(
      key(previous_key, &task, files_hash1, &full_environment),
      key(previous_key, &task, files_hash2, &full_environment)
    );
  }

  #[test]
  fn key_location() {
    let previous_key = "foo";

    let task1 = Task {
      dependencies: vec![],
      cache: true,
      environment: HashMap::new(),
      paths: vec![],
      location: Path::new("/foo").to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      environment: HashMap::new(),
      paths: vec![],
      location: Path::new("/bar").to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let files_hash = "bar";

    let full_environment = HashMap::new();

    assert_ne!(
      key(previous_key, &task1, files_hash, &full_environment),
      key(previous_key, &task2, files_hash, &full_environment)
    );
  }

  #[test]
  fn key_user() {
    let previous_key = "foo";

    let task1 = Task {
      dependencies: vec![],
      cache: true,
      environment: HashMap::new(),
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: "foo".to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      environment: HashMap::new(),
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: "bar".to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let files_hash = "bar";

    let full_environment = HashMap::new();

    assert_ne!(
      key(previous_key, &task1, files_hash, &full_environment),
      key(previous_key, &task2, files_hash, &full_environment)
    );
  }

  #[test]
  fn key_command_different() {
    let previous_key = "foo";

    let task1 = Task {
      dependencies: vec![],
      cache: true,
      environment: HashMap::new(),
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      environment: HashMap::new(),
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wobble".to_owned()),
    };

    let files_hash = "bar";

    let full_environment = HashMap::new();

    assert_ne!(
      key(previous_key, &task1, files_hash, &full_environment),
      key(previous_key, &task2, files_hash, &full_environment)
    );
  }

  #[test]
  fn key_command_some_none() {
    let previous_key = "foo";

    let task1 = Task {
      dependencies: vec![],
      cache: true,
      environment: HashMap::new(),
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: Some("echo wibble".to_owned()),
    };

    let task2 = Task {
      dependencies: vec![],
      cache: true,
      environment: HashMap::new(),
      paths: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: None,
    };

    let files_hash = "bar";

    let full_environment = HashMap::new();

    assert_ne!(
      key(previous_key, &task1, files_hash, &full_environment),
      key(previous_key, &task2, files_hash, &full_environment)
    )
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
  fn extend_pure() {
    assert_eq!(extend("foo", "bar"), extend("foo", "bar"));
  }

  #[test]
  fn extend_not_constant() {
    assert_ne!(extend("foo", "bar"), extend("foo", "baz"));
    assert_ne!(extend("foo", "bar"), extend("baz", "bar"));
  }
}
