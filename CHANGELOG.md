# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.8.0] - 2019-05-15

### Fixed
- Fixed a bug that would cause input paths to be read-only to non-root users in the container.

## [0.7.0] - 2019-05-14

### Added
- Added helpful messages to the spinner animation.

### Changed
- Changed some log formatting to improve visual appeal.
- Improved the performance of tasks which aren't cacheable.
- Bake no longer respects filter files like `.gitignore`. Input paths are taken literally and match the behavior of output paths.

### Fixed
- Fixed a bug where Bake would try to copy an output file to a non-existent directory.
- Fixed a bug in which Bake would incorrectly delete existing local cache entries when local cache writes are disabled.

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
