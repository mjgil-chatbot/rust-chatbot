#!/usr/bin/env node

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

function expandHome(input) {
  if (!input) {
    return input;
  }
  if (input === '~') {
    return os.homedir();
  }
  if (input.startsWith('~/')) {
    return path.join(os.homedir(), input.slice(2));
  }
  return input;
}

function parseArgs(argv) {
  const options = {
    repo: '~/git/others/gemini-cli',
    storageHome: '~/.rust-chatbot',
    noBrowser: false,
    skipInstall: false,
    dryRun: false,
    help: false,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') {
      options.help = true;
    } else if (arg === '--no-browser') {
      options.noBrowser = true;
    } else if (arg === '--skip-install') {
      options.skipInstall = true;
    } else if (arg === '--dry-run') {
      options.dryRun = true;
    } else if (arg === '--repo') {
      i += 1;
      options.repo = argv[i];
    } else if (arg === '--storage-home') {
      i += 1;
      options.storageHome = argv[i];
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }

  return {
    ...options,
    repo: path.resolve(expandHome(options.repo)),
    storageHome: path.resolve(expandHome(options.storageHome)),
  };
}

function printHelp() {
  console.log(`Usage: node scripts/gemini-cli-reauth.js [options]

Build gemini-cli if needed, clear the target OAuth cache, and launch a fresh
Google OAuth login using file-backed storage in the chosen home directory.

Options:
  --repo <path>          gemini-cli checkout
                         default: ~/git/others/gemini-cli
  --storage-home <path>  base home used for GEMINI_CLI_HOME
                         default: ~/.rust-chatbot
  --no-browser           request manual code-entry auth flow
  --skip-install         do not run pnpm install when node_modules is missing
  --dry-run              print actions without executing them
  -h, --help             show this help

Examples:
  node scripts/gemini-cli-reauth.js
  node scripts/gemini-cli-reauth.js --no-browser
  node scripts/gemini-cli-reauth.js --storage-home ~/.rust-chatbot
`);
}

function runOrThrow(command, args, options = {}) {
  const rendered = [command, ...args].join(' ');
  console.log(`$ ${rendered}`);
  if (options.dryRun) {
    return;
  }
  const result = spawnSync(command, args, {
    cwd: options.cwd,
    env: options.env || process.env,
    stdio: 'inherit',
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`Command failed with exit code ${result.status}: ${rendered}`);
  }
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printHelp();
    return;
  }

  const repoPackageJson = path.join(options.repo, 'package.json');
  if (!fs.existsSync(repoPackageJson)) {
    throw new Error(`gemini-cli repo not found at ${options.repo}`);
  }

  const nodeModulesDir = path.join(options.repo, 'node_modules');
  const oauthFile = path.join(options.storageHome, '.gemini', 'oauth_creds.json');
  const geminiEnv = {
    ...process.env,
    GEMINI_CLI_HOME: options.storageHome,
    GEMINI_FORCE_FILE_STORAGE: 'true',
  };
  if (options.noBrowser) {
    geminiEnv.NO_BROWSER = 'true';
  }

  console.log(`gemini-cli repo: ${options.repo}`);
  console.log(`GEMINI_CLI_HOME: ${options.storageHome}`);
  console.log(`OAuth file: ${oauthFile}`);

  if (!fs.existsSync(nodeModulesDir)) {
    if (options.skipInstall) {
      throw new Error(
        `node_modules is missing in ${options.repo}; rerun without --skip-install`,
      );
    }
    runOrThrow('pnpm', ['install'], { cwd: options.repo, dryRun: options.dryRun });
  }

  runOrThrow('pnpm', ['build'], { cwd: options.repo, dryRun: options.dryRun });

  console.log(`$ rm -f ${oauthFile}`);
  if (!options.dryRun) {
    fs.rmSync(oauthFile, { force: true });
  }

  runOrThrow('node', ['bundle/gemini.js'], {
    cwd: options.repo,
    env: geminiEnv,
    dryRun: options.dryRun,
  });
}

try {
  main();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
