use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// The default location for commands and files.
const DEFAULT_LOCATION: &str = "/scratch";

// This struct represents a task.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
  pub name: String,

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

// This struct represents a job.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
  pub image: String,
  pub tasks: Vec<Task>,
}

// Parse config data.
pub fn parse(job: &str) -> Result<Job, String> {
  serde_yaml::from_str(job).map_err(|e| format!("{}", e))
}

// Build a map from task name to task ID.
pub fn index(job: &Job) -> Result<HashMap<String, usize>, String> {
  let mut job_index = HashMap::new();
  for i in 0..job.tasks.len() {
    if job_index.contains_key(&job.tasks[i].name) {
      return Err(
        format!("Duplicate task name: `{}`.", job.tasks[i].name).to_owned(),
      );
    } else {
      job_index.insert(job.tasks[i].name.clone(), i);
    }
  }
  Ok(job_index)
}

#[cfg(test)]
mod tests {
  use crate::job::{index, parse, Job, Task, DEFAULT_LOCATION};
  use std::collections::HashMap;

  #[test]
  fn parse_empty() {
    let input = r#"
image: ubuntu:bionic
tasks: []
    "#
    .trim();

    let job = Ok(Job {
      image: "ubuntu:bionic".to_owned(),
      tasks: vec![],
    });

    assert_eq!(parse(input), job);
  }

  #[test]
  fn parse_minimal_task() {
    let input = r#"
image: ubuntu:bionic
tasks:
  - name: build
    "#
    .trim();

    let job = Ok(Job {
      image: "ubuntu:bionic".to_owned(),
      tasks: vec![Task {
        name: "build".to_owned(),
        dependencies: vec![],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      }],
    });

    assert_eq!(parse(input), job);
  }

  #[test]
  fn parse_comprehensive_task() {
    let input = r#"
image: ubuntu:bionic
tasks:
  - name: build
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

    let job = Ok(Job {
      image: "ubuntu:bionic".to_owned(),
      tasks: vec![Task {
        name: "build".to_owned(),
        dependencies: vec!["install_rust".to_owned()],
        cache: true,
        args: args,
        files: vec![
          "Cargo.lock".to_owned(),
          "Cargo.toml".to_owned(),
          "src/*".to_owned(),
        ],
        location: "/code".to_owned(),
        command: Some("cargo build".to_owned()),
      }],
    });

    assert_eq!(parse(input), job);
  }

  #[test]
  fn index_empty() {
    let job = Job {
      image: "ubuntu:bionic".to_owned(),
      tasks: vec![],
    };

    let job_index = HashMap::new();

    assert_eq!(index(&job), Ok(job_index));
  }

  #[test]
  fn index_no_dupes() {
    let job = Job {
      image: "ubuntu:bionic".to_owned(),
      tasks: vec![
        Task {
          name: "build".to_owned(),
          dependencies: vec![],
          cache: true,
          args: HashMap::new(),
          files: vec![],
          location: DEFAULT_LOCATION.to_owned(),
          command: None,
        },
        Task {
          name: "test".to_owned(),
          dependencies: vec![],
          cache: true,
          args: HashMap::new(),
          files: vec![],
          location: DEFAULT_LOCATION.to_owned(),
          command: None,
        },
      ],
    };

    let mut job_index = HashMap::new();
    job_index.insert("build".to_owned(), 0);
    job_index.insert("test".to_owned(), 1);

    assert_eq!(index(&job), Ok(job_index));
  }

  #[test]
  fn index_dupes() {
    let job = Job {
      image: "ubuntu:bionic".to_owned(),
      tasks: vec![
        Task {
          name: "build".to_owned(),
          dependencies: vec![],
          cache: true,
          args: HashMap::new(),
          files: vec![],
          location: DEFAULT_LOCATION.to_owned(),
          command: None,
        },
        Task {
          name: "build".to_owned(),
          dependencies: vec![],
          cache: true,
          args: HashMap::new(),
          files: vec![],
          location: DEFAULT_LOCATION.to_owned(),
          command: None,
        },
      ],
    };

    assert!(index(&job).is_err());
  }
}
