use crate::bakefile::Bakefile;
use std::collections::HashSet;

// Compute a topological sort of the transitive reflexive closure of a set of
// tasks. The resulting schedule does not depend on the order of the inputs.
pub fn compute<'a>(bakefile: &'a Bakefile, tasks: &[&'a str]) -> Vec<&'a str> {
  // Sort the input tasks to ensure that the given order doesn't matter.
  let mut roots: Vec<&'a str> = tasks.to_vec();
  roots.sort();

  // We will use this set to keep track of what tasks have already been
  // seen.
  let mut visited: HashSet<&'a str> = HashSet::new();

  // This vector accumulates the final schedule.
  let mut schedule: Vec<&'a str> = vec![];

  // For each root, compute its transitive reflexive closure, topsort it, and
  // add it to the schedule.
  for root in roots {
    // If we have already scheduled this root task, skip to the next one.
    if visited.contains(root) {
      break;
    }

    // Mark this task as seen so we don't process it again.
    visited.insert(root);

    // The frontier is a stack, which means we are doing a depth-first
    // traversal.
    let mut frontier: Vec<&'a str> = vec![root];

    // This vector will accumulate the topsorted tasks.
    let mut topological_sort: Vec<&'a str> = vec![];

    // Keep processing nodes on the frontier until there aren't any more left.
    // [tag:frontier_nonempty]
    while !frontier.is_empty() {
      // Pop a task from the frontier and schedule it.
      let task = frontier.pop().unwrap(); // [ref:frontier_nonempty]
      topological_sort.push(task);

      // Add the task's dependencies to the frontier.
      // The `unwrap` is safe due to [ref:tasks_valid].
      for dependency in &bakefile.tasks[task].dependencies {
        let dep: &'a str = &dependency;
        if !visited.contains(dep) {
          visited.insert(dep);
          frontier.push(dep);
        }
      }
    }

    // The DFS algorithm pushes tasks before their dependencies. Here we
    // reverse the order so dependencies are scheduled first.
    topological_sort.reverse();

    // Add the topsorted tasks to the schedule.
    schedule.extend(topological_sort);
  }

  // Return the final schedule.
  schedule
}

#[cfg(test)]
mod tests {
  use crate::bakefile::{Bakefile, Task, DEFAULT_LOCATION};
  use crate::schedule::compute;
  use std::collections::HashMap;

  #[test]
  fn schedule_empty() {
    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks: HashMap::new(),
    };

    let actual: Vec<&str> = compute(&bakefile, &[]);
    let expected: Vec<&str> = vec![];

    assert_eq!(actual, expected);
  }

  #[test]
  fn schedule_single() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks,
    };

    let actual: Vec<&str> = compute(&bakefile, &["foo"]);
    let expected: Vec<&str> = vec!["foo"];

    assert_eq!(actual, expected);
  }

  #[test]
  fn schedule_linear() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "bar".to_owned(),
      Task {
        dependencies: vec!["foo".to_owned()],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "baz".to_owned(),
      Task {
        dependencies: vec!["bar".to_owned()],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks,
    };

    let actual: Vec<&str> = compute(&bakefile, &["baz"]);
    let expected: Vec<&str> = vec!["foo", "bar", "baz"];

    assert_eq!(actual, expected);
  }

  #[test]
  fn schedule_duplicates() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "bar".to_owned(),
      Task {
        dependencies: vec!["foo".to_owned()],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "baz".to_owned(),
      Task {
        dependencies: vec!["bar".to_owned()],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks,
    };

    let actual: Vec<&str> = compute(&bakefile, &["baz", "baz"]);
    let expected: Vec<&str> = vec!["foo", "bar", "baz"];

    assert_eq!(actual, expected);
  }

  #[test]
  fn schedule_tie_breaking() {
    let mut tasks = HashMap::new();
    tasks.insert(
      "foo".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "bar".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );
    tasks.insert(
      "baz".to_owned(),
      Task {
        dependencies: vec![],
        cache: true,
        args: HashMap::new(),
        files: vec![],
        location: DEFAULT_LOCATION.to_owned(),
        command: None,
      },
    );

    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      tasks,
    };

    let actual: Vec<&str> = compute(&bakefile, &["foo", "bar", "baz"]);
    let expected: Vec<&str> = vec!["bar", "baz", "foo"];

    assert_eq!(actual, expected);
  }
}
