# Bake

[![Build Status](https://travis-ci.org/stepchowfun/bake.svg?branch=master)](https://travis-ci.org/stepchowfun/bake)

*Bake* is a containerized build system. You define tasks and their dependencies in a *bakefile*, and Bake runs them in Docker containers based on an image of your choosing. Bake supports remote caching to avoid doing redundant work.

Running tasks in containers helps with reproducibility. If a Bake task works on your machine, it'll work on your teammate's machine too. You don't have to worry about ensuring everyone has the same versions of all the tools and dependencies.

Here are some reasons to use Bake on top of vanilla Docker:

- Bake supports *remote caching* of intermediate tasks. You don't have to manually build and distribute a Docker image with pre-installed tools, libraries, etc. Just define a Bake task which installs those things, and let Bake take care of distributing the resulting image and rebuilding it when necessary.
- Bake supports *non-cacheable* tasks, such as publishing a library or deploying an application. You can invoke these tasks with secrets such as API keys without worrying about them being persisted.
- Bake allows you to define an arbitrary directed acyclic graph (DAG) of tasks and dependencies. Dockerfiles only support sequential tasks.

## Tutorial

### A simple task

Let's create a simple bakefile. Create a file named `bake.yml` with the following contents:

```yaml
image: ubuntu
tasks:
  greet: echo 'Hello, World!'
```

Now run `bake`. You should see something like the following:

```sh
$ bake
[INFO] The following tasks will be executed in the order given: `greet`.
[INFO] Pulling image `ubuntu`...
       <...>
[INFO] Running task `greet`...
[INFO] echo 'Hello, World!'
Hello, World!
[INFO] Successfully executed 1 task.
```

### Adding a dependency

Let's make the greeting more fun with a program called `cowsay`. We'll add a task to install `cowsay`, and we'll change the `greet` task to depend on it:

```yaml
image: ubuntu
tasks:
  cowsay: |
    apt-get update
    apt-get install --yes cowsay
  greet:
    dependencies:
      - cowsay
    command: /usr/games/cowsay 'Hello, World!'
```

Run `bake` again and you will see:

```sh
[INFO] The following tasks will be executed in the order given: `cowsay` and `greet`.
[INFO] Running task `cowsay`...
[INFO] apt-get update
       apt-get install -y cowsay
       <...>
[INFO] Running task `greet`...
[INFO] /usr/games/cowsay 'Hello, World!'
 _______________
< Hello, World! >
 ---------------
        \   ^__^
         \  (oo)\_______
            (__)\       )\/\
                ||----w |
                ||     ||
[INFO] Successfully executed 2 tasks.
```

Now that's better!

### Using files from the host

Here's a more realistic example. Suppose you want to compile and run a C program. Create a simple C file called `main.c`:

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
  gcc: |
    apt-get update
    apt-get install -y gcc
  build:
    dependencies:
      - gcc
    paths:
      - main.c
    command: gcc main.c
  greet:
    dependencies:
      - build
    command: ./a.out
```

Notice the `paths` array in the `build` task. Here we are copying a single file into the container, but we could copy the entire working directory with `.`. By default, the files will be copied into a directory called `/scratch` in the container. The commands will be run in that directory as well.

If you run `bake`, you will see this:

```sh
$ bake
[INFO] The following tasks will be executed in the order given: `gcc`, `build`, and `greet`.
[INFO] Running task `gcc`...
[INFO] apt-get update
       apt-get install -y gcc
       <...>
[INFO] Running task `build`...
[INFO] gcc main.c
[INFO] Running task `greet`...
[INFO] ./a.out
Hello, World!
[INFO] Successfully executed 3 tasks.
```

## Bakefiles

A *bakefile* is a YAML file (typically named `bake.yml`) that defines tasks and their dependencies. The schema contains three top-level keys:

```yaml
image: <Docker image name>
default: <name of default task to run (default behavior: run all tasks)>
tasks: <map from task name to task>
```

The following formats are supported for `image`: `name`, `name:tag`, or `name@digest`. You can also refer to an image in a custom registry, for example `myregistry.com:5000/testing/test-image`.

Tasks have the following schema and defaults:

```yaml
dependencies: []   # Names of dependencies
cache: true        # Whether a task can be cached
environment: {}    # Map from environment variable to optional default
paths: []          # Paths to copy into the container
location: /scratch # Path in the container for running this task
user: root         # Name of the user in the container for running this task
command: null      # Shell command to run in the container
```

For convenience, a task can also be represented by a string rather than an object. The resulting task uses that string as its `command`, with the other fields set to their defaults. So the following two bakefiles are equivalent:

```yaml
image: alpine
tasks:
  greet: echo 'Hello, World!'
```

```yaml
image: alpine
tasks:
  greet:
    command: echo 'Hello, World!'
```

The [bakefile](https://github.com/stepchowfun/bake/blob/master/bake.yml) for Bake itself is a comprehensive example.

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

A typical configuration for a continuous integration (CI) environment will enable all forms of caching, whereas for local development you may want to set `write_remote_cache: false` to avoid waiting for remote cache writes.

All of these options can be overridden via command-line options (see below).

## Usage

Run `bake` with no arguments to execute the default task, or all the tasks if the bakefile doesn't define a default. You can also execute specific tasks and their dependencies:

```sh
bake task1 task2 task3...
```

Here are all the supported command-line options:

```sh
USAGE:
    bake [OPTIONS] [TASKS]...

OPTIONS:
    -c, --config-file <PATH>
            Sets the path of the config file (default: depends on the OS)

    -f, --file <PATH>
            Sets the path to the bakefile (default: bake.yml)

    -h, --help
            Prints help information

        --read-local-cache <BOOL>
            Sets whether local cache reading is enabled (default: true)

        --read-remote-cache <BOOL>
            Sets whether remote cache reading is enabled (default: false)

    -r, --repo <REPO>
            Sets the Docker repository (default: bake)

    -s, --shell
            Drops you into a shell after the tasks are complete

    -v, --version
            Prints version information

        --write-local-cache <BOOL>
            Sets whether local cache writing is enabled (default: true)

        --write-remote-cache <BOOL>
            Sets whether remote cache writing is enabled (default: false)


ARGS:
    <TASKS>...
            Sets the tasks to run
```

## Dependencies

Bake requires Docker Engine 17.03.0 or later.

## Acknowledgements

The inspiration for Bake came from a similar tool used at Airbnb for continuous integration (CI) jobs. Bake was not designed under the same constraints as Airbnb's CI tool, so it makes somewhat different tradeoffs.
