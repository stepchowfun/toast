# Toast 🥂

[![Build status](https://github.com/stepchowfun/toast/workflows/Continuous%20integration/badge.svg?branch=main)](https://github.com/stepchowfun/toast/actions?query=branch%3Amain)

*Toast* is a tool for doing work in containers. You define tasks in a YAML file called a *toastfile*, and Toast runs them in a containerized environment based on a Docker image of your choosing. What constitutes a "task" is up to you: tasks can install system packages, build an application, run a test suite, or even serve web pages. Tasks can depend on other tasks, so Toast can be understood as a high-level containerized build system.

![Welcome to Toast.](https://raw.githubusercontent.com/stepchowfun/toast/main/media/welcome-0.svg?sanitize=true)

Here's the toastfile for the example shown above:

```yaml
image: ubuntu
tasks:
  install_gcc:
    command: |
      apt-get update
      apt-get install --yes gcc

  build:
    dependencies:
      - install_gcc
    input_paths:
      - main.c
    command: gcc main.c

  run:
    dependencies:
      - build
    command: ./a.out
```

Toast caches each task by committing the container to an image. The image is tagged with a cryptographic hash of the shell command for the task, the contents of the files copied into the container, and all the other task inputs. This hash allows Toast to skip tasks that haven't changed since the last run.

In addition to local caching, Toast can use a Docker registry as a remote cache. You, your teammates, and your continuous integration (CI) system can all share the same remote cache. Used in this way, your CI system can do all the heavy lifting like building and installing dependencies so you and your team can focus on development.

Related tools:

- [Docker Compose](https://docs.docker.com/compose/): Docker Compose is a convenient Docker-based development environment which shares many features with Toast. However, it doesn't support defining tasks (like `lint`, `test`, `run`, etc.) or remote caching.
- [Nix](https://nixos.org/nix/): Nix achieves reproducible builds by leveraging ideas from functional programming rather than containerization. We're big fans of Nix. However, Nix requires a larger commitment compared to Toast because you have to use the Nix package manager or write your own Nix derivations. For better or worse, Toast allows you to use familiar idioms like `apt-get install ...`.

To prevent Docker images from accumulating on your machine when using Docker-related tools such as Toast or Docker Compose, we recommend using [Docuum](https://github.com/stepchowfun/docuum) to perform least recently used (LRU) image eviction.

## Tutorial

### Defining a simple task

Let's create a toastfile. Create a file named `toast.yml` with the following contents:

```yaml
image: ubuntu
tasks:
  greet:
    command: echo 'Hello, World!' # Toast will run this in a container.
```

Now run `toast`. You should see the following:

![Defining a simple task.](https://raw.githubusercontent.com/stepchowfun/toast/main/media/simple-task-0.svg?sanitize=true)

If you run it again, Toast will find that nothing has changed and skip the task:

![Caching.](https://raw.githubusercontent.com/stepchowfun/toast/main/media/caching-0.svg?sanitize=true)

Toast caches tasks to save you time. For example, you don't want to reinstall your dependencies every time you run your tests. However, caching may not be appropriate for some tasks, like running a development server. You can disable caching for a specific task and all tasks that depend on it with the `cache` option:

```yaml
image: ubuntu
tasks:
  greet:
    cache: false # Don't cache this task.
    command: echo 'Hello, World!'
```

### Adding a dependency

Let's make the greeting more fun with a program called `figlet`. We'll add a task to install `figlet`, and we'll change the `greet` task to depend on it:

```yaml
image: ubuntu
tasks:
  install_figlet:
    command: |
      apt-get update
      apt-get install --yes figlet

  greet:
    dependencies:
      - install_figlet # Toast will run this task first.
    command: figlet 'Hello, World!'
```

Run `toast` to see a marvelous greeting:

![Adding a dependency.](https://raw.githubusercontent.com/stepchowfun/toast/main/media/dependencies-0.svg?sanitize=true)

### Importing files from the host

Here's a more realistic example. Suppose you want to compile and run a simple C program. Create a file called `main.c`:

```c
#include <stdio.h>

int main(void) {
  printf("Hello, World!\n");
}
```

Update `toast.yml` to compile and run the program:

```yaml
image: ubuntu
tasks:
  install_gcc:
    command: |
      apt-get update
      apt-get install --yes gcc

  build:
    dependencies:
      - install_gcc
    input_paths:
      - main.c # Toast will copy this file into the container before running the command.
    command: gcc main.c

  run:
    dependencies:
      - build
    command: ./a.out
```

Notice the `input_paths` array in the `build` task. Here we're copying a single file into the container, but we could instead import the entire directory containing the toastfile with `.`. By default, the files will be copied into a directory called `/scratch` in the container. The commands will be run in that directory as well.

Now if you run `toast`, you'll see this:

![Importing files from the host.](https://raw.githubusercontent.com/stepchowfun/toast/main/media/input-paths-0.svg?sanitize=true)

For subsequent runs, Toast will skip the task if nothing has changed. But if you update the greeting in `main.c`, Toast will detect the change and rerun the `build` and `run` tasks on the next invocation.

### Exporting files from the container

A common use case for Toast is to build a project. Naturally, you might wonder how to access the build artifacts produced inside the container from the host machine. It's easy to do with `output_paths`:

```yaml
image: ubuntu
tasks:
  install_gcc:
    command: |
      apt-get update
      apt-get install --yes gcc

  build:
    dependencies:
      - install_gcc
    input_paths:
      - main.c
    output_paths:
      - a.out # Toast will copy this file onto the host after running the command.
    command: gcc main.c
```

When Toast runs the `build` task, it will copy the `a.out` file to the host.

![Exporting files from the container.](https://raw.githubusercontent.com/stepchowfun/toast/main/media/output-paths-0.svg?sanitize=true)

### Passing arguments to a task

Sometimes it's useful for tasks to take arguments. For example, a `deploy` task might want to know whether you want to deploy to the `staging` or `production` cluster. To do this, add an `environment` section to your task:

```yaml
image: ubuntu
tasks:
  deploy:
    cache: false
    environment:
      CLUSTER: staging # Deploy to staging by default.
    command: echo "Deploying to $CLUSTER..."
```

When you run this task, Toast will read the value from the environment:

![Passing arguments to a task.](https://raw.githubusercontent.com/stepchowfun/toast/main/media/arguments-explicit-0.svg?sanitize=true)

If the variable doesn't exist in the environment, Toast will use the default value:

![Using argument defaults.](https://raw.githubusercontent.com/stepchowfun/toast/main/media/arguments-default-0.svg?sanitize=true)

If you don't want to have a default, set it to `null`:

```yaml
image: ubuntu
tasks:
  deploy:
    cache: false
    environment:
      CLUSTER: null # No default; this variable must be provided at runtime.
    command: echo "Deploying to $CLUSTER..."
```

Now if you run `toast deploy` without specifying a `CLUSTER`, Toast will complain about the missing variable and refuse to run the task.

Environment variables listed in a task are also set for any tasks that run after it.

### Running a server and mounting paths into the container

Toast can be used for more than just building a project. Suppose you're developing a website. You can define a Toast task to run your web server! Create a file called `index.html` with the following contents:

```html
<!DOCTYPE html>
<html>
  <head>
    <title>Welcome to Toast!</title>
  </head>
  <body>
    <p>Hello, World!</p>
  </body>
</html>
```

We can use a web server like [nginx](https://www.nginx.com/). The official `nginx` Docker image will do, but you could also use a more general image and define a Toast task to install nginx.

In our `toast.yml` file, we'll use the `ports` field to make the website accessible outside the container. We'll also use `mount_paths` rather than `input_paths` to synchronize files between the host and the container while the server is running.

```yaml
image: nginx
tasks:
  serve:
    cache: false # It doesn't make sense to cache this task.
    mount_paths:
      - index.html # Synchronize this file between the host and the container.
    ports:
      - 3000:80 # Expose port 80 in the container as port 3000 on the host.
    location: /usr/share/nginx/html/ # Nginx will serve the files in here.
    command: nginx -g 'daemon off;' # Run in foreground mode.
```

Now you can use Toast to run the server:

![Running a server.](https://raw.githubusercontent.com/stepchowfun/toast/main/media/server-1.svg?sanitize=true)

### Configuring the shell

It's often desirable to configure the shell in some way before running any commands. Shells are typically configured with so-called "startup files" (e.g., `~/.bashrc`). However, many shells skip loading such configuration files when running in non-interactive, non-login mode, which is how the shell is invoked by Toast. Toast provides an alternative mechanism to configure the shell that doesn't require creating any special files or invoking the shell in a particular way.

Consider the following toastfile which uses Bash as the shell, since that's the default preferred login shell in Ubuntu:

```yaml
image: ubuntu
tasks:
  install_figlet:
    command: |
      apt-get update
      apt-get install --yes figlet
```

What happens if `apt-get update` fails? Due to the way Bash works, the failure would be ignored and execution would continue to the subsequent line. You can fix this with `set -e` as follows:

```yaml
image: ubuntu
tasks:
  install_figlet:
    command: |
      set -e # Make Bash fail fast.
      apt-get update
      apt-get install --yes figlet
```

However, it's tedious and error-prone to add that to each task separately. Instead, you can add it to every task at once by setting `command_prefix` as follows:

```yaml
image: ubuntu
command_prefix: set -e # Make Bash fail fast.
tasks:
  install_figlet:
    command: |
      apt-get update
      apt-get install --yes figlet
```

For Bash in particular, we recommend going even further and setting `set -euo pipefail` instead of just `set -e`.

### Dropping into an interactive shell

If you run Toast with `--shell`, Toast will drop you into an interactive shell inside the container when the requested tasks are finished, or if any of them fails. This feature is useful for debugging tasks or exploring what's in the container. Suppose you have the following toastfile:

```yaml
image: ubuntu
tasks:
  install_figlet:
    command: |
      apt-get update
      apt-get install --yes figlet
```

You can run `toast --shell` to play with the `figlet` program:

![Dropping into a shell.](https://raw.githubusercontent.com/stepchowfun/toast/main/media/shell-0.svg?sanitize=true)

When you're done, the container is deleted automatically.

## How Toast works

Given a set of tasks to run, Toast computes a [topological sort](https://en.wikipedia.org/wiki/Topological_sorting) of the dependency DAG to determine in what order to run the tasks. Toast then builds a Docker image for each task based on the image from the previous task in the topological sort, or the base image in the case of the first task.

The topological sort of an arbitrary DAG isn't necessarily unique. Toast uses an algorithm based on depth-first search, traversing children in lexicographical order. The algorithm is deterministic and invariant to the order in which tasks and dependencies are listed, so reordering tasks in a toastfile won't invalidate the cache. Furthermore, `toast foo bar` and `toast bar foo` are guaranteed to produce identical schedules to maximize cache utilization.

For each task in the schedule, Toast first computes a cache key based on a hash of the shell command, the contents of the `input_paths`, the cache key of the previous task in the schedule, etc. Toast will then look for a Docker image tagged with that cache key. If the image is found, Toast will skip the task. Otherwise, Toast will create a container, copy any `input_paths` into it, run the shell command, copy any `output_paths` from the container to the host, commit the container to an image, and delete the container. The image is tagged with the cache key so the task can be skipped for subsequent runs.

Toast aims to make as few assumptions about the container environment as possible. Toast only assumes there is a program at `/bin/su` which can be invoked as `su -c COMMAND USER`. This program is used to run commands for tasks in the container as the appropriate user with their preferred shell. Every popular Linux distribution has a `su` utility that supports this usage. Toast has integration tests to ensure it works with popular base images such as `debian`, `alpine`, `busybox`, etc.

## Toastfiles

A *toastfile* is a YAML file (typically named `toast.yml`) that defines tasks and their dependencies. The schema contains the following top-level keys and defaults:

```yaml
image: <required>   # Docker image name with optional tag or digest
default: null       # Name of default task to run or `null` to run all tasks by default
location: /scratch  # Path in the container for running tasks
user: root          # Name of the user in the container for running tasks
command_prefix: ''  # A string to be prepended to all commands by default
tasks: {}           # Map from task name to task
```

Tasks have the following schema and defaults:

```yaml
description: null           # A description of the task for the `--list` option
dependencies: []            # Names of dependencies
cache: true                 # Whether a task can be cached
environment: {}             # Map from environment variable to optional default
input_paths: []             # Paths to copy into the container
excluded_input_paths: []    # A denylist for `input_paths`
output_paths: []            # Paths to copy out of the container if the task succeeds
output_paths_on_failure: [] # Paths to copy out of the container if the task fails
mount_paths: []             # Paths to mount into the container
mount_readonly: false       # Whether to mount the `mount_paths` as readonly
ports: []                   # Port mappings to publish
location: null              # Overrides the corresponding top-level value
user: null                  # Overrides the corresponding top-level value
command: ''                 # Shell command to run in the container
command_prefix: null        # Overrides the corresponding top-level value
extra_docker_arguments: []  # Additional arguments for `docker container create`
```

The [toastfile](https://github.com/stepchowfun/toast/blob/main/toast.yml) for Toast itself is a comprehensive real-world example.

## Configuration

Toast can be customized with a YAML configuration file. The default location of the configuration file depends on the operating system:

- For macOS, the default location is `$HOME/Library/Application Support/toast/toast.yml`.
- For other Unix platforms, Toast follows the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html). The default location is `$XDG_CONFIG_HOME/toast/toast.yml` or `$HOME/.config/toast/toast.yml` if `XDG_CONFIG_HOME` isn't set to an absolute path.
- For Windows, the default location is `{FOLDERID_RoamingAppData}\toast\toast.yml`.

The schema of the configuration file is described in the subsections below.

### Cache configuration

Toast supports local and remote caching. By default, only local caching is enabled. Remote caching requires that the Docker Engine is logged into a Docker registry (e.g., via `docker login`).

The cache-related fields and their default values are as follows:

```yaml
docker_repo: toast        # Docker repository
read_local_cache: true    # Whether Toast should read from local cache
write_local_cache: true   # Whether Toast should write to local cache
read_remote_cache: false  # Whether Toast should read from remote cache
write_remote_cache: false # Whether Toast should write to remote cache
```

Each of these options can be overridden via command-line options (see [below](#command-line-options)).

A typical configuration for a CI environment will enable all forms of caching, whereas for local development you may want to set `write_remote_cache: false` to avoid waiting for remote cache writes.

### Docker CLI

You can configure the Docker CLI binary used by Toast. Toast uses the `PATH` environment variable to search for the specified binary. You can use this mechanism to switch to a drop-in replacement for the Docker CLI, such as Podman.

The relevant field and its default value are as follows:

```yaml
docker_cli: docker
```

## Command-line options

By default, Toast looks for a toastfile called `toast.yml` in the working directory, then in the parent directory, and so on. Any paths in the toastfile are relative to where the toastfile lives, not the working directory. This means you can run Toast from anywhere in your project and get the same results.

Run `toast` with no arguments to execute the default task, or all the tasks if the toastfile doesn't define a default. You can also execute specific tasks and their dependencies:

```sh
toast task1 task2 task3…
```

Here are all the supported command-line options:

```
USAGE:
    toast [OPTIONS] [--] [TASKS]...

OPTIONS:
    -c, --config-file <PATH>
            Sets the path of the config file

        --docker-cli <CLI>
            Sets the Docker CLI binary

    -r, --docker-repo <REPO>
            Sets the Docker repository

    -f, --file <PATH>
            Sets the path to the toastfile

        --force <TASK>...
            Runs a task unconditionally, even if it’s cached

    -h, --help
            Prints help information

    -l, --list
            Lists the tasks that have a description

        --read-local-cache <BOOL>
            Sets whether local cache reading is enabled

        --read-remote-cache <BOOL>
            Sets whether remote cache reading is enabled

    -s, --shell
            Drops you into a shell after the tasks are finished

    -v, --version
            Prints version information

        --write-local-cache <BOOL>
            Sets whether local cache writing is enabled

        --write-remote-cache <BOOL>
            Sets whether remote cache writing is enabled


ARGS:
    <TASKS>...
            Sets the tasks to run
```

## Installation instructions

### Installation on macOS or Linux (x86-64)

If you're running macOS or Linux on an x86-64 CPU, you can install Toast with this command:

```sh
curl https://raw.githubusercontent.com/stepchowfun/toast/main/install.sh -LSfs | sh
```

The same command can be used again to update to the latest version.

The installation script supports the following optional environment variables:

- `VERSION=x.y.z` (defaults to the latest version)
- `PREFIX=/path/to/install` (defaults to `/usr/local/bin`)

For example, the following will install Toast into the working directory:

```sh
curl https://raw.githubusercontent.com/stepchowfun/toast/main/install.sh -LSfs | PREFIX=. sh
```

If you prefer not to use this installation method, you can download the binary from the [releases page](https://github.com/stepchowfun/toast/releases), make it executable (e.g., with `chmod`), and place it in some directory in your [`PATH`](https://en.wikipedia.org/wiki/PATH_\(variable\)) (e.g., `/usr/local/bin`).

### Installation on Windows (x86-64)

If you're running Windows on an x86-64 CPU, download the latest binary from the [releases page](https://github.com/stepchowfun/toast/releases) and rename it to `toast` (or `toast.exe` if you have file extensions visible). Create a directory called `Toast` in your `%PROGRAMFILES%` directory (e.g., `C:\Program Files\Toast`), and place the renamed binary in there. Then, in the "Advanced" tab of the "System Properties" section of Control Panel, click on "Environment Variables..." and add the full path to the new `Toast` directory to the `PATH` variable under "System variables". Note that the `Program Files` directory might have a different name if Windows is configured for a language other than English.

To update to an existing installation, simply replace the existing binary.

### Installation on Debian or Ubuntu

If you use Debian or Ubuntu, you can build and install Toast from source from the [MPR](https://mpr.makedeb.org/packages/toast). You'll need [makedeb](https://makedeb.org) installed in order to build it.

```sh
git clone 'https://mpr.makedeb.org/toast'
cd toast/
makedeb -si
```

Alternatively, you can set up the [Prebuilt-MPR](https://docs.makedeb.org/prebuilt-mpr/getting-started) and then install Toast directly with APT:

```sh
sudo apt install toast
```

### Installation with Homebrew

If you have [Homebrew](https://brew.sh/), you can install Toast as follows:

```sh
brew install toast
```

You can update an existing installation with `brew upgrade toast`.

### Installation with MacPorts

On macOS, you can also install Toast via [MacPorts](https://www.macports.org) as follows:

```sh
sudo port install toast
```

You can update an existing installation via:

```sh
sudo port selfupdate
sudo port upgrade toast
```

### Installation with Cargo

If you have [Cargo](https://doc.rust-lang.org/cargo/), you can install Toast as follows:

```sh
cargo install toast
```

You can run that command with `--force` to update an existing installation.

## Running Toast in CI

The easiest way to run Toast in CI is to use [GitHub Actions](https://help.github.com/en/actions). Toast provides a convenient GitHub action that you can use in your [workflows](https://help.github.com/en/actions/configuring-and-managing-workflows/configuring-and-managing-workflow-files-and-runs). Here's a simple workflow that runs Toast with no arguments:

```yaml
# .github/workflows/ci.yml
name: Continuous integration
on: [push, pull_request]
jobs:
  ci:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v2
    - uses: stepchowfun/toast/.github/actions/toast@main
```

Here's a more customized workflow that showcases all the options:

```yaml
# .github/workflows/ci.yml
name: Continuous integration
on: [push, pull_request]
jobs:
  ci:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v2
    - uses: azure/docker-login@v1
      with:
        username: DOCKER_USERNAME
        password: ${{ secrets.DOCKER_PASSWORD }}
      if: github.event_name == 'push'
    - uses: stepchowfun/toast/.github/actions/toast@main
      with:
        file: toastfiles/toast.yml
        tasks: build lint test
        docker_repo: DOCKER_USERNAME/DOCKER_REPO
        write_remote_cache: ${{ github.event_name == 'push' }}
```

## Requirements

- Toast requires [Docker Engine](https://www.docker.com/products/docker-engine) 17.06.0 or later.
- Toast only works with Linux containers; Windows containers aren't currently supported. However, in addition to Linux hosts, Toast also supports macOS and Windows hosts with the appropriate virtualization capabilities thanks to [Docker Desktop](https://www.docker.com/products/docker-desktop).

## Acknowledgements

Toast was inspired by an in-house tool used at Airbnb for CI jobs. The design was heavily influenced by the lessons I learned working on that tool and building out Airbnb's CI system with the fabulous CI Infrastructure Team.

Special thanks to Julia Wang ([@juliahw](https://github.com/juliahw)) for valuable early feedback. Thanks to her and Mark Tai ([@marktai](https://github.com/marktai)) for coming up with the name *Toast*.

The terminal animations were produced with [asciinema](https://asciinema.org/) and [svg-term-cli](https://github.com/marionebl/svg-term-cli).
