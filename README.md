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

Let's make the greeting more fun with a program called `cowsay`. We will add a task to install `cowsay` and make it a dependency for the `greet` task:

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

```yml
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
[INFO] The following tasks will be executed in the order given: `gcc`, `build`, and
       `greet`.
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

Tasks have the following schema:

```yaml
dependencies: <names of dependencies (default: [])>
cache: <whether a task can be cached (default: true)>
environment: <map from string to string or null (default: {})>
paths: <paths to copy into the container (default: [])>
location: <path in the container for running this task (default: /scratch)>
user: <name of the user in the container for running this task (default: root)>
command: <shell command to run in the container (default: null)>
```

The simplest valid bakefile has no tasks:

```yaml
image: alpine
tasks: {}
```

The [bakefile](https://github.com/stepchowfun/bake/blob/master/bake.yml) for Bake itself is a comprehensive example.

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

The inspiration for Bake came from a similar tool used at Airbnb for continuous integration jobs.
