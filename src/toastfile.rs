use {
    crate::{failure::Failure, format, format::CodeStr},
    serde::{Deserialize, Deserializer, Serialize, Serializer},
    std::{
        collections::{HashMap, HashSet},
        env,
        fmt::{self, Display, Formatter},
        path::{Path, PathBuf},
    },
};

// The default location for commands and files copied into the container
pub const DEFAULT_LOCATION: &str = "/scratch";

// The default user for commands and files copied into the container
pub const DEFAULT_USER: &str = "root";

// This struct represents a path on the host and a corresponding path in the container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappingPath {
    pub host_path: PathBuf,
    pub container_path: PathBuf,
}

impl Display for MappingPath {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}:{}",
            self.host_path.to_string_lossy(),
            self.container_path.to_string_lossy(),
        )
    }
}

impl Serialize for MappingPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(format!("{}", self).as_str())
    }
}

struct MappingPathVisitor;

impl<'de> serde::de::Visitor<'de> for MappingPathVisitor {
    type Value = MappingPath;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("a path")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if let Some((host_path, container_path)) = v.split_once(":") {
            Ok(MappingPath {
                host_path: host_path
                    .parse()
                    .map_err(|_| E::custom("Illegal host path."))?,
                container_path: container_path
                    .parse()
                    .map_err(|_| E::custom("Illegal container path."))?,
            })
        } else {
            let path: PathBuf = v.parse().map_err(|_| E::custom("Illegal path."))?;
            Ok(MappingPath {
                host_path: path.clone(),
                container_path: path,
            })
        }
    }
}

impl<'de> Deserialize<'de> for MappingPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(MappingPathVisitor)
    }
}

// This struct represents a task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub description: Option<String>,

    // Must point to valid task names [ref:dependencies_exist] and the dependency DAG must not form
    // cycles [ref:tasks_dag]
    #[serde(default)]
    pub dependencies: Vec<String>,

    // Must be disabled if any of the following conditions hold:
    // - `mount_paths` is nonempty [ref:mount_paths_nand_cache]
    // - `ports` is nonempty [ref:ports_nand_cache]
    // - `extra_docker_arguments` is nonempty [ref:extra_docker_arguments_nand_cache]
    #[serde(default = "default_task_cache")]
    pub cache: bool,

    // Keys must not contain `=` [ref:env_var_equals]
    #[serde(default)] // [tag:default_environment]
    pub environment: HashMap<String, Option<String>>,

    // Must be relative [ref:input_paths_relative]
    #[serde(default)]
    pub input_paths: Vec<PathBuf>,

    // Must be relative [ref:excluded_input_paths_relative]
    #[serde(default)]
    pub excluded_input_paths: Vec<PathBuf>,

    // Must be relative [ref:output_paths_relative]
    #[serde(default)]
    pub output_paths: Vec<PathBuf>,

    // Must be relative [ref:output_paths_on_failure_relative]
    #[serde(default)]
    pub output_paths_on_failure: Vec<PathBuf>,

    // Can be relative or absolute (absolute paths are allowed in order to support mounting the
    //   Docker socket, which is usually located at `/var/run/docker.sock`)
    // Must not contain `,` [ref:mount_paths_no_commas]
    // Must be empty if `cache` is enabled [ref:mount_paths_nand_cache]
    // Can be `host_path:container_path` or a single path if `host_path` is the same as
    //   `container_path`
    #[serde(default)] // [tag:default_mount_paths]
    pub mount_paths: Vec<MappingPath>,

    #[serde(default = "default_task_mount_readonly")]
    pub mount_readonly: bool,

    // Must be empty if `cache` is enabled [ref:ports_nand_cache]
    #[serde(default)] // [tag:default_ports]
    pub ports: Vec<String>,

    // If `None`, the corresponding top-level value in the toastfile should be used.
    // Must be absolute [ref:task_location_absolute]
    #[serde(default)]
    pub location: Option<PathBuf>,

    // If `None`, the corresponding top-level value in the toastfile should be used.
    pub user: Option<String>,

    #[serde(default)]
    pub command: String,

    // If `None`, the corresponding top-level value in the toastfile should be used.
    #[serde(default)]
    pub command_prefix: Option<String>,

    // Must be empty if `cache` is enabled [ref:extra_docker_arguments_nand_cache]
    #[serde(default)]
    pub extra_docker_arguments: Vec<String>,
}

