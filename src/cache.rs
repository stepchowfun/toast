use crate::{failure, failure::Failure, toastfile::Task};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io, io::Read};

// Bump this if we need to invalidate all existing caches for some reason.
const CACHE_VERSION: usize = 0;

// Determine the cache ID of a task based on the cache ID of the previous task in the schedule (or
// the hash of the base image, if this is the first task).
pub fn key(
    previous_key: &str,
    task: &Task,
    input_files_hash: &str,
    environment: &HashMap<String, String>,
) -> String {
    // Start with the previous key.
    let mut cache_key = previous_key.to_owned();

    // If there are no environment variables, no input paths, no command to run, we can just use the
    // cache key from the previous task.
    if task.environment.is_empty() && task.input_paths.is_empty() && task.command.is_empty() {
        return cache_key;
    }

    // Incorporate the cache version.
    cache_key = combine(&cache_key, &format!("{}", CACHE_VERSION));

    // Environment variables
    let mut environment_hash = String::new();
    let mut variables = task.environment.keys().collect::<Vec<_>>();
    variables.sort();
    for variable in variables {
        // The variable name
        environment_hash = combine(&environment_hash, variable);

        // The value [ref:environment_valid]
        environment_hash = combine(&environment_hash, &environment[variable]);
    }
    cache_key = combine(&cache_key, &environment_hash);

    // Input paths and contents
    cache_key = combine(&cache_key, &input_files_hash);

    // Location
    cache_key = combine(&cache_key, &task.location.to_string_lossy());

    // User
    cache_key = combine(&cache_key, &task.user);

    // Command
    cache_key = combine(&cache_key, &task.command);

    // We add this "toast-" prefix because Docker has a rule that tags cannot be 64-byte hexadecimal
    // strings. See this for more details: https://github.com/moby/moby/issues/20972
    format!("toast-{}", cache_key)
}

// Compute the hash of a readable object (e.g., a file). This function does not need to load all the
// data in memory at the same time. The guarantees:
//   1. For all `x`, `hash_str(x)` = `hash_str(x)`.
//   1. For all known `x` and `y`, `x` != `y` implies `hash_str(x)` != `hash_str(y)`.
pub fn hash_read<R: Read>(input: &mut R) -> Result<String, Failure> {
    let mut hasher = Sha256::new();
    io::copy(input, &mut hasher).map_err(failure::system("Unable to compute hash."))?;
    Ok(hex::encode(hasher.result()))
}

