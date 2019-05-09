# Bake

[![Build Status](https://travis-ci.org/stepchowfun/bake.svg?branch=master)](https://travis-ci.org/stepchowfun/bake)

*Bake* is a containerized build system. You define tasks and their dependencies in a *bakefile*, and Bake runs them in containers based on a Docker image of your choosing. Bake supports local and remote caching to avoid repeating work.

Running tasks in containers helps with reproducibility. If a Bake task works on your machine, it'll work on your teammate's machine too. You don't have to worry about ensuring everyone has the same versions of all the tools and dependencies.

![Welcome to Bake.](https://s3.amazonaws.com/static.stephanboyer.com/bake/welcome.svg)

Here are two reasons to use Bake on top of vanilla Docker:

- Bake allows you to define an arbitrary directed acyclic graph (DAG) of **tasks** and **dependencies**. You can define tasks for installing dependencies, building the application, running tests, linting, deploying, etc.
- Bake supports **remote caching** of tasks. You don't have to manually build and distribute a Docker image with pre-installed tools, libraries, etc. Just define a task which installs those things, and let Bake handle the rest.

On the other hand, here are two reasons *not* to use Bake:

- Bake is not suitable for tasks that cannot run in Linux containers (e.g., builds for iOS applications).
- Bake tasks cannot run a Docker daemon (e.g., to build an image), because [containers don't nest well](https://jpetazzo.github.io/2015/09/03/do-not-use-docker-in-docker-for-ci/). In the future, Bake may provide an option to expose the host's Docker daemon, but this is not currently supported.

Bake has no knowledge of specific programming languages or frameworks. You can use Bake with another tool like [Bazel](https://bazel.build/) or [Buck](https://buckbuild.com/) to perform language-specific build tasks.

## Tutorial

### A simple task

Let's create a simple bakefile. Create a file named `bake.yml` with the following contents:

```yaml
image: ubuntu
tasks:
  greet:
    command: echo 'Hello, World!'
```

Now run `bake`. You should see the following:

![A simple task.](https://s3.amazonaws.com/static.stephanboyer.com/bake/minimal-task.svg)

If you run it again, Bake will find that nothing has changed and skip the task:

![Caching.](https://s3.amazonaws.com/static.stephanboyer.com/bake/caching.svg)

Bake caches tasks to save you time. For example, you don't want to reinstall your dependencies every time you run your tests. However, caching may not be appropriate for some tasks, like deploying your application. You can disable caching for a specific task and all tasks that depend on it with the `cache` option:

```yaml
image: ubuntu
tasks:
  greet:
    cache: false
    command: echo 'Hello, World!'
```

### Adding a dependency

Let's make the greeting more fun with a program called `figlet`. We'll add a task to install `figlet`, and we'll change the `greet` task to depend on it:

```yaml
image: ubuntu
tasks:
  figlet:
    command: |
      apt-get update
      apt-get install --yes figlet

  greet:
    dependencies:
      - figlet
    command: figlet 'Hello, World!'
```

Run `bake` to see a marvelous greeting:

![Adding a dependency.](https://s3.amazonaws.com/static.stephanboyer.com/bake/dependency.svg)

### Using files from the host

Here's a more realistic example. Suppose you want to compile and run a simple C program. Create a file called `main.c`:

```c
#include <stdio.h>

int main(void) {
  printf("Hello, World!\n");
}
```

Update `bake.yml` to compile and run the program:

```yaml
image: ubuntu
tasks:
  gcc:
    command: |
      apt-get update
      apt-get install --yes gcc

  build:
    dependencies:
      - gcc
    input_paths:
      - main.c
    command: gcc main.c

  run:
    dependencies:
      - build
    command: ./a.out
```

Notice the `input_paths` array in the `build` task. Here we are copying a single file into the container, but we could instead copy the entire working directory with `.`. By default, the files will be copied into a directory called `/scratch` in the container. The commands will be run in that directory as well.

Now if you run `bake`, you'll see this:

![Adding files from the host.](https://s3.amazonaws.com/static.stephanboyer.com/bake/input-paths.svg)

### Exporting files from the container

A common use case for Bake is to build a project. Naturally, you might wonder how to access the build artifacts produced inside the container. It's easy to do with `output_paths`:

```yaml
image: ubuntu
tasks:
  gcc:
    command: |
      apt-get update
      apt-get install --yes gcc

  build:
    dependencies:
      - gcc
    input_paths:
      - main.c
    output_paths:
      - a.out
    command: gcc main.c
```

When Bake runs the `build` task, it will copy the `a.out` file to the host.

![Exporting files from the container.](https://s3.amazonaws.com/static.stephanboyer.com/bake/output-paths.svg)

### Passing arguments to a task

Sometimes it's useful for tasks to take arguments. For example, a `deploy` task might want to know whether you want to deploy to the `staging` or `production` cluster. To do this, add an `environment` section to your task:

```yaml
image: ubuntu
tasks:
  deploy:
    cache: false
    environment:
      CLUSTER: staging # Deploy to staging by default
    command: echo "Deploying to $CLUSTER..."
```

When you run this task, Bake will read the value from the environment:

![Passing arguments to a task.](https://s3.amazonaws.com/static.stephanboyer.com/bake/arguments-explicit.svg)

If the variable does not exist in the environment, Bake will use the default value:

![Using argument defaults.](https://s3.amazonaws.com/static.stephanboyer.com/bake/arguments-default.svg)

If you don't want to have a default, set it to `null`:

```yaml
image: ubuntu
tasks:
  deploy:
    cache: false
    environment:
      CLUSTER: null # No default provided
    command: echo "Deploying to $CLUSTER..."
```

Now if you run `bake deploy` without specifying a `CLUSTER`, Bake will complain about the missing variable and refuse to run the task.

### Dropping into a shell

If you run Bake with `--shell`, Bake will drop you into an interactive shell inside the container when the requested tasks are finished. Suppose you have the following bakefile:

```yaml
image: ubuntu
tasks:
  figlet:
    command: |
      apt-get update
      apt-get install --yes figlet
```

Now you can run `bake --shell` to play with `figlet`.

![Dropping into a shell.](https://s3.amazonaws.com/static.stephanboyer.com/bake/shell.svg)

When you're done, the container is deleted automatically.

## How Bake works

Given a set of tasks to run, Bake computes a [topological sort](https://en.wikipedia.org/wiki/Topological_sorting) of the dependency DAG to determine in what order to run the tasks. Because Docker doesn't support combining two arbitrary images into one (for good reasons), Bake does not run tasks in parallel and must instead use a sequential execution schedule. You are free to use parallelism within individual tasks, of course.

The topological sort of an arbitrary DAG is not necessarily unique. Bake uses [depth-first search](https://en.wikipedia.org/wiki/Depth-first_search), traversing children in lexicographical order. This algorithm is deterministic and invariant to the order in which tasks and dependencies are listed, so reordering will not invalidate the cache. Furthermore, `bake foo bar` and `bake bar foo` are guaranteed to produce identical schedules.

Bake builds a Docker image for each task and uses it for the next task in the schedule. Each image is tagged with a cache key that incorporates the shell command, the contents of the files copied into the container, and other inputs. If local caching is enabled, these Docker images remain on disk for subsequent executions. If remote caching is enabled, the images will be synchronized with a remote Docker registry.

If a task is marked as non-cacheable, the Docker images for that task and any subsequent tasks in the schedule will not be persisted or uploaded.

## Bakefiles

A *bakefile* is a YAML file (typically named `bake.yml`) that defines tasks and their dependencies. The schema contains three top-level keys:

```yaml
image: <Docker image name>
default: <name of default task to run (default behavior: run all tasks)>
tasks: <map from task name to task>
```

Tasks have the following schema and defaults:

```yaml
dependencies: []   # Names of dependencies
cache: true        # Whether a task can be cached
environment: {}    # Map from environment variable to optional default
input_paths: []    # Paths to copy into the container
output_paths: []   # Paths to copy out of the container
location: /scratch # Path in the container for running this task
user: root         # Name of the user in the container for running this task
command: null      # Shell command to run in the container
```

The [bakefile](https://github.com/stepchowfun/bake/blob/master/bake.yml) for Bake itself is a comprehensive real-world example.

## Cache configuration

Bake supports local and remote caching. By default, only local caching is enabled. Remote caching requires that the Docker Engine is logged into a Docker registry (e.g., via `docker login`).

The caching behavior can be customized with a configuration file. The default location of the configuration file depends on the operating system:

- For macOS, the default location is `~/Library/Preferences/bake/bake.yml`.
- For other platforms, Bake follows the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html). The default location is `~/.config/bake/bake.yml` unless overridden by the `XDG_CONFIG_HOME` environment variable.

The configuration file has the following schema and defaults:

```yaml
docker_repo: bake         # Docker repository
read_local_cache: true    # Whether Bake should read from local cache
write_local_cache: true   # Whether Bake should write to local cache
read_remote_cache: false  # Whether Bake should read from remote cache
write_remote_cache: false # Whether Bake should write to remote cache
```

Each of these options can be overridden via command-line options (see [below](#command-line-options)).

A typical configuration for a continuous integration (CI) environment will enable all forms of caching, whereas for local development you may want to set `write_remote_cache: false` to avoid waiting for remote cache writes. See [`.travis.yml`](https://github.com/stepchowfun/bake/blob/master/.travis.yml) for a complete example of how to use Bake in a CI environment.

## Command-line options

By default, Bake looks for a bakefile called `bake.yml` in the working directory, then in the parent directory, and so on. Any paths in the bakefile are relative to where the bakefile lives, not the working directory. This means you can run Bake from anywhere in your project and get the same results.

Run `bake` with no arguments to execute the default task, or all the tasks if the bakefile doesn't define a default. You can also execute specific tasks and their dependencies:

```sh
bake task1 task2 task3…
```

Here are all the supported command-line options:

```
USAGE:
    bake [OPTIONS] [TASKS]...

OPTIONS:
    -c, --config-file <PATH>
            Sets the path of the config file

    -f, --file <PATH>
            Sets the path to the bakefile

    -h, --help
            Prints help information

        --read-local-cache <BOOL>
            Sets whether local cache reading is enabled

        --read-remote-cache <BOOL>
            Sets whether remote cache reading is enabled

    -r, --repo <REPO>
            Sets the Docker repository

    -s, --shell
            Drops you into a shell after the tasks are finished

    -v, --version
            Prints version information

        --write-local-cache <BOOL>
            Sets whether local cache writing is enabled

        --write-remote-cache <BOOL>
            Sets whether remote cache writing is enabled
```

## Installation

### Easy installation

If you are running macOS or a GNU-based Linux on an x86-64 CPU, you can install Bake with this command:

```sh
curl https://raw.githubusercontent.com/stepchowfun/bake/master/install.sh -LSfs | sh
```

The same command can be used again to update Bake to the latest version.

**NOTE:** Piping `curl` to `sh` is dangerous since the server might be compromised. If you're concerned about this, you can download the installation script and inspect it or choose one of the other installation methods.

#### Customizing the installation

The installation script supports the following environment variables:

- `VERSION=x.y.z` (defaults to the latest version)
- `PREFIX=/path/to/install` (defaults to `/usr/local/bin`)

For example, the following will install Bake into the current directory:

```sh
curl https://raw.githubusercontent.com/stepchowfun/bake/master/install.sh -LSfs | PREFIX=. sh
```

### Manual installation

The [releases page](https://github.com/stepchowfun/bake/releases) has precompiled binaries for macOS or Linux systems running on an x86-64 CPU. You can download one of them and place it in a directory listed in your [`PATH`](https://en.wikipedia.org/wiki/PATH_\(variable\)).

### Installation with Cargo

If you have [Cargo](https://doc.rust-lang.org/cargo/), you can install Bake as follows:

```sh
cargo install bake
```

You can run that command with `--force` to update an existing installation.

## Requirements

- Bake requires [Docker Engine](https://www.docker.com/products/docker-engine) 17.03.0 or later.
- Only Linux-based Docker images are supported. Bake can run on any platform capable of running such images, e.g., macOS with [Docker Desktop](https://www.docker.com/products/docker-desktop).

## Acknowledgements

The inspiration for Bake came from a similar tool used at Airbnb for CI jobs.
