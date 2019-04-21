use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// The default location for commands and files.
pub const DEFAULT_LOCATION: &str = "/scratch";

// This struct represents a task.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
  #[serde(default)]
  pub dependencies: Vec<String>,

  #[serde(default = "default_task_cache")]
  pub cache: bool,

  #[serde(default)]
  pub args: HashMap<String, Option<String>>,

  #[serde(default)]
  pub files: Vec<String>,

  #[serde(default = "default_task_location")]
  pub location: String,

  pub command: Option<String>,
}

fn default_task_cache() -> bool {
  true
}

fn default_task_location() -> String {
  DEFAULT_LOCATION.to_owned()
}

// This struct represents a bakefile.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bakefile {
  pub image: String,
  pub tasks: HashMap<String, Task>,
}

// Parse config data.
pub fn parse(bakefile: &str) -> Result<Bakefile, String> {
  serde_yaml::from_str(bakefile).map_err(|e| format!("{}", e))
}

#[cfg(test)]
mod tests {
  use crate::bakefile::{parse, Bakefile, Task, DEFAULT_LOCATION};
  use std::collections::HashMap;

  #[test]
  fn parse_empty() {
    let input = r#"
image: ubuntu:18.04
tasks: {}
    "#
    .trim();

    let bakefile = Ok(Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks: HashMap::new(),
    });

    assert_eq!(parse(input), bakefile);
  }

  #[test]
  fn parse_minimal_task() {
    let input = r#"
image: ubuntu:18.04
tasks:
  build: {}
    "#
    .trim();

    let mut tasks = HashMap::new();
    tasks.insert(
      "build".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );

    let bakefile = Ok(Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks,
    });

    assert_eq!(parse(input), bakefile);
  }

  #[test]
  fn parse_comprehensive_task() {
    let input = r#"
image: ubuntu:18.04
tasks:
  build:
    dependencies:
      - install_rust
    cache: true
    args:
      AWS_ACCESS_KEY_ID: null
      AWS_DEFAULT_REGION: null
      AWS_SECRET_ACCESS_KEY: null
    files:
      - Cargo.lock
      - Cargo.toml
      - src/*
    location: /code
    command: cargo build
    "#
    .trim();

    let mut args = HashMap::new();
    args.insert("AWS_ACCESS_KEY_ID".to_owned(), None);
    args.insert("AWS_DEFAULT_REGION".to_owned(), None);
    args.insert("AWS_SECRET_ACCESS_KEY".to_owned(), None);

    let mut tasks = HashMap::new();
    tasks.insert(
      "build".to_owned(),
      Task {
        dependencies: vec!["install_rust".to_owned()],
        cache: true,
        args,
        files: vec![
          "Cargo.lock".to_owned(),
          "Cargo.toml".to_owned(),
          "src/*".to_owned(),
        ],
        location: "/code".to_owned(),
        command: Some("cargo build".to_owned()),
      },
    );

    let bakefile = Ok(Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks,
    });

    assert_eq!(parse(input), bakefile);
  }
}
