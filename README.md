# Bake

[![Build Status](https://travis-ci.org/stepchowfun/bake.svg?branch=master)](https://travis-ci.org/stepchowfun/bake)

*Bake* is a containerized build system.

## Usage

To build a single task and its dependencies, run:

```sh
bake my_task
```

To build all the tasks, just run `bake` with no arguments. Here are all the supported command-line options:

```
USAGE:
    bake [FLAGS] [OPTIONS] [TASKS]...

FLAGS:
    -h, --help       Prints help information
    -s, --shell      Drops you into a shell after the tasks are complete
    -v, --version    Prints version information

OPTIONS:
    -f, --file <PATH>    Sets the path to the bakefile (default:
                         bake.yml)

ARGS:
    <TASKS>...    Sets the tasks to run
```

## Bakefiles

A *bakefile* is a YAML file (typically named `bake.yml`) that defines a directed acyclic graph of tasks and their dependencies. The schema contains three top-level keys:

```yaml
image: <Docker image name>
default: <name of default task to run (default behavior: run all tasks)>
tasks: <map from task name to task>
```

The following formats are supported for `image`: `name`, `name:tag`, or `name@digest`. You can also refer to an image in a custom registry, for example `myregistry.com:5000/testing/test-image`.

Tasks have the following schema:

```yaml
dependencies: <names of dependencies (default: [])>
cache: <whether a task can be cached (default: true)>
args: <map from string to string or null (default: {})>
paths: <paths to copy into the container (default: [])>
location: <path in the container for running this task (default: /scratch)>
user: <name of the user in the container for running this task (default: root)>
command: <shell command to run in the container (default: null)>
```

The simplest bakefile has no tasks:

```yaml
image: alpine
tasks: {}
```

Here is an example bakefile:

```yaml
image: ubuntu:18.04
default: build
tasks:
  rust_dependencies:
    command: |
      DEBIAN_FRONTEND=noninteractive apt-get --yes update
      DEBIAN_FRONTEND=noninteractive apt-get --yes install build-essential curl

  user:
    command: useradd --user-group --create-home user

  rust:
    dependencies:
      - rust_dependencies
      - user
    user: user
    command: curl https://sh.rustup.rs -sSf | sh -s -- -y

  build:
    dependencies:
      - rust
    paths:
      - Cargo.lock
      - Cargo.toml
      - src
    user: user
    command: cargo build

  test:
    dependencies:
      - build
    user: user
    command: cargo test
```
