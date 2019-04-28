use crate::bakefile::Bakefile;
use std::collections::HashSet;

// Compute a topological sort of the transitive reflexive closure of a set of
// tasks. The resulting schedule does not depend on the order of the inputs.
// We assume that the tasks form a DAG (no cycles).
pub fn compute<'a>(bakefile: &'a Bakefile, tasks: &[&'a str]) -> Vec<&'a str> {
  // Sort the input tasks to ensure the given order doesn't matter.
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
    // The frontier is a stack, which means we are doing a depth-first
    // traversal.
    let mut frontier: Vec<&'a str> = vec![root];

    // This vector will accumulate the topsorted tasks.
    let mut topological_sort: Vec<&'a str> = vec![];

    // Keep processing nodes on the frontier until there aren't any more left.
    // [tag:schedule_frontier_nonempty]
    while !frontier.is_empty() {
      // Pop a task from the frontier.
      let task = frontier.pop().unwrap(); // [ref:schedule_frontier_nonempty]

      // If we have already scheduled this root task, skip to the next one.
      if visited.contains(task) {
        continue;
      }

      // Mark this task as seen so we don't process it again.
      visited.insert(task);

      // Schedule the task.
      topological_sort.push(task);

      // Add the task's dependencies to the frontier. We sort the dependencies
      // first to ensure their original order doesn't matter.
      // The indexing is safe due to [ref:tasks_valid].
      let mut dependencies: Vec<&'a str> = vec![];
      for dependency in &bakefile.tasks[task].dependencies {
        dependencies.push(dependency);
      }
      dependencies.sort();
      for dependency in dependencies {
        frontier.push(dependency);
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
  use crate::bakefile::{Bakefile, Task, DEFAULT_LOCATION, DEFAULT_USER};
  use crate::schedule::compute;
  use std::collections::HashMap;

  fn task_with_dependencies(dependencies: Vec<String>) -> Task {
    Task {
      dependencies,
      cache: true,
      env: HashMap::new(),
      paths: vec![],
      location: DEFAULT_LOCATION.to_owned(),
      user: DEFAULT_USER.to_owned(),
      command: None,
    }
  }

  fn empty_task() -> Task {
    task_with_dependencies(vec![])
  }

  #[test]
  fn schedule_empty() {
    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      default: None,
      tasks: HashMap::new(),
    };

    let actual: Vec<&str> = compute(&bakefile, &[]);
    let expected: Vec<&str> = vec![];

    assert_eq!(actual, expected);
  }

  #[test]
  fn schedule_single() {
    let mut tasks = HashMap::new();
    tasks.insert("foo".to_owned(), empty_task());

    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      default: None,
      tasks,
    };

    let actual: Vec<&str> = compute(&bakefile, &["foo"]);
    let expected: Vec<&str> = vec!["foo"];

    assert_eq!(actual, expected);
  }

  #[test]
  fn schedule_linear() {
    let mut tasks = HashMap::new();
    tasks.insert("foo".to_owned(), empty_task());
    tasks.insert(
      "bar".to_owned(),
      task_with_dependencies(vec!["foo".to_owned()]),
    );
    tasks.insert(
      "baz".to_owned(),
      task_with_dependencies(vec!["bar".to_owned()]),
    );

    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      default: None,
      tasks,
    };

    let actual: Vec<&str> = compute(&bakefile, &["baz"]);
    let expected: Vec<&str> = vec!["foo", "bar", "baz"];

    assert_eq!(actual, expected);
  }

  #[test]
  fn schedule_duplicates() {
    let mut tasks = HashMap::new();
    tasks.insert("foo".to_owned(), empty_task());
    tasks.insert(
      "bar".to_owned(),
      task_with_dependencies(vec!["foo".to_owned()]),
    );
    tasks.insert(
      "baz".to_owned(),
      task_with_dependencies(vec!["bar".to_owned()]),
    );

    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      default: None,
      tasks,
    };

    let actual: Vec<&str> = compute(&bakefile, &["baz", "baz"]);
    let expected: Vec<&str> = vec!["foo", "bar", "baz"];

    assert_eq!(actual, expected);
  }

  #[test]
  fn schedule_tie_breaking() {
    let mut tasks = HashMap::new();
    tasks.insert("foo".to_owned(), empty_task());
    tasks.insert("bar".to_owned(), empty_task());
    tasks.insert("baz".to_owned(), empty_task());

    let bakefile = Bakefile {
      image: "ubuntu:18.04".to_owned(),
      default: None,
      tasks,
    };

    let actual: Vec<&str> = compute(&bakefile, &["foo", "bar", "baz"]);
    let expected: Vec<&str> = vec!["bar", "baz", "foo"];

    assert_eq!(actual, expected);
  }
}
