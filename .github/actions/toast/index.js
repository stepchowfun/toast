const childProcess = require('child_process');
const core = require('@actions/core');
const path = require('path');

// The Toast version
const toastVersion = '0.32.0';

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

// Install Toast.
childProcess.execSync(
  'curl https://raw.githubusercontent.com/stepchowfun/toast/master/install.sh -LSfs | ' +
    `PREFIX="${toastPrefix}" VERSION="${toastVersion}" sh`,
  { stdio: 'inherit' },
);

// Construct the command-line arguments for Toast.
const taskArgs = tasks === null ? [] : tasks;
const fileArgs = file === null ? [] : ['--file', file];
const repositoryArgs = repo === null ? [] : ['--repo', repo];
const readRemoteCacheArgs = repo === null ? [] : ['--read-remote-cache', 'true'];
const writeRemoteCacheArgs = writeRemoteCache ? ['--write-remote-cache', 'true'] : [];

// Run Toast.
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
