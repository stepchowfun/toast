const childProcess = require('child_process');
const core = require('@actions/core');
const path = require('path');
const { v4: uuidv4 } = require('uuid');

// Read the action inputs.
const tasksInput = core.getInput('tasks').trim();
const fileInput = core.getInput('file').trim();
const repoInput = core.getInput('repo').trim();
const writeRemoteCacheInput = core.getInput('write_remote_cache').trim();

// Parse the action inputs.
const tasks = tasksInput === '' ? null : tasksInput.split(/\s+/);
const file = fileInput === '' ? null : fileInput;
const repo = repoInput === '' ? null : repoInput;
const writeRemoteCache = writeRemoteCacheInput == 'true';

// Where to install Toast.
const toastPrefix = process.env.HOME;

// Before doing anything, disable command workflow processing. This is to prevent an injection
// attack with GitHub Actions. For more information, see:
// - https://bugs.chromium.org/p/project-zero/issues/detail?id=2070&can=2&q=&colspec=ID%20Type%20Status%20Priority%20Milestone%20Owner%20Summary&cells=ids
// - https://github.blog/changelog/2020-10-01-github-actions-deprecating-set-env-and-add-path-commands/
// - https://github.com/actions/toolkit/security/advisories/GHSA-mfwh-5m23-j46w
const token = uuidv4();
console.log(`::stop-commands::${token}`);

// Install Toast.
childProcess.execSync(
  'curl https://raw.githubusercontent.com/stepchowfun/toast/main/install.sh -LSfs | ' +
    `PREFIX="${toastPrefix}" sh`,
  { stdio: 'inherit' },
);

// Construct the command-line arguments for Toast.
const taskArgs = tasks === null ? [] : tasks;
const fileArgs = file === null ? [] : ['--file', file];
const repositoryArgs = repo === null ? [] : ['--repo', repo];
const readRemoteCacheArgs = repo === null ? [] : ['--read-remote-cache', 'true'];
const writeRemoteCacheArgs = writeRemoteCache ? ['--write-remote-cache', 'true'] : [];

// Run Toast.
try {
  childProcess.execFileSync(
    path.join(toastPrefix, 'toast'),
    fileArgs
      .concat(repositoryArgs)
      .concat(readRemoteCacheArgs)
      .concat(writeRemoteCacheArgs)
      .concat(taskArgs),
    {
      cwd: process.env.GITHUB_WORKSPACE,
      stdio: 'inherit',
    },
  );
} catch (_) {
  // Toast should print a helpful error message. There's no need to log anything more.
} finally {
  // To be a good citizen, we now re-enable command workflow processing.
  console.log(`::${token}::`);
}