fn default_task_cache() -> bool {
    true
}

pub fn default_task_mount_readonly() -> bool {
    false
}

// This struct represents a toastfile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Toastfile {
    pub image: String,

    // If present, must point to a task [ref:valid_default]
    pub default: Option<String>,

    // Must be absolute [ref:toastfile_location_absolute]
    #[serde(default = "default_location")]
    pub location: PathBuf,

    #[serde(default = "default_user")]
    pub user: String,

    #[serde(default)]
    pub command_prefix: String,

    #[serde(default)]
    pub tasks: HashMap<String, Task>,
}

fn default_location() -> PathBuf {
    Path::new(DEFAULT_LOCATION).to_owned()
}

fn default_user() -> String {
    DEFAULT_USER.to_owned()
}

// Parse config data.
pub fn parse(toastfile_data: &str) -> Result<Toastfile, Failure> {
    // Deserialize the data.
    let toastfile: Toastfile =
        serde_yaml::from_str(toastfile_data).map_err(|e| Failure::User(format!("{}", e), None))?;

    // Make sure the dependencies are valid.
    check_dependencies(&toastfile)?;

    // Check that `location` is absolute [tag:toastfile_location_absolute].
    if !is_absolute_linux_path(&toastfile.location) {
        return Err(Failure::User(
            format!(
                "Toastfile has a relative {}: {}.",
                "location".code_str(),
                toastfile.location.to_string_lossy().code_str(),
            ),
            None,
        ));
    }

    // Make sure each task is valid.
    for (name, task) in &toastfile.tasks {
        check_task(name, task)?;
    }

    // Return the toastfile.
    Ok(toastfile)
}

// Fetch the variables for a task from the environment.
pub fn environment(task: &Task) -> Result<HashMap<String, String>, Vec<&str>> {
    // The result will be a map from variable name to value.
    let mut result = HashMap::new();

    // We accumulate a list of errors to be shown to the user when there is a problem.
    let mut violations = vec![];

    // Fetch each environment variable.
    for (arg, default) in &task.environment {
        // Read the variable from the environment.
        let maybe_var = env::var(arg);

        // If a default value was provided, use that if the variable is missing from the
        // environment. If there was no default, the variable must be in the environment or else
        // we'll report a violation.
        if let Some(default) = default {
            result.insert(arg.clone(), maybe_var.unwrap_or_else(|_| default.clone()));
        } else if let Ok(var) = maybe_var {
            result.insert(arg.clone(), var);
        } else {
            violations.push(arg.as_ref());
        }
    }

    // If there were no violations, return the map. Otherwise, report the violations.
    if violations.is_empty() {
        Ok(result)
    } else {
        Err(violations)
    }
}

// Fetch the location for a task, defaulting to the top-level location if needed.
pub fn location(toastfile: &Toastfile, task: &Task) -> PathBuf {
    task.location
        .clone()
        .unwrap_or_else(|| toastfile.location.clone())
}

// Fetch the user for a task, defaulting to the top-level location if needed.
pub fn user(toastfile: &Toastfile, task: &Task) -> String {
    task.user.clone().unwrap_or_else(|| toastfile.user.clone())
}

// Fetch the command for a task, including the prefix, using the top-level prefix if needed.
pub fn command(toastfile: &Toastfile, task: &Task) -> String {
    let mut command = String::new();

    if let Some(command_prefix) = &task.command_prefix {
        command.push_str(command_prefix);
    } else {
        command.push_str(&toastfile.command_prefix);
    }

    command.push('\n');

    command.push_str(&task.command);

    command
}

