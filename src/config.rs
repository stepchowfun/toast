use serde::{Deserialize, Serialize};

pub const REPO_DEFAULT: &str = "bake";
pub const EMPTY_CONFIG: &str = "{}";

// A program configuration
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
  #[serde(default = "default_docker_repo")]
  pub docker_repo: String,

  #[serde(default = "default_local_cache")]
  pub local_cache: bool,

  #[serde(default = "default_remote_cache")]
  pub remote_cache: bool,
}

fn default_docker_repo() -> String {
  REPO_DEFAULT.to_owned()
}

fn default_local_cache() -> bool {
  true
}

fn default_remote_cache() -> bool {
  false
}

// Parse config data.
pub fn parse(config: &str) -> Result<Config, String> {
  serde_yaml::from_str(config).map_err(|e| format!("{}", e))
}

#[cfg(test)]
mod tests {
  use crate::config::{parse, Config, EMPTY_CONFIG};

  #[test]
  fn parse_empty() {
    let result = Ok(Config {
      docker_repo: "bake".to_owned(),
      local_cache: true,
      remote_cache: false,
    });

    assert_eq!(parse(EMPTY_CONFIG), result);
  }

  #[test]
  fn parse_nonempty() {
    let config = r#"
docker_repo: foo
local_cache: false
remote_cache: true
    "#
    .trim();

    let result = Ok(Config {
      docker_repo: "foo".to_owned(),
      local_cache: false,
      remote_cache: true,
    });

    assert_eq!(parse(config), result);
  }
}
