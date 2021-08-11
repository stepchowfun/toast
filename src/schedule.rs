use crate::toastfile::Toastfile;
use std::{collections::HashSet, convert::AsRef};

// Compute a topological sort of the transitive reflexive closure of a set of tasks. The resulting
// schedule does not depend on the order of the inputs or dependencies. We assume the tasks form a
// DAG [ref:tasks_dag].
pub fn compute<'a>(toastfile: &'a Toastfile, tasks: &[&'a str]) -> Vec<&'a str> {
    // Sort the input tasks to ensure the given order doesn't matter.
    let mut roots: Vec<&'a str> = tasks.to_vec();
    roots.sort_unstable();

    // We will use this set to keep track of what tasks have already been seen.
    let mut visited: HashSet<&'a str> = HashSet::new();

    // This vector accumulates the final schedule.
    let mut schedule: Vec<&'a str> = vec![];

    // For each root, compute its transitive reflexive closure, topsort it, and add it to the
    // schedule.
    for root in roots {
        // We'll this frontier as a stack for a depth-first traversal.
        let mut frontier: Vec<(&'a str, bool)> = vec![(root, true)];

        // This vector will accumulate the topsorted tasks.
        let mut topological_sort: Vec<&'a str> = vec![];

        // Keep processing nodes on the frontier until there aren't any more left.
        // [tag:schedule_frontier_nonempty]
        while !frontier.is_empty() {
            // Pop a task from the frontier. [ref:schedule_frontier_nonempty]
            let (task, new) = frontier.pop().unwrap();

            // Check if this is a new task or one that we are coming back to because we finished
            // processing its dependencies.
            if new {
                // If we have already scheduled this root task, skip to the next one.
                if visited.contains(task) {
                    continue;
                }

                // Mark this task as seen so we don't process it again.
                visited.insert(task);

                // Come back to this task once all its dependencies have been processed.
                frontier.push((task, false));

                // Add the task's dependencies to the frontier. We sort the dependencies first to
                // ensure their original order doesn't matter. After sorting, we reverse the order
                // of the dependencies before adding them to the frontier so that they will be
                // processed in lexicographical order (since the frontier is a stack rather than a
                // queue). The indexing is safe due to [ref:tasks_valid].
                let mut dependencies: Vec<&'a str> = toastfile.tasks[task]
                    .dependencies
                    .iter()
                    .map(AsRef::as_ref)
                    .collect();
                dependencies.sort_unstable();
                dependencies.reverse();
                frontier.extend(
                    dependencies
                        .into_iter()
                        .map(|dependency| (dependency, true)),
                );
            } else {
                // Now that the task's dependencies have been processed, schedule it.
                topological_sort.push(task);
            }
        }

        // Add the topsorted tasks to the schedule.
        schedule.extend(topological_sort);
    }

    // Return the final schedule.
    schedule
}

#[cfg(test)]
mod tests {
    use crate::schedule::compute;
    use crate::toastfile::{Task, Toastfile, DEFAULT_LOCATION, DEFAULT_USER};
    use std::{collections::HashMap, path::Path};

    fn task_with_dependencies(dependencies: Vec<String>) -> Task {
        Task {
            description: None,
            dependencies,
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: String::new(),
            extra_docker_args: vec![],
        }
    }

    fn empty_task() -> Task {
        task_with_dependencies(vec![])
    }

    #[test]
    fn schedule_empty() {
        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks: HashMap::new(),
        };

        let actual: Vec<&str> = compute(&toastfile, &[]);
        let expected: Vec<&str> = vec![];

        assert_eq!(actual, expected);
    }

    #[test]
    fn schedule_single() {
        let mut tasks = HashMap::new();
        tasks.insert("foo".to_owned(), empty_task());

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks,
        };

        let actual: Vec<&str> = compute(&toastfile, &["foo"]);
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

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks,
        };

        let actual: Vec<&str> = compute(&toastfile, &["baz"]);
        let expected: Vec<&str> = vec!["foo", "bar", "baz"];

        assert_eq!(actual, expected);
    }

    #[test]
    fn schedule_diamond() {
        let mut tasks = HashMap::new();
        tasks.insert("foo".to_owned(), empty_task());
        tasks.insert(
            "bar".to_owned(),
            task_with_dependencies(vec!["foo".to_owned()]),
        );
        tasks.insert(
            "baz".to_owned(),
            task_with_dependencies(vec!["foo".to_owned()]),
        );
        tasks.insert(
            "qux".to_owned(),
            task_with_dependencies(vec!["bar".to_owned(), "baz".to_owned()]),
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks,
        };

        let actual: Vec<&str> = compute(&toastfile, &["qux"]);
        let expected: Vec<&str> = vec!["foo", "bar", "baz", "qux"];

        assert_eq!(actual, expected);
    }

    #[test]
    fn schedule_lexicographical_tie_breaking() {
        let mut tasks = HashMap::new();
        tasks.insert("foo".to_owned(), empty_task());
        tasks.insert("bar".to_owned(), empty_task());
        tasks.insert("baz".to_owned(), empty_task());

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks,
        };

        let actual: Vec<&str> = compute(&toastfile, &["foo", "bar", "baz"]);
        let expected: Vec<&str> = vec!["bar", "baz", "foo"];

        assert_eq!(actual, expected);
    }

    #[test]
    fn schedule_dependency_duplicates() {
        let mut tasks1 = HashMap::new();
        tasks1.insert("foo".to_owned(), empty_task());
        tasks1.insert("bar".to_owned(), empty_task());
        tasks1.insert(
            "baz".to_owned(),
            task_with_dependencies(vec!["foo".to_owned(), "bar".to_owned(), "foo".to_owned()]),
        );

        let mut tasks2 = HashMap::new();
        tasks2.insert("foo".to_owned(), empty_task());
        tasks2.insert("bar".to_owned(), empty_task());
        tasks2.insert(
            "baz".to_owned(),
            task_with_dependencies(vec!["bar".to_owned(), "foo".to_owned(), "bar".to_owned()]),
        );

        let toastfile1 = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks: tasks1,
        };

        let toastfile2 = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks: tasks2,
        };

        let first: Vec<&str> = compute(&toastfile1, &["baz"]);
        let second: Vec<&str> = compute(&toastfile2, &["baz"]);

        assert_eq!(first, second);
    }

    #[test]
    fn schedule_input_duplicates() {
        let mut tasks = HashMap::new();
        tasks.insert("foo".to_owned(), empty_task());
        tasks.insert("bar".to_owned(), empty_task());
        tasks.insert("baz".to_owned(), empty_task());

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks,
        };

        let first: Vec<&str> = compute(&toastfile, &["baz", "bar", "baz"]);
        let second: Vec<&str> = compute(&toastfile, &["bar", "baz", "bar"]);

        assert_eq!(first, second);
    }

    #[test]
    fn schedule_dependency_order() {
        let mut tasks1 = HashMap::new();
        tasks1.insert("foo".to_owned(), empty_task());
        tasks1.insert("bar".to_owned(), empty_task());
        tasks1.insert("baz".to_owned(), empty_task());
        tasks1.insert(
            "qux".to_owned(),
            task_with_dependencies(vec!["foo".to_owned(), "bar".to_owned(), "baz".to_owned()]),
        );

        let mut tasks2 = HashMap::new();
        tasks2.insert("foo".to_owned(), empty_task());
        tasks2.insert("bar".to_owned(), empty_task());
        tasks2.insert("baz".to_owned(), empty_task());
        tasks2.insert(
            "qux".to_owned(),
            task_with_dependencies(vec!["baz".to_owned(), "bar".to_owned(), "foo".to_owned()]),
        );

        let toastfile1 = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks: tasks1,
        };

        let toastfile2 = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks: tasks2,
        };

        let first: Vec<&str> = compute(&toastfile1, &["baz"]);
        let second: Vec<&str> = compute(&toastfile2, &["baz"]);

        assert_eq!(first, second);
    }

    #[test]
    fn schedule_input_order() {
        let mut tasks = HashMap::new();
        tasks.insert("foo".to_owned(), empty_task());
        tasks.insert("bar".to_owned(), empty_task());
        tasks.insert("baz".to_owned(), empty_task());

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            tasks,
        };

        let first: Vec<&str> = compute(&toastfile, &["foo", "bar", "baz"]);
        let second: Vec<&str> = compute(&toastfile, &["baz", "bar", "foo"]);

        assert_eq!(first, second);
    }
}