// Check that all dependencies exist and form a DAG (no cycles).
#[allow(clippy::too_many_lines)]
fn check_dependencies<'a>(toastfile: &'a Toastfile) -> Result<(), Failure> {
    // Check the default task [tag:valid_default].
    let valid_default = toastfile
        .default
        .as_ref()
        .map_or(true, |default| toastfile.tasks.contains_key(default));

    // Map from task to vector of invalid dependencies.
    let mut violations: HashMap<String, Vec<String>> = HashMap::new();

    // Scan for invalid dependencies [tag:task_valid].
    for task in toastfile.tasks.keys() {
        // [ref:task_valid]
        for dependency in &toastfile.tasks[task].dependencies {
            if !toastfile.tasks.contains_key(dependency) {
                // [tag:dependencies_exist]
                violations
                    .entry(task.clone())
                    .or_insert_with(Vec::new)
                    .push(dependency.clone());
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
                                .as_ref(),
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .as_ref(),
        );

        if valid_default {
            return Err(Failure::User(
                format!(
                    "The following tasks have invalid dependencies: {}.",
                    violations_series,
                ),
                None,
            ));
        }

        return Err(Failure::User(
            format!(
                "The default task {} does not exist, and the following tasks have invalid \
                 dependencies: {}.",
                toastfile.default.as_ref().unwrap().code_str(), // [ref:valid_default]
                violations_series,
            ),
            None,
        ));
    } else if !valid_default {
        return Err(Failure::User(
            format!(
                "The default task {} does not exist.",
                toastfile.default.as_ref().unwrap().code_str(), // [ref:valid_default]
            ),
            None,
        ));
    }

    // Check that the dependencies aren't cyclic [tag:tasks_dag].
    let mut visited: HashSet<&'a str> = HashSet::new();
    for task in toastfile.tasks.keys() {
        let mut frontier: Vec<(&'a str, usize)> = vec![(task, 0)];
        let mut ancestors_set: HashSet<&'a str> = HashSet::new();
        let mut ancestors_stack: Vec<&'a str> = vec![];

        // Keep going as long as there are more nodes to process [tag:toastfile_frontier_nonempty].
        while !frontier.is_empty() {
            // Take the top task from the frontier. This is safe due to
            // [ref:toastfile_frontier_nonempty].
            let (task, task_depth) = frontier.pop().unwrap();

            // Update the ancestors set and stack.
            for _ in 0..ancestors_stack.len() - task_depth {
                // The `unwrap` is safe because `ancestors_stack.len()` is positive in every
                // iteration of this loop.
                let task_to_remove = ancestors_stack.pop().unwrap();
                ancestors_set.remove(task_to_remove);
            }

            // If this task is an ancestor of itself, we have a cycle. Return an error.
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
                        cycle[1].code_str(),
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
                                .map(|(x, y)| {
                                    format!("{} depends on {}", x.code_str(), y.code_str())
                                })
                                .collect::<Vec<_>>()
                                .as_ref(),
                        ),
                    )
                };
                return Err(Failure::User(
                    format!("The dependencies are cyclic. {}", error_message),
                    None,
                ));
            }

            // If we've never seen this task before, add its dependencies to the frontier.
            if !visited.contains(task) {
                visited.insert(task);

                ancestors_set.insert(task);
                ancestors_stack.push(task);

                for dependency in &toastfile.tasks[task].dependencies {
                    frontier.push((dependency, task_depth + 1));
                }
            }
        }
    }

    // No violations
    Ok(())
}

// Check if a path is an absolute Linux path.
#[cfg(unix)]
fn is_absolute_linux_path(path: &Path) -> bool {
    path.is_absolute()
}

// Check if a path is an absolute Linux path.
#[cfg(windows)]
fn is_absolute_linux_path(path: &Path) -> bool {
    path.has_root() && !path.is_absolute()
}

// Check if a path is a relative Linux path.
fn is_relative_linux_path(path: &Path) -> bool {
    path.is_relative() && !path.has_root()
}

