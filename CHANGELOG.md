# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.47.4] - 2023-06-02

### Added
- Toast supports a new platform: AArch64 musl Linux.

## [0.47.3] - 2023-05-23

### Added
- Toast supports a new platform: AArch64 GNU Linux.

## [0.47.2] - 2023-05-13

### Added
- Toast supports a new platform: macOS on Apple silicon.

## [0.47.1] - 2023-04-03

### Added
- Fixed a bug in `--force-image-pull` which would cause the flag to have no effect if the first task in the schedule is available in the local or remote cache. Also, the flag has been renamed to `--force-all`.

## [0.47.0] - 2023-04-03

### Added
- Added support for `--force-image-pull` (thanks to @raennor).

## [0.46.2] - 2023-03-08

### Fixed
- Various bugs related to paths on Windows have been fixed with the help of [`typed-path`](https://crates.io/crates/typed-path).

## [0.46.1] - 2023-03-02

### Fixed
- `mount_paths` now works on Windows.

## [0.46.0] - 2023-02-21

### Added
- Added support for `--output-dir`.

## [0.45.5] - 2022-07-02

### Fixed
- Fixed a bug which would cause Toast to fail if `-s`/`--shell` is provided and `-f`/`--file` is set to a path with a single component (e.g., `toast.yml` rather than `./toast.yml`).

## [0.45.4] - 2022-05-20

### Fixed
- Fixed a bug which would cause Toast to fail if the Docker image specifies a user other than `root`.

## [0.45.3] - 2022-02-09

### Fixed
- Fixed a bug which would cause `input_paths` which correspond to files to be incorporated into the cache key even if they are denied by `excluded_input_paths`.

## [0.45.2] - 2021-11-15

### Fixed
- Fixed a bug which could cause the cache key for a task to be computed incorrectly in rare situations.

## [0.45.1] - 2021-11-05

### Fixed
- Added a workaround for the Docker limitation that bind mount paths must be absolute on the host. This would cause Toast to fail when using relative `mount_paths` in combination with the `-f`/`--file` flag.

## [0.45.0] - 2021-09-23

### Changed
- `toast --list` now only includes tasks with a `description`. You can use this to control which tasks show up in the list.

## [0.44.0] - 2021-09-23

### Removed
- Removed the deprecated `--repo` flag. Please use `--docker-repo` instead.

## [0.43.0] - 2021-09-23

### Added
- Added support for `--docker-cli`. This enables users to switch from Docker to Podman, if they so choose.

### Changed
- `--repo` has been renamed to `--docker-repo`. `--repo` is still supported for now, but will be removed in a future release. Please use `--docker-repo` from now on. If you are using the `repo` option for the GitHub Action, please use `docker_repo` instead.

## [0.42.1] - 2021-08-26

### Fixed
- Removed some debugging output that was mistakenly included in a previous release.

## [0.42.0] - 2021-08-26

### Changed
- Toast no longer wastes time committing the container when a task is interrupted (e.g., by hitting CTRL+C).

## [0.41.0] - 2021-08-21

### Added
- Added support for `command_prefix`.
- Added 3 top-level fields to the toastfile format, which serve as defaults for the corresponding task-level fields: `location`, `user`, and `command_prefix`.

## [0.40.0] - 2021-08-17

### Added
- Added support for `extra_docker_arguments`, thanks to Xiangru Lian.

## [0.39.0] - 2021-08-08

### Added
- The `mount_paths` field now supports mounting a path on the host to a different path in the container, thanks to Xiangru Lian.

## [0.38.0] - 2021-07-09

### Changed
- Toast now supports `input_paths` which are invalid UTF-8 on Windows.

## [0.37.0] - 2021-07-07

### Added
- Added support for `--force TASK`.

## [0.36.0] - 2021-07-06

### Added
- Added support for `excluded_input_paths`.

### Fixed
- Fixed a bug that would cause Docker images to be deleted prematurely.

## [0.35.0] - 2021-07-06

### Changed
- When the final task being executed is not cached, Toast no longer commits the container to a temporary image and subsequently deletes it. This results in a significant performance boost in some situations.

## [0.34.0] - 2021-07-06

### Changed
- The default location for the configuration file on macOS has been changed from `$HOME/Library/Preferences/toast/toast.yml` to `$HOME/Library/Application Support/toast/toast.yml`. See [this discussion](https://github.com/dirs-dev/directories-rs/issues/62) for details.

## [0.33.0] - 2021-06-20

### Added
- Windows builds are now automated.

## [0.32.0] - 2020-10-10

### Added
- Thanks to Mackenzie Clark, Toast now may support Windows. No stability guarantees are made regarding Windows support, but contributions that enhance or fix Windows support are welcome.

## [0.31.0] - 2020-04-06

### Added
- Introduced `output_paths_on_failure`.

## [0.30.0] - 2019-12-31

### Fixed
- Toast now decides whether to print colored output based on whether STDERR is connected to a TTY. Previously, this decision was based on whether STDOUT is connected to a TTY, even though Toast mostly prints colored output to STDERR.
- When STDERR is not connected to a TTY, Toast now logs spinner messages. Previously, these messages were only displayed as part of a spinner animation when STDERR is connected to a TTY.

## [0.29.0] - 2019-07-11

### Changed
- `mount_paths` are now allowed to be absolute. This is to support mounting the Docker IPC socket (usually located at `/var/run/docker.sock`) in the container for running Docker commands in tasks.

## [0.28.0] - 2019-06-30

### Changed
- The container used for the `--shell` feature now uses the mount settings and ports from the last executed task, if any.

## [0.27.0] - 2019-06-09

### Fixed
- Fixed a bug that would cause Toast to crash if the first task had no environment variables, no input paths, and no command to run.

## [0.26.0] - 2019-06-09

### Fixed
- Fixed the way symlinks in `output_paths` are handled.

## [0.25.0] - 2019-06-09

### Fixed
- Fixed the way symlinks in `input_paths` are handled.
- Fix a bug that prevented the standard error output from being logged if a child process failed.

## [0.24.0] - 2019-06-02

### Changed
- This release contains only internal improvements to the robustness of the code. Upgrading to this new version will invalidate existing cached tasks.

## [0.23.0] - 2019-05-31

### Fixed
- Fixed a bug that would cause the `output_files` feature to fail if `/tmp` on the host is on a different mounted filesystem than the destination.

## [0.22.0] - 2019-05-29

### Added
- Added the `--list` option to list all the tasks in the toastfile.
- Added the `description` task field to be shown to the user when `--list` is used.

## [0.21.0] - 2019-05-26

### Added
- Added the `mount_paths` feature.

### Removed
- Removed the `watch` feature in favor of `mount_paths`.

## [0.20.0] - 2019-05-26

### Changed
- Toast now uses the environment, location, and user of the last task when running interactive shells for the `--shell` feature.
- Toast no longer depends on `/bin/sh` existing in the containers it creates.

## [0.19.0] - 2019-05-26

### Fixed
- Fix a bug that would cause failed tasks to be cached.

### Changed
- When using `--shell` with a failing task, the shell environment is now based on the container from when the task failed rather than the last succeeding task. This makes debugging failed tasks easier.

## [0.18.0] - 2019-05-22

### Fixed
- Fixed a bug that would cause images to be far larger than necessary.

## [0.17.0] - 2019-05-22

### Added
- Every release from this point forward will include checksums of the precompiled binaries.

## [0.16.0] - 2019-05-21

### Changed
- Renamed the project from *Bake* to *Toast*.

## [0.15.0] - 2019-05-20

### Changed
- Improved the performance of tasks that have no input paths and no command.
- Revamped the filesystem watching feature.
- Toast no longer depends on `chown` and `mkdir` in the container.
- Toast now renders a spinner animation when reading files from the host.
- Toast now requires that caching be disabled for tasks that expose ports or use filesystem watching.

### Fixed
- Fixed a bug that would cause the incorrect ports to be exposed in some situations.

## [0.14.0] - 2019-05-19

### Added
- Added support for filesystem watching.

### Changed
- Changed the cache key format.

## [0.13.0] - 2019-05-19

### Added
- Added support for port mapping.

### Changed
- The `--shell` option now applies even when there is a task failure.

## [0.12.0] - 2019-05-18

### Fixed
- Fixed an issue that caused Toast to not work with Linux distributions which aren't based on GNU.

### Changed
- Optimized the spinner animation rendering.

## [0.11.0] - 2019-05-18

### Fixed
- If the first task is a cache hit, Toast no longer pulls the base image.
- Fixed a bug in which Toast would read from cache for tasks that have `cache: false`.

## [0.10.0] - 2019-05-16

### Changed
- To match the way Toast runs tasks, the `--shell` feature no longer uses a login shell.

## [0.9.0] - 2019-05-16

### Fixed
- Fixed a minor bug in the way Toast handles child processes that are killed by signals.

## [0.8.0] - 2019-05-15

### Fixed
- Fixed a bug that would cause input paths to be read-only to non-root users in the container.

## [0.7.0] - 2019-05-14

### Added
- Added helpful messages to the spinner animation.

### Changed
- Changed some log formatting to improve visual appeal.
- Improved the performance of tasks which aren't cacheable.
- Toast no longer respects filter files like `.gitignore`. Input paths are taken literally and match the behavior of output paths.

### Fixed
- Fixed a bug where Toast would try to copy an output file to a non-existent directory.
- Fixed a bug in which Toast would incorrectly delete existing local cache entries when local cache writes are disabled.

## [0.6.0] - 2019-05-09

### Added
- Added support for `output_paths`.

### Changed
- Renamed `paths` to `input_paths`.

### Removed
- Removed support for fancy word wrapping because it interacted poorly with ANSI color escape sequences.

## [0.5.0] - 2019-05-08

### Added
- Added a spinner animation to entertain the user.
- Added more colors and improved some log messages.

### Fixed
- Fixed some minor issues with signal handling.

## [0.4.0] - 2019-05-07

### Added
- Added this changelog.
