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
    bake [OPTIONS] [TASKS]...

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -f, --file <PATH>    Sets the path to the bakefile (default: bake.yml)

ARGS:
    <TASKS>...    Sets the tasks to run
```

## Bakefiles

A *bakefile* is a YAML file (typically named `bake.yml`) that defines a directed acyclic graph of tasks and their dependencies. The schema contains two top-level keys:

```yaml
image: <Docker image name>
tasks: <map from task name to task>
```

The following formats are supported for `image`: `name`, `name:tag`, or `name@digest`. You can also refer to an image in a custom registry, for example `myregistry.com:5000/testing/test-image`.

Tasks have the following schema:

```yaml
dependencies: <names of dependencies (default: [])>
cache: <whether a task can be cached (default: true)>
args: <map from string to string or null (default: {})>
files: <paths to copy into the container (default: [])>
location: <path in the container for running this task>
command: <shell command to run in the container>
```

The simplest bakefile has no tasks:

```
image: alpine
tasks: {}
```

Here is an example bakefile:

```
image: ubuntu:18.04
tasks:
  install_rust:
    command: apt-get install rust

  build:
    dependencies:
      - install_rust
    files:
      - Cargo.lock
      - Cargo.toml
      - src
    location: /scratch
    command: cargo build

  publish:
    dependencies:
      - build
    cache: false
    args:
      AWS_ACCESS_KEY_ID: null
      AWS_DEFAULT_REGION: null
      AWS_SECRET_ACCESS_KEY: null
    location: /scratch
    command: aws s3 cp target/release/program s3://bucket/program
```