// Check that a task is valid.
#[allow(clippy::too_many_lines)]
fn check_task(name: &str, task: &Task) -> Result<(), Failure> {
    // Check that environment variable names don't have `=` in them [tag:env_var_equals].
    for variable in task.environment.keys() {
        if variable.contains('=') {
            return Err(Failure::User(
                format!(
                    "Environment variable {} of task {} contains {}.",
                    variable.code_str(),
                    name.code_str(),
                    "=".code_str(),
                ),
                None,
            ));
        }
    }

    // Check that `input_paths` are relative [tag:input_paths_relative].
    for path in &task.input_paths {
        if !is_relative_linux_path(path) {
            return Err(Failure::User(
                format!(
                    "Task {} has an absolute {}: {}.",
                    name.code_str(),
                    "input_path".code_str(),
                    path.to_string_lossy().code_str(),
                ),
                None,
            ));
        }
    }

    // Check that `excluded_input_paths` are relative [tag:excluded_input_paths_relative].
    for path in &task.excluded_input_paths {
        if !is_relative_linux_path(path) {
            return Err(Failure::User(
                format!(
                    "Task {} has an absolute {}: {}.",
                    name.code_str(),
                    "excluded_input_path".code_str(),
                    path.to_string_lossy().code_str(),
                ),
                None,
            ));
        }
    }

    // Check that `output_paths` are relative [tag:output_paths_relative].
    for path in &task.output_paths {
        if !is_relative_linux_path(path) {
            return Err(Failure::User(
                format!(
                    "Task {} has an absolute path in {}: {}.",
                    name.code_str(),
                    "output_paths".code_str(),
                    path.to_string_lossy().code_str(),
                ),
                None,
            ));
        }
    }

    // Check that `output_paths_on_failure` are relative [tag:output_paths_on_failure_relative].
    for path in &task.output_paths_on_failure {
        if !is_relative_linux_path(path) {
            return Err(Failure::User(
                format!(
                    "Task {} has an absolute path in {}: {}.",
                    name.code_str(),
                    "output_paths_on_failure".code_str(),
                    path.to_string_lossy().code_str(),
                ),
                None,
            ));
        }
    }

    // Check `mount_paths`.
    for path in &task.mount_paths {
        // Check that the path doesn't contain any commas [tag:mount_paths_no_commas].
        if path.container_path.to_string_lossy().contains(',')
            || path.host_path.to_string_lossy().contains(',')
        {
            return Err(Failure::User(
                format!(
                    "Mount path {} of task {} has a {}.",
                    format!("{}", path).code_str(),
                    name.code_str(),
                    ",".code_str(),
                ),
                None,
            ));
        }
    }

    // Check that `location` is absolute [tag:task_location_absolute].
    if let Some(location) = &task.location {
        if !is_absolute_linux_path(location) {
            return Err(Failure::User(
                format!(
                    "Task {} has a relative {}: {}.",
                    name.code_str(),
                    "location".code_str(),
                    location.to_string_lossy().code_str(),
                ),
                None,
            ));
        }
    }

    // If a task has any mount paths, then caching should be disabled [tag:mount_paths_nand_cache].
    if !task.mount_paths.is_empty() && task.cache {
        return Err(Failure::User(
            format!(
                "Task {} has {} but does not disable caching. \
                 To fix this, set {} for this task.",
                name.code_str(),
                "mount_paths".code_str(),
                "cache: false".code_str(),
            ),
            None,
        ));
    }

    // If a task exposes ports, then caching should be disabled [tag:ports_nand_cache].
    if !&task.ports.is_empty() && task.cache {
        return Err(Failure::User(
            format!(
                "Task {} exposes ports but does not disable caching. \
                 To fix this, set {} for this task.",
                name.code_str(),
                "cache: false".code_str(),
            ),
            None,
        ));
    }

    // If a task has any extra Docker arguments, then caching should be disabled.
    // [tag:extra_docker_arguments_nand_cache]
    if !&task.extra_docker_arguments.is_empty() && task.cache {
        return Err(Failure::User(
            format!(
                "Task {} has extra Docker arguments but does not disable caching. \
                 To fix this, set {} for this task.",
                name.code_str(),
                "cache: false".code_str(),
            ),
            None,
        ));
    }

    // If we made it this far, the task is valid.
    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        crate::toastfile::{
            check_dependencies, check_task, command, environment, location, parse, user,
            MappingPath, Task, Toastfile, DEFAULT_LOCATION, DEFAULT_USER,
        },
        std::{collections::HashMap, env, path::Path},
    };

    #[test]
    fn parse_empty() {
        let input = r#"
image: encom:os-12
    "#
        .trim();

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks: HashMap::new(),
        };

        assert_eq!(parse(input).unwrap(), toastfile);
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
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert_eq!(parse(input).unwrap(), toastfile);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn parse_comprehensive_task() {
        let input = r#"
image: encom:os-12
default: bar
location: /default_location
user: default_user
command_prefix: prefix
tasks:
  foo: {}
  bar:
    description: Reticulate splines.
    dependencies:
      - foo
    cache: false
    environment:
      SPAM: monty
      HAM: null
      EGGS: null
    input_paths:
      - qux
      - quux
      - quuz
    excluded_input_paths:
      - spam
      - ham
      - eggs
    output_paths:
      - corge
      - grault
      - garply
    output_paths_on_failure:
      - fnord
      - smurf
      - xyzzy
    mount_paths:
      - wibble
      - /wobble
      - wubble:wabble
    mount_readonly: true
    ports:
      - 3000
      - 3001
      - 3002
    location: /code
    user: waldo
    command: flob
    command_prefix: flob_prefix
    extra_docker_arguments:
      - --cpus
      - '4'
    "#
        .trim();

        let mut environment = HashMap::new();
        environment.insert("SPAM".to_owned(), Some("monty".to_owned()));
        environment.insert("HAM".to_owned(), None);
        environment.insert("EGGS".to_owned(), None);

        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );
        tasks.insert(
            "bar".to_owned(),
            Task {
                description: Some("Reticulate splines.".to_owned()),
                dependencies: vec!["foo".to_owned()],
                cache: false,
                environment,
                input_paths: vec![
                    Path::new("qux").to_owned(),
                    Path::new("quux").to_owned(),
                    Path::new("quuz").to_owned(),
                ],
                excluded_input_paths: vec![
                    Path::new("spam").to_owned(),
                    Path::new("ham").to_owned(),
                    Path::new("eggs").to_owned(),
                ],
                output_paths: vec![
                    Path::new("corge").to_owned(),
                    Path::new("grault").to_owned(),
                    Path::new("garply").to_owned(),
                ],
                output_paths_on_failure: vec![
                    Path::new("fnord").to_owned(),
                    Path::new("smurf").to_owned(),
                    Path::new("xyzzy").to_owned(),
                ],
                mount_paths: vec![
                    MappingPath {
                        host_path: Path::new("wibble").to_owned(),
                        container_path: Path::new("wibble").to_owned(),
                    },
                    MappingPath {
                        host_path: Path::new("/wobble").to_owned(),
                        container_path: Path::new("/wobble").to_owned(),
                    },
                    MappingPath {
                        host_path: Path::new("wubble").to_owned(),
                        container_path: Path::new("wabble").to_owned(),
                    },
                ],
                mount_readonly: true,
                ports: vec!["3000".to_owned(), "3001".to_owned(), "3002".to_owned()],
                location: Some(Path::new("/code").to_owned()),
                user: Some("waldo".to_owned()),
                command: "flob".to_owned(),
                command_prefix: Some("flob_prefix".to_owned()),
                extra_docker_arguments: vec!["--cpus".to_owned(), "4".to_owned()],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: Some("bar".to_owned()),
            location: Path::new("/default_location").to_owned(),
            user: "default_user".to_owned(),
            command_prefix: "prefix".to_owned(),
            tasks,
        };

        assert_eq!(parse(input).unwrap(), toastfile);
    }

    #[test]
    fn check_dependencies_valid_default() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: Some("foo".to_owned()),
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert!(check_dependencies(&toastfile).is_ok());
    }

    #[test]
    fn check_dependencies_invalid_default() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: Some("bar".to_owned()),
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        let result = check_dependencies(&toastfile);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("bar"));
    }

    #[test]
    fn check_dependencies_empty() {
        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks: HashMap::new(),
        };

        assert!(check_dependencies(&toastfile).is_ok());
    }

    #[test]
    fn check_dependencies_single() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert!(check_dependencies(&toastfile).is_ok());
    }

    #[test]
    fn check_task_dependencies_nonempty() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );
        tasks.insert(
            "bar".to_owned(),
            Task {
                description: None,
                dependencies: vec!["foo".to_owned()],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert!(check_dependencies(&toastfile).is_ok());
    }

    #[test]
    fn check_dependencies_nonexistent() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );
        tasks.insert(
            "bar".to_owned(),
            Task {
                description: None,
                dependencies: vec!["foo".to_owned(), "baz".to_owned()],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        let result = check_dependencies(&toastfile);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("baz"));
    }

    #[test]
    fn check_dependencies_cycle_1() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec!["foo".to_owned()],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        let result = check_dependencies(&toastfile);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cyclic"));
    }

    #[test]
    fn check_dependencies_cycle_2() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec!["bar".to_owned()],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );
        tasks.insert(
            "bar".to_owned(),
            Task {
                description: None,
                dependencies: vec!["foo".to_owned()],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        let result = check_dependencies(&toastfile);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cyclic"));
    }

    #[test]
    fn check_dependencies_cycle_3() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec!["baz".to_owned()],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );
        tasks.insert(
            "bar".to_owned(),
            Task {
                description: None,
                dependencies: vec!["foo".to_owned()],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );
        tasks.insert(
            "baz".to_owned(),
            Task {
                description: None,
                dependencies: vec!["bar".to_owned()],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        let result = check_dependencies(&toastfile);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cyclic"));
    }

    #[test]
    fn check_task_environment_ok() {
        let mut environment = HashMap::new();
        environment.insert("corge".to_owned(), None);
        environment.insert("grault".to_owned(), None);
        environment.insert("garply".to_owned(), None);

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment,
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        assert!(check_task("foo", &task).is_ok());
    }

    #[test]
    fn check_task_environment_equals() {
        let mut environment = HashMap::new();
        environment.insert("corge".to_owned(), None);
        environment.insert("gra=ult".to_owned(), None);
        environment.insert("garply".to_owned(), None);

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment,
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let result = check_task("foo", &task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains('='));
    }

    #[test]
    fn check_task_paths_ok() {
        let task = Task {
            description: None,
            dependencies: vec![],
            cache: false,
            environment: HashMap::new(),
            input_paths: vec![Path::new("bar").to_owned()],
            excluded_input_paths: vec![Path::new("baz").to_owned()],
            output_paths: vec![Path::new("qux").to_owned()],
            output_paths_on_failure: vec![Path::new("quux").to_owned()],
            mount_paths: vec![
                MappingPath {
                    host_path: Path::new("quuy").to_owned(),
                    container_path: Path::new("quuz").to_owned(),
                },
                MappingPath {
                    host_path: Path::new("quuy").to_owned(),
                    container_path: Path::new("/quuz").to_owned(),
                },
                MappingPath {
                    host_path: Path::new("quuy").to_owned(),
                    container_path: Path::new("/quuz").to_owned(),
                },
                MappingPath {
                    host_path: Path::new("/quuy").to_owned(),
                    container_path: Path::new("/quuz").to_owned(),
                },
            ],
            mount_readonly: false,
            ports: vec![],
            location: Some(Path::new("/corge").to_owned()),
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        assert!(check_task("foo", &task).is_ok());
    }

    #[test]
    fn check_task_paths_absolute_input_paths() {
        #[cfg(unix)]
        let absolute_path = "/bar";

        #[cfg(windows)]
        let absolute_path = "C:\\bar";

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![Path::new(absolute_path).to_owned()],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let result = check_task("foo", &task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(absolute_path));
    }

    #[test]
    fn check_task_paths_absolute_excluded_input_paths() {
        #[cfg(unix)]
        let absolute_path = "/bar";

        #[cfg(windows)]
        let absolute_path = "C:\\bar";

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![Path::new(absolute_path).to_owned()],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let result = check_task("foo", &task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(absolute_path));
    }

    #[test]
    fn check_task_paths_absolute_output_paths() {
        #[cfg(unix)]
        let absolute_path = "/bar";

        #[cfg(windows)]
        let absolute_path = "C:\\bar";

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: false,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![Path::new(absolute_path).to_owned()],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let result = check_task("foo", &task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(absolute_path));
    }

    #[test]
    fn check_task_paths_absolute_output_paths_on_failure() {
        #[cfg(unix)]
        let absolute_path = "/bar";

        #[cfg(windows)]
        let absolute_path = "C:\\bar";

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: false,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![Path::new(absolute_path).to_owned()],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let result = check_task("foo", &task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(absolute_path));
    }

    #[test]
    fn check_task_paths_mount_paths_comma() {
        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![MappingPath {
                host_path: Path::new("bar,baz").to_owned(),
                container_path: Path::new("bar,baz").to_owned(),
            }],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let result = check_task("foo", &task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("bar,baz"));
    }

    #[test]
    fn check_task_paths_relative_location() {
        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Some(Path::new("code").to_owned()),
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let result = check_task("foo", &task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("code"));
    }

    #[test]
    fn check_task_caching_enabled_with_mount_paths() {
        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![MappingPath {
                host_path: Path::new("bar").to_owned(),
                container_path: Path::new("bar").to_owned(),
            }],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let result = check_task("foo", &task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("mount_paths"));
    }

    #[test]
    fn check_task_caching_disabled_with_mount_paths() {
        let task = Task {
            description: None,
            dependencies: vec![],
            cache: false,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![MappingPath {
                host_path: Path::new("bar").to_owned(),
                container_path: Path::new("bar").to_owned(),
            }],
            mount_readonly: false,
            ports: vec!["3000:80".to_owned()],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        assert!(check_task("foo", &task).is_ok());
    }

    #[test]
    fn check_task_caching_enabled_with_ports() {
        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec!["3000:80".to_owned()],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let result = check_task("foo", &task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("caching"));
    }

    #[test]
    fn check_task_caching_disabled_with_ports() {
        let task = Task {
            description: None,
            dependencies: vec![],
            cache: false,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec!["3000:80".to_owned()],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        assert!(check_task("foo", &task).is_ok());
    }

    #[test]
    fn check_task_caching_enabled_with_extra_docker_arguments() {
        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec!["--cpus".to_owned(), "4".to_owned()],
        };

        let result = check_task("foo", &task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("caching"));
    }

    #[test]
    fn check_task_caching_disabled_with_extra_docker_arguments() {
        let task = Task {
            description: None,
            dependencies: vec![],
            cache: false,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec!["--cpus".to_owned(), "4".to_owned()],
        };

        assert!(check_task("foo", &task).is_ok());
    }

    #[test]
    fn environment_empty() {
        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        assert_eq!(environment(&task), Ok(HashMap::new()));
    }

    #[test]
    fn environment_default_overridden() {
        // NOTE: We add an index to the test arg ("foo1", "foo2", ...) to avoid having parallel
        // tests clobbering environment variables used by other threads.
        let mut env_map = HashMap::new();
        env_map.insert("foo1".to_owned(), Some("bar".to_owned()));

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: env_map,
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let mut expected = HashMap::new();
        expected.insert("foo1".to_owned(), "baz".to_owned());

        env::set_var("foo1", "baz");
        assert_eq!(env::var("foo1"), Ok("baz".to_owned()));
        assert_eq!(environment(&task), Ok(expected));
    }

    #[test]
    fn environment_default_not_overridden() {
        // NOTE: We add an index to the test arg ("foo1", "foo2", ...) to avoid having parallel
        // tests clobbering environment variables used by other threads.
        let mut env_map = HashMap::new();
        env_map.insert("foo2".to_owned(), Some("bar".to_owned()));

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: env_map,
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        let mut expected = HashMap::new();
        expected.insert("foo2".to_owned(), "bar".to_owned());

        env::remove_var("foo2");
        assert!(env::var("foo2").is_err());
        assert_eq!(environment(&task), Ok(expected));
    }

    #[test]
    fn environment_missing() {
        // NOTE: We add an index to the test arg ("foo1", "foo2", ...) to avoid having parallel
        // tests clobbering environment variables used by other threads.
        let mut env_map = HashMap::new();
        env_map.insert("foo3".to_owned(), None);

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: env_map,
            input_paths: vec![],
            excluded_input_paths: vec![],
            output_paths: vec![],
            output_paths_on_failure: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: None,
            user: None,
            command: String::new(),
            command_prefix: None,
            extra_docker_arguments: vec![],
        };

        env::remove_var("foo3");
        assert!(env::var("foo3").is_err());
        let result = environment(&task);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err()[0].to_owned(), "foo3");
    }

    #[test]
    fn location_default() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert_eq!(
            location(&toastfile, &toastfile.tasks["foo"]),
            Path::new(DEFAULT_LOCATION),
        );
    }

    #[test]
    fn location_override() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: Some(Path::new("/bar").to_owned()),
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert_eq!(
            location(&toastfile, &toastfile.tasks["foo"]),
            Path::new("/bar"),
        );
    }

    #[test]
    fn user_default() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert_eq!(
            user(&toastfile, &toastfile.tasks["foo"]),
            DEFAULT_USER.to_owned(),
        );
    }

    #[test]
    fn user_override() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: Some("bar".to_owned()),
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert_eq!(user(&toastfile, &toastfile.tasks["foo"]), "bar");
    }

    #[test]
    fn command_default_prefix_default() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "set -euo pipefail".to_owned(),
            tasks,
        };

        assert_eq!(
            command(&toastfile, &toastfile.tasks["foo"]),
            "set -euo pipefail\n".to_owned(),
        );
    }

    #[test]
    fn command_override_prefix_default() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: "echo hello".to_owned(),
                command_prefix: None,
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert_eq!(
            command(&toastfile, &toastfile.tasks["foo"]),
            "\necho hello".to_owned(),
        );
    }

    #[test]
    fn command_default_prefix_override() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: String::new(),
                command_prefix: Some("set -euo pipefail".to_owned()),
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert_eq!(
            command(&toastfile, &toastfile.tasks["foo"]),
            "set -euo pipefail\n".to_owned(),
        );
    }

    #[test]
    fn command_override_prefix_override() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "foo".to_owned(),
            Task {
                description: None,
                dependencies: vec![],
                cache: true,
                environment: HashMap::new(),
                input_paths: vec![],
                excluded_input_paths: vec![],
                output_paths: vec![],
                output_paths_on_failure: vec![],
                mount_paths: vec![],
                mount_readonly: false,
                ports: vec![],
                location: None,
                user: None,
                command: "echo hello".to_owned(),
                command_prefix: Some("set -euo pipefail".to_owned()),
                extra_docker_arguments: vec![],
            },
        );

        let toastfile = Toastfile {
            image: "encom:os-12".to_owned(),
            default: None,
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command_prefix: "".to_owned(),
            tasks,
        };

        assert_eq!(
            command(&toastfile, &toastfile.tasks["foo"]),
            "set -euo pipefail\necho hello".to_owned(),
        );
    }
}