// Compute the hash of a string. The guarantees:
//   1. For all `x`, `hash_str(x)` = `hash_str(x)`.
//   1. For all known `x` and `y`, `x` != `y` implies `hash_str(x)` != `hash_str(y)`.
pub fn hash_str(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

// Combine two strings into a hash. The guarantees:
//   1. For all `x` and `y`, `combine(x, y)` = `combine(x, y)`.
//   2. For all known `x1`, `x2`, `y1`, and `y2`,
//      `x1` != `x2` implies `combine(x1, y1)` != `combine(x2, y2)`.
//   3. For all known `x1`, `x2`, `y1`, and `y2`,
//      `y1` != `y2` implies `combine(x1, y1)` != `combine(x2, y2)`.
pub fn combine(x: &str, y: &str) -> String {
    // Why not just take the hash of the concatenation of the two strings? Because then ("foo",
    // "bar") would have the same hash as ("foob", "ar"). To disambiguate in this situation, we also
    // include the length of the first string.
    hash_str(&format!("{}:{}{}", x.len(), x, y))
}

#[cfg(test)]
mod tests {
    use crate::{
        cache::{combine, hash_read, hash_str, key},
        toastfile::{Task, DEFAULT_LOCATION, DEFAULT_USER},
    };
    use std::{collections::HashMap, path::Path};

    #[test]
    fn key_noop() {
        let previous_key = "corge";

        let environment: HashMap<String, Option<String>> = HashMap::new();

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment,
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: String::new(),
        };

        let input_files_hash = "grault";

        let full_environment = HashMap::new();

        assert_eq!(
            previous_key,
            key(previous_key, &task, input_files_hash, &full_environment),
        );
    }

    #[test]
    fn key_pure() {
        let previous_key = "corge";

        let mut environment: HashMap<String, Option<String>> = HashMap::new();
        environment.insert("foo".to_owned(), None);

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment,
            input_paths: vec![Path::new("flob").to_owned()],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo wibble".to_owned(),
        };

        let input_files_hash = "grault";

        let mut full_environment = HashMap::new();
        full_environment.insert("foo".to_owned(), "qux".to_owned());

        assert_eq!(
            key(previous_key, &task, input_files_hash, &full_environment),
            key(previous_key, &task, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_previous_key() {
        let previous_key1 = "foo";
        let previous_key2 = "bar";

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo wibble".to_owned(),
        };

        let input_files_hash = "grault";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key1, &task, input_files_hash, &full_environment),
            key(previous_key2, &task, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_environment_order() {
        let previous_key = "corge";

        let mut environment1: HashMap<String, Option<String>> = HashMap::new();
        environment1.insert("foo".to_owned(), None);
        environment1.insert("bar".to_owned(), None);

        let mut environment2: HashMap<String, Option<String>> = HashMap::new();
        environment2.insert("bar".to_owned(), None);
        environment2.insert("foo".to_owned(), None);

        let task1 = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: environment1,
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo wibble".to_owned(),
        };

        let task2 = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: environment2,
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo wibble".to_owned(),
        };

        let input_files_hash = "grault";

        let mut full_environment = HashMap::new();
        full_environment.insert("foo".to_owned(), "qux".to_owned());
        full_environment.insert("bar".to_owned(), "fum".to_owned());

        assert_eq!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_environment_keys() {
        let previous_key = "corge";

        let mut environment1: HashMap<String, Option<String>> = HashMap::new();
        environment1.insert("foo".to_owned(), None);

        let mut environment2: HashMap<String, Option<String>> = HashMap::new();
        environment2.insert("bar".to_owned(), None);

        let task1 = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: environment1,
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo wibble".to_owned(),
        };

        let task2 = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: environment2,
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo wibble".to_owned(),
        };

        let input_files_hash = "grault";

        let mut full_environment = HashMap::new();
        full_environment.insert("foo".to_owned(), "qux".to_owned());
        full_environment.insert("bar".to_owned(), "fum".to_owned());

        assert_ne!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_environment_values() {
        let previous_key = "corge";

        let mut environment: HashMap<String, Option<String>> = HashMap::new();
        environment.insert("foo".to_owned(), None);

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment,
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo wibble".to_owned(),
        };

        let input_files_hash = "grault";

        let mut full_environment1 = HashMap::new();
        full_environment1.insert("foo".to_owned(), "bar".to_owned());
        let mut full_environment2 = HashMap::new();
        full_environment2.insert("foo".to_owned(), "baz".to_owned());

        assert_ne!(
            key(previous_key, &task, input_files_hash, &full_environment1),
            key(previous_key, &task, input_files_hash, &full_environment2)
        );
    }

    #[test]
    fn key_input_files_hash() {
        let previous_key = "corge";

        let task = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![Path::new("flob").to_owned()],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo wibble".to_owned(),
        };

        let input_files_hash1 = "foo";
        let input_files_hash2 = "bar";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key, &task, input_files_hash1, &full_environment),
            key(previous_key, &task, input_files_hash2, &full_environment)
        );
    }

    #[test]
    fn key_location() {
        let previous_key = "corge";

        let task1 = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new("/foo").to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo wibble".to_owned(),
        };

        let task2 = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new("/bar").to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo wibble".to_owned(),
        };

        let input_files_hash = "grault";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_user() {
        let previous_key = "corge";

        let task1 = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: "foo".to_owned(),
            command: "echo wibble".to_owned(),
        };

        let task2 = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: "bar".to_owned(),
            command: "echo wibble".to_owned(),
        };

        let input_files_hash = "grault";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_command() {
        let previous_key = "corge";

        let task1 = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo foo".to_owned(),
        };

        let task2 = Task {
            description: None,
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            input_paths: vec![],
            output_paths: vec![],
            mount_paths: vec![],
            mount_readonly: false,
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: "echo bar".to_owned(),
        };

        let input_files_hash = "grault";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn hash_read_pure() {
        let mut str1 = b"foo" as &[u8];
        let mut str2 = b"foo" as &[u8];
        assert_eq!(hash_read(&mut str1).unwrap(), hash_read(&mut str2).unwrap());
    }

    #[test]
    fn hash_read_not_constant() {
        let mut str1 = b"foo" as &[u8];
        let mut str2 = b"bar" as &[u8];
        assert_ne!(hash_read(&mut str1).unwrap(), hash_read(&mut str2).unwrap());
    }

    #[test]
    fn hash_str_pure() {
        assert_eq!(hash_str("foo"), hash_str("foo"));
    }

    #[test]
    fn hash_str_not_constant() {
        assert_ne!(hash_str("foo"), hash_str("bar"));
    }

    #[test]
    fn combine_pure() {
        assert_eq!(combine("foo", "bar"), combine("foo", "bar"));
    }

    #[test]
    fn combine_first_different() {
        assert_ne!(combine("foo", "bar"), combine("foo", "baz"));
    }

    #[test]
    fn combine_second_different() {
        assert_ne!(combine("foo", "bar"), combine("baz", "bar"));
    }

    #[test]
    fn combine_concat() {
        assert_ne!(combine("foo", "bar"), combine("foob", "ar"));
    }
}
