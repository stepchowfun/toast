# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
