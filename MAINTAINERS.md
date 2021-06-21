# Maintainers

This document describes some instructions for maintainers. Other contributors and users need not be concerned with this material.

### GitHub instructions

When setting up the repository on GitHub, configure the following settings:

- Under `Secrets`, add the following repository secrets with appropriate values:
  - `CRATES_IO_TOKEN`
  - `DOCKER_PASSWORD`
- Under `Branches`, add a branch protection rule for the `main` branch.
  - Enable `Require status checks to pass before merging`.
    - Enable `Require branches to be up to date before merging`.
    - Add the following status checks:
      - `Build for Linux`
      - `Build for Windows`
      - `Build for macOS`
      - `Install on Ubuntu`
      - `Install on macOS`
      - `Publish a release if applicable`
  - Enable `Include administrators`.
- Under `Options`, enable `Automatically delete head branches`.

The GitHub workflow will fail initially because the jobs which test the installer script will not find any release to download. You'll need to bootstrap a release by temporarily removing those jobs or changing them to no-ops. Be aware that the `create-release` job is configured to only run on the `main` branch, so you may also need to temporarily change that depending on which branch you're working on.

### Release instructions

Releasing a new version is a two-step process:

1. Bump the version in `Cargo.toml`, run `cargo build` to update `Cargo.lock`, and update `CHANGELOG.md` with information about the new version. Ship those changes as a single commit.
2. Once the GitHub workflow has finished on the `main` branch, update the version in `install.sh` to point to the new release.
