use crate::{format, format::CodeStr};
use serde::{Deserialize, Serialize};
use std::{
  collections::{HashMap, HashSet},
  env,
  path::{Path, PathBuf},
};

// The default location for commands and files copied into the container
pub const DEFAULT_LOCATION: &str = "/scratch";

// The default user for commands and files copied into the container
pub const DEFAULT_USER: &str = "root";

// This struct represents a task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
  #[serde(default)]
  pub dependencies: Vec<String>,

  #[serde(default = "default_task_cache")]
  pub cache: bool,

  #[serde(default)]
  pub environment: HashMap<String, Option<String>>,

  #[serde(default)]
  pub input_paths: Vec<PathBuf>,

  #[serde(default)]
  pub output_paths: Vec<PathBuf>,

  #[serde(default)]
  pub ports: Vec<String>,

  #[serde(default = "default_task_location")]
  pub location: PathBuf,

  #[serde(default = "default_task_user")]
  pub user: String,

  pub command: Option<String>,
}

fn default_task_cache() -> bool {
  true
}

fn default_task_location() -> PathBuf {
  Path::new(DEFAULT_LOCATION).to_owned()
}

fn default_task_user() -> String {
  DEFAULT_USER.to_owned()
}

// This struct represents a bakefile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bakefile {
  pub image: String,
  pub default: Option<String>,
  pub tasks: HashMap<String, Task>,
}

// Parse config data.
pub fn parse(bakefile_data: &str) -> Result<Bakefile, String> {
  // Deserialize the data.
  let bakefile: Bakefile =
    serde_yaml::from_str(bakefile_data).map_err(|e| format!("{}", e))?;

  // Make sure the paths are valid.
  check_paths(&bakefile)?;

  // Make sure the dependencies are valid.
  check_dependencies(&bakefile)?;

  // Return the bakefile.
  Ok(bakefile)
}

// Fetch the variables for a task from the environment.
pub fn environment<'a>(
  task: &'a Task,
) -> Result<HashMap<String, String>, Vec<&'a str>> {
  let mut violations = vec![];
  let mut result = HashMap::new();

  for (arg, default) in &task.environment {
    let maybe_var = env::var(arg);
    if let Some(default) = default {
      result
        .insert(arg.clone(), maybe_var.unwrap_or_else(|_| default.clone()));
    } else if let Ok(var) = maybe_var {
      result.insert(arg.clone(), var);
    } else {
      violations.push(arg.as_ref());
    }
  }

  if violations.is_empty() {
    Ok(result)
  } else {
    Err(violations)
  }
}

// Check that paths that should be relative are, and likewise for paths that
// should be absolute.
fn check_paths(bakefile: &Bakefile) -> Result<(), String> {
  for (name, task) in &bakefile.tasks {
    // Check `input_paths`.
    for path in &task.input_paths {
      if path.is_absolute() {
        return Err(format!(
          "Task {} has an absolute {}: {}.",
          name.code_str(),
          "input_path".code_str(),
          path.to_string_lossy().code_str()
        ));
      }
    }

    // Check `output_paths`.
    for path in &task.output_paths {
      if path.is_absolute() {
        return Err(format!(
          "Task {} has an absolute {}: {}.",
          name.code_str(),
          "ouput_path".code_str(),
          path.to_string_lossy().code_str()
        ));
      }
    }

    // Check `location`.
    if task.location.is_relative() {
      return Err(format!(
        "Task {} has a relative {}: {}.",
        name.code_str(),
        "location".code_str(),
        task.location.to_string_lossy().code_str()
      ));
    }
  }

  Ok(())
}

