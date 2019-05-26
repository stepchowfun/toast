# Toast 🥂

[![Build Status](https://travis-ci.org/stepchowfun/toast.svg?branch=master)](https://travis-ci.org/stepchowfun/toast)

*Toast* is a tool for running tasks in containers. You define tasks in a *toastfile*, and Toast runs them in an environment based on a Docker image of your choosing. Tasks can depend on other tasks, which makes Toast similar to a build system. What constitutes a "task" is up to you: tasks can install system packages, build an application, run a test suite, serve web pages, deploy a service, etc.

Toast supports local and remote caching to avoid repeating work. Toast records a diff of the entire filesystem after each task by committing the container to an image. Each image is tagged with a cache key that incorporates the shell command for the task, the contents of the files copied into the container, and all the other task inputs. If remote caching is enabled, Toast will upload the images to a Docker registry to be used by other machines.

![Welcome to Toast.](https://raw.githubusercontent.com/stepchowfun/toast/master/media/welcome-0.svg?sanitize=true)

The tutorial below aims to demonstrate how Toast can simplify your development workflow. On the other hand, here are two situations for which Toast is *not* suitable:

- Tasks that cannot run in Linux containers: for example, you wouldn't use Toast to build an iOS application.
- Multi-container applications: you can use a tool like [Docker Compose](https://docs.docker.com/compose/overview/) to do that, but you will forgo some toasty benefits like remote caching, filesystem watching, and the ability to define tasks and dependencies.

Toast has no knowledge of specific programming languages or frameworks. You can use Toast with another tool like [Bazel](https://bazel.build/) or [Buck](https://buckbuild.com/) to perform language-specific build tasks.

## Tutorial

### A simple task

Let's create a simple toastfile. Create a file named `toast.yml` with the following contents:

```yaml
image: ubuntu
tasks:
  greet:
    command: echo 'Hello, World!'
```

Now run `toast`. You should see the following:

![A simple task.](https://raw.githubusercontent.com/stepchowfun/toast/master/media/simple-task-0.svg?sanitize=true)

If you run it again, Toast will find that nothing has changed and skip the task:

![Caching.](https://raw.githubusercontent.com/stepchowfun/toast/master/media/caching-0.svg?sanitize=true)

Toast caches tasks to save you time. For example, you don't want to reinstall your dependencies every time you run your tests. However, caching may not be appropriate for some tasks, like deploying your application. You can disable caching for a specific task and all tasks that depend on it with the `cache` option:

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
  install_figlet:
    command: |
      apt-get update
      apt-get install --yes figlet

  greet:
    dependencies:
      - install_figlet
    command: figlet 'Hello, World!'
```

Run `toast` to see a marvelous greeting:

![Adding a dependency.](https://raw.githubusercontent.com/stepchowfun/toast/master/media/dependencies-0.svg?sanitize=true)

### Using files from the host

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
      - main.c
    command: gcc main.c

  run:
    dependencies:
      - build
    command: ./a.out
```

Notice the `input_paths` array in the `build` task. Here we are copying a single file into the container, but we could instead copy the entire working directory with `.`. By default, the files will be copied into a directory called `/scratch` in the container. The commands will be run in that directory as well.

Now if you run `toast`, you'll see this:

![Adding files from the host.](https://raw.githubusercontent.com/stepchowfun/toast/master/media/input-paths-0.svg?sanitize=true)

### Exporting files from the container

A common use case for Toast is to build a project. Naturally, you might wonder how to access the build artifacts produced inside the container. It's easy to do with `output_paths`:

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
      - a.out
    command: gcc main.c
```

When Toast runs the `build` task, it will copy the `a.out` file to the host.

![Exporting files from the container.](https://raw.githubusercontent.com/stepchowfun/toast/master/media/output-paths-0.svg?sanitize=true)

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

When you run this task, Toast will read the value from the environment:

![Passing arguments to a task.](https://raw.githubusercontent.com/stepchowfun/toast/master/media/arguments-explicit-0.svg?sanitize=true)

If the variable does not exist in the environment, Toast will use the default value:

![Using argument defaults.](https://raw.githubusercontent.com/stepchowfun/toast/master/media/arguments-default-0.svg?sanitize=true)

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

Now if you run `toast deploy` without specifying a `CLUSTER`, Toast will complain about the missing variable and refuse to run the task.

### Running a server and watching the filesystem

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

In our `toast.yml` file, we'll use the `ports` field to make the website accessible outside the container. We'll also set the `watch` flag to enable filesystem watching.

```yml
image: nginx
tasks:
  serve:
    cache: false # It doesn't make sense to cache this task.
    watch: true # Synchronize changes to `index.html`.
    input_paths:
      - index.html
    ports:
      - 3000:80 # Expose port 80 in the container as port 3000 on the host.
    location: /usr/share/nginx/html/ # Nginx will serve the files in here.
    command: nginx -g 'daemon off;' # Run in foreground mode.
```

Now you can use Toast to run the server:

![Running a server.](https://raw.githubusercontent.com/stepchowfun/toast/master/media/server-0.svg?sanitize=true)

### Dropping into a shell

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

![Dropping into a shell.](https://raw.githubusercontent.com/stepchowfun/toast/master/media/shell-0.svg?sanitize=true)

When you're done, the container is deleted automatically.

## How Toast works

Given a set of tasks to run, Toast computes a [topological sort](https://en.wikipedia.org/wiki/Topological_sorting) of the dependency DAG to determine in what order to run the tasks. Because Docker doesn't support combining two arbitrary images into one (for good reasons), Toast does not run tasks in parallel and must instead use a sequential execution schedule. You are free to use parallelism within individual tasks, of course.

The topological sort of an arbitrary DAG is not necessarily unique. Toast uses an algorithm based on depth-first search, traversing children in lexicographical order. The algorithm is deterministic and invariant to the order in which tasks and dependencies are listed, so reordering will not invalidate the cache. Furthermore, `toast foo bar` and `toast bar foo` are guaranteed to produce identical schedules to maximize cache utilization.

Toast aims to make as few assumptions about the container environment as possible. Toast only assumes there is a program at `/bin/su` which can be invoked as `su -c COMMAND USER`. This program is used to run commands for tasks in the container as the appropriate user with their preferred shell.

Every popular Linux distribution has a `su` utility that satisfies this criterion. Toast has integration tests to ensure it works with popular base images such as `debian`, `alpine`, `busybox`, etc.

## Toastfiles

A *toastfile* is a YAML file (typically named `toast.yml`) that defines tasks and their dependencies. The schema contains three top-level keys:

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
watch: false       # Whether to sync input files from the host to the container
input_paths: []    # Paths to copy into the container
output_paths: []   # Paths to copy out of the container
ports: []          # Port mappings to publish
location: /scratch # Path in the container for running this task
user: root         # Name of the user in the container for running this task
command: null      # Shell command to run in the container
```

The [toastfile](https://github.com/stepchowfun/toast/blob/master/toast.yml) for Toast itself is a comprehensive real-world example.

## Cache configuration

Toast supports local and remote caching. By default, only local caching is enabled. Remote caching requires that the Docker Engine is logged into a Docker registry (e.g., via `docker login`).

The caching behavior can be customized with a configuration file. The default location of the configuration file depends on the operating system:

- For macOS, the default location is `~/Library/Preferences/toast/toast.yml`.
- For other platforms, Toast follows the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html). The default location is `~/.config/toast/toast.yml` unless overridden by the `XDG_CONFIG_HOME` environment variable.

The configuration file has the following schema and defaults:

```yaml
docker_repo: toast         # Docker repository
read_local_cache: true    # Whether Toast should read from local cache
write_local_cache: true   # Whether Toast should write to local cache
read_remote_cache: false  # Whether Toast should read from remote cache
write_remote_cache: false # Whether Toast should write to remote cache
```

Each of these options can be overridden via command-line options (see [below](#command-line-options)).

A typical configuration for a continuous integration (CI) environment will enable all forms of caching, whereas for local development you may want to set `write_remote_cache: false` to avoid waiting for remote cache writes. See [`.travis.yml`](https://github.com/stepchowfun/toast/blob/master/.travis.yml) for a complete example of how to use Toast in a CI environment.

## Command-line options

By default, Toast looks for a toastfile called `toast.yml` in the working directory, then in the parent directory, and so on. Any paths in the toastfile are relative to where the toastfile lives, not the working directory. This means you can run Toast from anywhere in your project and get the same results.

Run `toast` with no arguments to execute the default task, or all the tasks if the toastfile doesn't define a default. You can also execute specific tasks and their dependencies:

```sh
toast task1 task2 task3…
```

Here are all the supported command-line options:

```
USAGE:
    toast [OPTIONS] [TASKS]...

OPTIONS:
    -c, --config-file <PATH>
            Sets the path of the config file

    -f, --file <PATH>
            Sets the path to the toastfile

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

If you are running macOS or a GNU-based Linux on an x86-64 CPU, you can install Toast with this command:

```sh
curl https://raw.githubusercontent.com/stepchowfun/toast/master/install.sh -LSfs | sh
```

The same command can be used again to update Toast to the latest version.

**NOTE:** Piping `curl` to `sh` is dangerous since the server might be compromised. If you're concerned about this, you can download the installation script and inspect it or choose one of the other installation methods.

#### Customizing the installation

The installation script supports the following environment variables:

- `VERSION=x.y.z` (defaults to the latest version)
- `PREFIX=/path/to/install` (defaults to `/usr/local/bin`)

For example, the following will install Toast into the working directory:

```sh
curl https://raw.githubusercontent.com/stepchowfun/toast/master/install.sh -LSfs | PREFIX=. sh
```

### Manual installation

The [releases page](https://github.com/stepchowfun/toast/releases) has precompiled binaries for macOS or Linux systems running on an x86-64 CPU. You can download one of them and place it in a directory listed in your [`PATH`](https://en.wikipedia.org/wiki/PATH_\(variable\)).

### Installation with Cargo

If you have [Cargo](https://doc.rust-lang.org/cargo/), you can install Toast as follows:

```sh
cargo install toast
```

You can run that command with `--force` to update an existing installation.

## Requirements

- Toast requires [Docker Engine](https://www.docker.com/products/docker-engine) 17.03.0 or later.
- Only Linux-based Docker images are supported. Toast can run on any platform capable of running such images, e.g., macOS with [Docker Desktop](https://www.docker.com/products/docker-desktop).

## Acknowledgements

Toast was inspired by an in-house tool used at Airbnb for CI jobs. The design was heavily influenced by the lessons I learned working on that tool and building out Airbnb's CI system with the fabulous CI Infrastructure Team.

Special thanks to Julia Wang ([@juliahw](https://github.com/juliahw)) for valuable early feedback. Thanks to Julia and Mark Tai [@marktai](https://github.com/marktai) for coming up with the name *Toast*.
