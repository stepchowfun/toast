use {
    crate::{failure, failure::Failure},
    serde::Deserialize,
};

pub const REPO_DEFAULT: &str = "toast";
pub const EMPTY_CONFIG: &str = "{}";
const DOCKER_CLI_DEFAULT: &str = "docker";

// A program configuration
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    #[serde(default = "default_docker_cli")]
    pub docker_cli: String,

    #[serde(default = "default_docker_repo")]
    pub docker_repo: String,

    #[serde(default = "default_read_local_cache")]
    pub read_local_cache: bool,

    #[serde(default = "default_write_local_cache")]
    pub write_local_cache: bool,

    #[serde(default = "default_read_remote_cache")]
    pub read_remote_cache: bool,

    #[serde(default = "default_write_remote_cache")]
    pub write_remote_cache: bool,
}

fn default_docker_cli() -> String {
    DOCKER_CLI_DEFAULT.to_owned()
}

fn default_docker_repo() -> String {
    REPO_DEFAULT.to_owned()
}

fn default_read_local_cache() -> bool {
    true
}

fn default_write_local_cache() -> bool {
    true
}

fn default_read_remote_cache() -> bool {
    false
}

fn default_write_remote_cache() -> bool {
    false
}

// Parse a program configuration.
pub fn parse(config: &str) -> Result<Config, Failure> {
    serde_yaml::from_str(config).map_err(failure::user("Syntax error."))
}

#[cfg(test)]
mod tests {
    use crate::config::{parse, Config, DOCKER_CLI_DEFAULT, EMPTY_CONFIG};

    #[test]
    fn parse_empty() {
        let result = Config {
            docker_cli: DOCKER_CLI_DEFAULT.to_owned(),
            docker_repo: "toast".to_owned(),
            read_local_cache: true,
            write_local_cache: true,
            read_remote_cache: false,
            write_remote_cache: false,
        };

        assert_eq!(parse(EMPTY_CONFIG).unwrap(), result);
    }

    #[test]
    fn parse_nonempty() {
        let config = r"
docker_cli: podman
docker_repo: foo
read_local_cache: false
write_local_cache: false
read_remote_cache: true
write_remote_cache: true
    "
        .trim();

        let result = Config {
            docker_cli: "podman".to_owned(),
            docker_repo: "foo".to_owned(),
            read_local_cache: false,
            write_local_cache: false,
            read_remote_cache: true,
            write_remote_cache: true,
        };

        assert_eq!(parse(config).unwrap(), result);
    }
}