// Check that all dependencies exist and form a DAG (no cycles).
// [tag:tasks_dag]
fn check_dependencies<'a>(bakefile: &'a Bakefile) -> Result<(), String> {
  // Check the default task. [tag:valid_default]
  let valid_default = bakefile
    .default
    .as_ref()
    .map_or(true, |default| bakefile.tasks.contains_key(default));

  // Map from task to vector of invalid dependencies.
  let mut violations: HashMap<String, Vec<String>> = HashMap::new();

  // Scan for invalid dependencies. [tag:task_valid]
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

  // If there were any invalid dependencies, report them.
  if !violations.is_empty() {
    let violations_series = format::series(
      violations
        .iter()
        .map(|(task, dependencies)| {
          format!(
            "{} ({})",
            task.code_str(),
            format::series(
              dependencies
                .iter()
                .map(|task| format!("{}", task.code_str()))
                .collect::<Vec<_>>()
                .as_ref()
            )
          )
        })
        .collect::<Vec<_>>()
        .as_ref(),
    );

    if valid_default {
      return Err(format!(
        "The following tasks have invalid dependencies: {}.",
        violations_series
      ));
    } else {
      return Err(format!(
        "The default task {} does not exist, and the following tasks have invalid dependencies: {}.",
        bakefile.default.as_ref().unwrap().code_str(), // [ref:valid_default]
        violations_series
      ));
    }
  } else if !valid_default {
    return Err(format!(
      "The default task {} does not exist.",
      bakefile.default.as_ref().unwrap().code_str() // [ref:valid_default]
    ));
  }

  // Check that the dependencies aren't cyclic.
  let mut visited: HashSet<&'a str> = HashSet::new();
  for task in bakefile.tasks.keys() {
    let mut frontier: Vec<(&'a str, usize)> = vec![(task, 0)];
    let mut ancestors_set: HashSet<&'a str> = HashSet::new();
    let mut ancestors_stack: Vec<&'a str> = vec![];

    // Keep going as long as there are more nodes to process.
    // [tag:bakefile_frontier_nonempty]
    while !frontier.is_empty() {
      // Take the top task from the frontier. This is safe due to
      // [ref:bakefile_frontier_nonempty].
      let (task, task_depth) = frontier.pop().unwrap();

      // Update the ancestors set and stack.
      for _ in 0..ancestors_stack.len() - task_depth {
        // The `unwrap` is safe because `ancestors_stack.len()` is positive in
        // every iteration of this loop.
        let task_to_remove = ancestors_stack.pop().unwrap();
        ancestors_set.remove(task_to_remove);
      }

      // If this task is an ancestor of itself, we have a cycle. Return an
      // error.
      if ancestors_set.contains(task) {
        let mut cycle_iter = ancestors_stack.iter();
        cycle_iter.find(|&&x| x == task);
        let mut cycle = cycle_iter.collect::<Vec<_>>();
        cycle.push(&task); // [tag:cycle_nonempty]
        let error_message = if cycle.len() == 1 {
          format!("{} depends on itself.", cycle[0].code_str())
        } else if cycle.len() == 2 {
          format!(
            "{} and {} depend on each other.",
            cycle[0].code_str(),
            cycle[1].code_str()
          )
        } else {
          let mut cycle_dependencies = cycle[1..].to_owned();
          cycle_dependencies.push(cycle[0]); // [ref:cycle_nonempty]
          format!(
            "{}.",
            format::series(
              cycle
                .iter()
                .zip(cycle_dependencies)
                .map(|(x, y)| format!(
                  "{} depends on {}",
                  x.code_str(),
                  y.code_str()
                ))
                .collect::<Vec<_>>()
                .as_ref(),
            )
          )
        };
        return Err(format!("The dependencies are cyclic. {}", error_message));
      }

      // If we've never seen this task before, add its dependencies to the
      // frontier.
      if !visited.contains(task) {
        visited.insert(task);

        ancestors_set.insert(task);
        ancestors_stack.push(task);

        for dependency in &bakefile.tasks[task].dependencies {
          frontier.push((dependency, task_depth + 1));
        }
      }
    }
  }

  // No violations
  Ok(())
}

#[cfg(test)]
mod tests {
  use crate::bakefile::{
    check_dependencies, environment, parse, Bakefile, Task, DEFAULT_LOCATION,
    DEFAULT_USER,
  };
  use std::{collections::HashMap, env, path::Path};

  #[test]
  fn parse_empty() {
    let input = r#"
image: encom:os-12
tasks: {}
    "#
    .trim();

    let bakefile = Ok(Bakefile {
      image: "encom:os-12".to_owned(),
      default: None,
      tasks: HashMap::new(),
    });

    assert_eq!(parse(input), bakefile);
  }

  #[test]
  fn parse_minimal_task() {
    let input = r#"
image: encom:os-12
tasks:
  foo: {}
    "#
    .trim();

    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );

    let bakefile = Ok(Bakefile {
      image: "encom:os-12".to_owned(),
      default: None,
      tasks,
    });

    assert_eq!(parse(input), bakefile);
  }

  #[test]
  fn parse_valid_default() {
    let input = r#"
image: encom:os-12
default: foo
tasks:
  foo: {}
    "#
    .trim();

    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );

    let bakefile = Ok(Bakefile {
      image: "encom:os-12".to_owned(),
      default: Some("foo".to_owned()),
      tasks,
    });

    assert_eq!(parse(input), bakefile);
  }

  #[test]
  fn parse_invalid_default() {
    let input = r#"
image: encom:os-12
default: bar
tasks:
  foo: {}
    "#
    .trim();

    let result = parse(input);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("bar"));
  }

  #[test]
  fn parse_comprehensive_task() {
    let input = r#"
image: encom:os-12
tasks:
  foo: {}
  bar:
    dependencies:
      - foo
    cache: true
    environment:
      SPAM: null
      HAM: null
      EGGS: null
    input_paths:
      - qux
      - quux
      - quuz
    output_paths:
      - corge
      - grault
      - garply
    ports:
      - 3000
      - 3001
      - 3002
    location: /code
    user: waldo
    command: wibble
    "#
    .trim();

    let mut environment = HashMap::new();
    environment.insert("SPAM".to_owned(), None);
    environment.insert("HAM".to_owned(), None);
    environment.insert("EGGS".to_owned(), None);

    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "bar".to_owned(),
      Task {
        dependencies: vec!["foo".to_owned()],
        cache: true,
        environment,
        input_paths: vec![
          Path::new("qux").to_owned(),
          Path::new("quux").to_owned(),
          Path::new("quuz").to_owned(),
        ],
        output_paths: vec![
          Path::new("corge").to_owned(),
          Path::new("grault").to_owned(),
          Path::new("garply").to_owned(),
        ],
        ports: vec!["3000".to_owned(), "3001".to_owned(), "3002".to_owned()],
        location: Path::new("/code").to_owned(),
        user: "waldo".to_owned(),
        command: Some("wibble".to_owned()),
      },
    );

    let bakefile = Ok(Bakefile {
      image: "encom:os-12".to_owned(),
      default: None,
      tasks,
    });

    assert_eq!(parse(input), bakefile);
  }

  #[test]
  fn environment_empty() {
    let task = Task {
      dependencies: vec![],
      cache: true,
      environment: HashMap::new(),
      input_paths: vec![],
      output_paths: vec![],
      ports: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: None,
    };

    assert_eq!(environment(&task), Ok(HashMap::new()));
  }

  #[test]
  fn environment_default_overridden() {
    // NOTE: We add an index to the test arg ("foo1", "foo2", ...) to avoid
    // having parallel tests clobbering environment variables used by other
    // threads.
    let mut env_map = HashMap::new();
    env_map.insert("foo1".to_owned(), Some("bar".to_owned()));

    let task = Task {
      dependencies: vec![],
      cache: true,
      environment: env_map,
      input_paths: vec![],
      output_paths: vec![],
      ports: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
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
    // NOTE: We add an index to the test arg ("foo1", "foo2", ...) to avoid
    // having parallel tests clobbering environment variables used by other
    // threads.
    let mut env_map = HashMap::new();
    env_map.insert("foo2".to_owned(), Some("bar".to_owned()));

    let task = Task {
      dependencies: vec![],
      cache: true,
      environment: env_map,
      input_paths: vec![],
      output_paths: vec![],
      ports: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
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
    // NOTE: We add an index to the test arg ("foo1", "foo2", ...) to avoid
    // having parallel tests clobbering environment variables used by other
    // threads.
    let mut env_map = HashMap::new();
    env_map.insert("foo3".to_owned(), None);

    let task = Task {
      dependencies: vec![],
      cache: true,
      environment: env_map,
      input_paths: vec![],
      output_paths: vec![],
      ports: vec![],
      location: Path::new(DEFAULT_LOCATION).to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: None,
    };

    env::remove_var("foo3");
    assert!(env::var("foo3").is_err());
    let result = environment(&task);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err()[0].to_owned(), "foo3");
  }

  #[test]
  fn check_dependencies_empty() {
    let bakefile = Bakefile {
      image: "encom:os-12".to_owned(),
      default: None,
      tasks: HashMap::new(),
    };

    assert!(check_dependencies(&bakefile).is_ok());
  }

  #[test]
  fn check_dependencies_single() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "encom:os-12".to_owned(),
      default: None,
      tasks,
    };

    assert!(check_dependencies(&bakefile).is_ok());
  }

  #[test]
  fn check_dependencies_nonempty() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "bar".to_owned(),
      Task {
        dependencies: vec!["foo".to_owned()],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "encom:os-12".to_owned(),
      default: None,
      tasks,
    };

    assert!(check_dependencies(&bakefile).is_ok());
  }

  #[test]
  fn check_dependencies_nonexistent() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "bar".to_owned(),
      Task {
        dependencies: vec!["foo".to_owned(), "baz".to_owned()],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "encom:os-12".to_owned(),
      default: None,
      tasks,
    };

    let result = check_dependencies(&bakefile);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("baz"));
  }

  #[test]
  fn check_dependencies_cycle_1() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec!["foo".to_owned()],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "encom:os-12".to_owned(),
      default: None,
      tasks,
    };

    let result = check_dependencies(&bakefile);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cyclic"));
  }

  #[test]
  fn check_dependencies_cycle_2() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec!["bar".to_owned()],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "bar".to_owned(),
      Task {
        dependencies: vec!["foo".to_owned()],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "encom:os-12".to_owned(),
      default: None,
      tasks,
    };

    let result = check_dependencies(&bakefile);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cyclic"));
  }

  #[test]
  fn check_dependencies_cycle_3() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec!["baz".to_owned()],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "bar".to_owned(),
      Task {
        dependencies: vec!["foo".to_owned()],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "baz".to_owned(),
      Task {
        dependencies: vec!["bar".to_owned()],
        cache: true,
        environment: HashMap::new(),
        input_paths: vec![],
        output_paths: vec![],
        ports: vec![],
        location: Path::new(DEFAULT_LOCATION).to_owned(),
        user: DEFAULT_USER.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "encom:os-12".to_owned(),
      default: None,
      tasks,
    };

    let result = check_dependencies(&bakefile);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cyclic"));
  }
}
