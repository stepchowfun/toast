use crate::format;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env};

// The default location for commands and paths.
pub const DEFAULT_LOCATION: &str = "/scratch";

// The default user for commands and paths.
pub const DEFAULT_USER: &str = "root";

// This struct represents a task.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
  #[serde(default)]
  pub dependencies: Vec<String>,

  #[serde(default = "default_task_cache")]
  pub cache: bool,

  #[serde(default)]
  pub env: HashMap<String, Option<String>>,

  #[serde(default)]
  pub paths: Vec<String>,

  #[serde(default = "default_task_location")]
  pub location: String,

  #[serde(default = "default_task_user")]
  pub user: String,

  pub command: Option<String>,
}

fn default_task_cache() -> bool {
  true
}

fn default_task_location() -> String {
  DEFAULT_LOCATION.to_owned()
}

fn default_task_user() -> String {
  DEFAULT_USER.to_owned()
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
  let bakefile =
    serde_yaml::from_str(bakefile).map_err(|e| format!("{}", e))?;
  check_dependencies(&bakefile)?;
  Ok(bakefile)
}

// Fetch the variables for a task from the environment.
pub fn environment<'a>(
  task: &'a Task,
) -> Result<HashMap<String, String>, Vec<&'a str>> {
  let mut violations = vec![];
  let mut result = HashMap::new();
  for (arg, default) in &task.env {
    let maybe_var = env::var(arg);
    if let Some(default) = default {
      result
        .insert(arg.clone(), maybe_var.unwrap_or_else(|_| default.clone()));
    } else if let Ok(var) = maybe_var {
      result.insert(arg.clone(), var);
    } else {
      violations.push(&arg[..]);
    }
  }

  if !violations.is_empty() {
    return Err(violations);
  }

  Ok(result)
}

// Check that all dependencies exist.
fn check_dependencies(bakefile: &Bakefile) -> Result<(), String> {
  let mut violations: HashMap<String, Vec<String>> = HashMap::new();
  // [tag:task_valid]
  for task in bakefile.tasks.keys() {
    // [ref:task_valid]
    for dependency in &bakefile.tasks[task].dependencies {
      if !bakefile.tasks.contains_key(dependency) {
        violations
          .entry(task.to_owned())
          .or_insert_with(|| vec![])
          .push(dependency.to_owned());
      }
    }
  }

  if !violations.is_empty() {
    return Err(format!(
      "The following tasks have invalid dependencies: {}.",
      format::series(
        &violations
          .iter()
          .map(|(task, dependencies)| format!(
            "`{}` ({})",
            task,
            format::series(
              &dependencies
                .iter()
                .map(|task| format!("`{}`", task))
                .collect::<Vec<_>>()[..]
            )
          ))
          .collect::<Vec<_>>()[..]
      )
    ));
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use crate::bakefile::{
    check_dependencies, environment, parse, Bakefile, Task, DEFAULT_LOCATION,
    DEFAULT_USER,
  };
  use std::{collections::HashMap, env};

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
        env: HashMap::new(),
        paths: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        user: DEFAULT_USER.to_owned(),
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
  install_rust: {}
  build:
    dependencies:
      - install_rust
    cache: true
    env:
      AWS_ACCESS_KEY_ID: null
      AWS_DEFAULT_REGION: null
      AWS_SECRET_ACCESS_KEY: null
    paths:
      - Cargo.lock
      - Cargo.toml
      - src/*
    location: /code
    user: foo
    command: cargo build
    "#
    .trim();

    let mut env = HashMap::new();
    env.insert("AWS_ACCESS_KEY_ID".to_owned(), None);
    env.insert("AWS_DEFAULT_REGION".to_owned(), None);
    env.insert("AWS_SECRET_ACCESS_KEY".to_owned(), None);

    let mut tasks = HashMap::new();
    tasks.insert(
      "install_rust".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        env: HashMap::new(),
        paths: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "build".to_owned(),
      Task {
        dependencies: vec!["install_rust".to_owned()],
        cache: true,
        env,
        paths: vec![
          "Cargo.lock".to_owned(),
          "Cargo.toml".to_owned(),
          "src/*".to_owned(),
        ],
        location: "/code".to_owned(),
        user: "foo".to_owned(),
        command: Some("cargo build".to_owned()),
      },
    );

    let bakefile = Ok(Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks,
    });

    assert_eq!(parse(input), bakefile);
  }

  #[test]
  fn environment_empty() {
    let task = Task {
      dependencies: vec![],
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: None,
    };

    assert_eq!(environment(&task), Ok(HashMap::new()));
  }

  #[test]
  fn environment_default_overridden() {
    // NOTE: We add an index to the test arg (foo1, foo2, ...) to avoid having
    // parallel tests clobbering environment variables used by other threads.
    let mut env_map = HashMap::new();
    env_map.insert("foo1".to_owned(), Some("bar".to_owned()));

    let task = Task {
      dependencies: vec![],
      cache: true,
      env: env_map,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: None,
    };

    let mut expected = HashMap::new();
    expected.insert("foo1".to_owned(), "baz".to_owned());

    env::set_var("foo1", "baz");
    assert_eq!(env::var("foo1"), Ok("baz".to_owned()));
    assert_eq!(environment(&task), Ok(expected));
  }

  #[test]
  fn environment_default_not_overridden() {
    // NOTE: We add an index to the test arg (foo1, foo2, ...) to avoid having
    // parallel tests clobbering environment variables used by other threads.
    let mut env_map = HashMap::new();
    env_map.insert("foo2".to_owned(), Some("bar".to_owned()));

    let task = Task {
      dependencies: vec![],
      cache: true,
      env: env_map,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: None,
    };

    let mut expected = HashMap::new();
    expected.insert("foo2".to_owned(), "bar".to_owned());

    env::remove_var("foo2");
    assert!(env::var("foo2").is_err());
    assert_eq!(environment(&task), Ok(expected));
  }

  #[test]
  fn environment_missing() {
    // NOTE: We add an index to the test arg (foo1, foo2, ...) to avoid having
    // parallel tests clobbering environment variables used by other threads.
    let mut env_map = HashMap::new();
    env_map.insert("foo3".to_owned(), None);

    let task = Task {
      dependencies: vec![],
      cache: true,
      env: env_map,
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: None,
    };

    let mut expected = HashMap::new();
    expected.insert("foo3".to_owned(), "bar".to_owned());

    env::remove_var("foo3");
    assert!(env::var("foo3").is_err());
    assert!(environment(&task).is_err());
  }

  #[test]
  fn check_dependencies_empty() {
    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks: HashMap::new(),
    };

    assert!(check_dependencies(&bakefile).is_ok());
  }

  #[test]
  fn check_dependencies_nonempty() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "build".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        env: HashMap::new(),
        paths: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "test".to_owned(),
      Task {
        dependencies: vec!["build".to_owned()],
        cache: true,
        env: HashMap::new(),
        paths: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks,
    };

    assert!(check_dependencies(&bakefile).is_ok());
  }

  #[test]
  fn check_dependencies_nonexistent() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "build".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        env: HashMap::new(),
        paths: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "test".to_owned(),
      Task {
        dependencies: vec!["build".to_owned(), "do_thing".to_owned()],
        cache: true,
        env: HashMap::new(),
        paths: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks,
    };

    assert!(check_dependencies(&bakefile).is_err());
  }
}
