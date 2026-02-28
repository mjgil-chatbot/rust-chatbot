#!/usr/bin/env node

const crypto = require('node:crypto');
const fs = require('node:fs');
const http = require('node:http');
const net = require('node:net');
const os = require('node:os');
const path = require('node:path');
const readline = require('node:readline');
const { spawn } = require('node:child_process');

const OAUTH_CLIENT_ID =
  '681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com';
const OAUTH_CLIENT_SECRET = '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';
const OAUTH_SCOPE = [
  'https://www.googleapis.com/auth/cloud-platform',
  'https://www.googleapis.com/auth/generative-language',
  'https://www.googleapis.com/auth/userinfo.email',
  'https://www.googleapis.com/auth/userinfo.profile',
];
const TOKEN_ENDPOINT = 'https://oauth2.googleapis.com/token';
const USERINFO_ENDPOINT = 'https://www.googleapis.com/oauth2/v2/userinfo';
const SUCCESS_URL =
  'https://developers.google.com/gemini-code-assist/auth_success_gemini';
const FAILURE_URL =
  'https://developers.google.com/gemini-code-assist/auth_failure_gemini';

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
    storageHome: '~/.rust-chatbot',
    noBrowser: false,
    printOnly: false,
    help: false,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') {
      options.help = true;
    } else if (arg === '--no-browser') {
      options.noBrowser = true;
    } else if (arg === '--print-only') {
      options.printOnly = true;
    } else if (arg === '--storage-home') {
      i += 1;
      options.storageHome = argv[i];
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }

  options.storageHome = path.resolve(expandHome(options.storageHome));
  return options;
}

function printHelp() {
  console.log(`Usage: node scripts/gemini-oauth-reauth.js [options]

Mint a fresh Gemini OAuth token with the generative-language scope and write it
directly into the Gemini CLI storage layout under the selected home directory.

Options:
  --storage-home <path>  base home used for the .gemini directory
                         default: ~/.rust-chatbot
  --no-browser           use manual code-entry flow instead of localhost callback
  --print-only           print target paths and exit without authenticating
  -h, --help             show this help

Examples:
  node scripts/gemini-oauth-reauth.js
  node scripts/gemini-oauth-reauth.js --no-browser
  node scripts/gemini-oauth-reauth.js --storage-home ~/.rust-chatbot
`);
}

function base64UrlEncode(buffer) {
  return buffer
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/g, '');
}

function createPkcePair() {
  const verifier = base64UrlEncode(crypto.randomBytes(32));
  const challenge = base64UrlEncode(
    crypto.createHash('sha256').update(verifier).digest(),
  );
  return { verifier, challenge };
}

function buildAuthorizeUrl(params) {
  const url = new URL('https://accounts.google.com/o/oauth2/v2/auth');
  for (const [key, value] of Object.entries(params)) {
    url.searchParams.set(key, value);
  }
  return url.toString();
}

async function postToken(params) {
  const response = await fetch(TOKEN_ENDPOINT, {
    method: 'POST',
    headers: {
      'content-type': 'application/x-www-form-urlencoded',
    },
    body: new URLSearchParams(params),
  });

  const text = await response.text();
  let payload;
  try {
    payload = JSON.parse(text);
  } catch {
    payload = { raw: text };
  }

  if (!response.ok) {
    throw new Error(
      `Token exchange failed (${response.status}): ${payload.error || text}`,
    );
  }

  return payload;
}

function getAvailablePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(0, '127.0.0.1');
    server.on('listening', () => {
      const address = server.address();
      const port = address && typeof address === 'object' ? address.port : 0;
      server.close(() => resolve(port));
    });
    server.on('error', reject);
  });
}

function openBrowser(url) {
  const commands =
    process.platform === 'darwin'
      ? [['open', [url]]]
      : process.platform === 'win32'
        ? [['cmd', ['/c', 'start', '', url]]]
        : [['xdg-open', [url]]];

  for (const [command, args] of commands) {
    try {
      const child = spawn(command, args, {
        detached: true,
        stdio: 'ignore',
      });
      child.unref();
      return true;
    } catch {
      // try next
    }
  }
  return false;
}

async function prompt(question) {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });
  try {
    return await new Promise((resolve) => {
      rl.question(question, (answer) => resolve(answer.trim()));
    });
  } finally {
    rl.close();
  }
}

async function fetchUserInfo(accessToken) {
  const response = await fetch(USERINFO_ENDPOINT, {
    headers: {
      Authorization: `Bearer ${accessToken}`,
    },
  });

  if (!response.ok) {
    throw new Error(`userinfo request failed (${response.status})`);
  }

  return response.json();
}

async function readJsonOrDefault(filePath, defaultValue) {
  try {
    const content = await fs.promises.readFile(filePath, 'utf8');
    return JSON.parse(content);
  } catch {
    return defaultValue;
  }
}

async function writeJson(filePath, value, mode) {
  await fs.promises.mkdir(path.dirname(filePath), { recursive: true });
  await fs.promises.writeFile(filePath, JSON.stringify(value, null, 2), {
    mode,
  });
  if (mode) {
    try {
      await fs.promises.chmod(filePath, mode);
    } catch {
      // ignore chmod failure
    }
  }
}

async function updateGoogleAccounts(filePath, email) {
  const accounts = await readJsonOrDefault(filePath, { active: null, old: [] });
  if (accounts.active && accounts.active !== email && !accounts.old.includes(accounts.active)) {
    accounts.old.push(accounts.active);
  }
  accounts.old = (accounts.old || []).filter((item) => item !== email);
  accounts.active = email;
  await writeJson(filePath, accounts);
}

async function clearPreviousAuth(oauthCredsPath, accountsPath) {
  await fs.promises.rm(oauthCredsPath, { force: true });
  const accounts = await readJsonOrDefault(accountsPath, { active: null, old: [] });
  if (accounts.active && !accounts.old.includes(accounts.active)) {
    accounts.old.push(accounts.active);
  }
  accounts.active = null;
  await writeJson(accountsPath, accounts);
}

async function runBrowserFlow() {
  const port = await getAvailablePort();
  const redirectUri = `http://127.0.0.1:${port}/oauth2callback`;
  const state = crypto.randomBytes(32).toString('hex');
  const authUrl = buildAuthorizeUrl({
    client_id: OAUTH_CLIENT_ID,
    redirect_uri: redirectUri,
    response_type: 'code',
    access_type: 'offline',
    prompt: 'consent',
    scope: OAUTH_SCOPE.join(' '),
    state,
  });

  const code = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error('OAuth browser flow timed out after 5 minutes.'));
    }, 5 * 60 * 1000);

    const server = http.createServer((req, res) => {
      try {
        const requestUrl = new URL(req.url, 'http://127.0.0.1');
        if (requestUrl.pathname !== '/oauth2callback') {
          res.writeHead(302, { Location: FAILURE_URL });
          res.end();
          reject(new Error(`Unexpected callback path: ${requestUrl.pathname}`));
          return;
        }

        const returnedState = requestUrl.searchParams.get('state');
        const returnedCode = requestUrl.searchParams.get('code');
        const returnedError = requestUrl.searchParams.get('error');

        if (returnedError) {
          res.writeHead(302, { Location: FAILURE_URL });
          res.end();
          reject(new Error(`Google OAuth error: ${returnedError}`));
          return;
        }
        if (returnedState !== state) {
          res.writeHead(302, { Location: FAILURE_URL });
          res.end();
          reject(new Error('OAuth state mismatch.'));
          return;
        }
        if (!returnedCode) {
          res.writeHead(302, { Location: FAILURE_URL });
          res.end();
          reject(new Error('No authorization code received.'));
          return;
        }

        res.writeHead(302, { Location: SUCCESS_URL });
        res.end();
        resolve(returnedCode);
      } catch (error) {
        reject(error);
      } finally {
        clearTimeout(timeout);
        server.close();
      }
    });

    server.on('error', (error) => {
      clearTimeout(timeout);
      reject(error);
    });

    server.listen(port, '127.0.0.1', () => {
      console.log(`Open this URL if the browser does not launch:\n\n${authUrl}\n`);
      if (!openBrowser(authUrl)) {
        console.log('Browser launch failed. Open the URL above manually.');
      }
    });
  });

  return postToken({
    client_id: OAUTH_CLIENT_ID,
    client_secret: OAUTH_CLIENT_SECRET,
    code,
    grant_type: 'authorization_code',
    redirect_uri: redirectUri,
  });
}

async function runManualFlow() {
  const redirectUri = 'https://codeassist.google.com/authcode';
  const { verifier, challenge } = createPkcePair();
  const state = crypto.randomBytes(32).toString('hex');
  const authUrl = buildAuthorizeUrl({
    client_id: OAUTH_CLIENT_ID,
    redirect_uri: redirectUri,
    response_type: 'code',
    access_type: 'offline',
    prompt: 'consent',
    scope: OAUTH_SCOPE.join(' '),
    code_challenge_method: 'S256',
    code_challenge: challenge,
    state,
  });

  console.log(`Open this URL and complete sign-in:\n\n${authUrl}\n`);
  const code = await prompt('Paste the authorization code: ');
  if (!code) {
    throw new Error('Authorization code is required.');
  }

  return postToken({
    client_id: OAUTH_CLIENT_ID,
    client_secret: OAUTH_CLIENT_SECRET,
    code,
    code_verifier: verifier,
    grant_type: 'authorization_code',
    redirect_uri: redirectUri,
  });
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printHelp();
    return;
  }

  const geminiDir = path.join(options.storageHome, '.gemini');
  const oauthCredsPath = path.join(geminiDir, 'oauth_creds.json');
  const accountsPath = path.join(geminiDir, 'google_accounts.json');

  console.log(`Storage home: ${options.storageHome}`);
  console.log(`OAuth file: ${oauthCredsPath}`);
  console.log(`Accounts file: ${accountsPath}`);

  if (options.printOnly) {
    return;
  }

  await clearPreviousAuth(oauthCredsPath, accountsPath);

  const tokens = options.noBrowser
    ? await runManualFlow()
    : await runBrowserFlow();

  const credentials = {
    access_token: tokens.access_token,
    refresh_token: tokens.refresh_token,
    scope: tokens.scope,
    token_type: tokens.token_type,
    expiry_date: tokens.expires_in
      ? Date.now() + Number(tokens.expires_in) * 1000
      : undefined,
  };

  await writeJson(oauthCredsPath, credentials, 0o600);

  try {
    const userInfo = await fetchUserInfo(tokens.access_token);
    if (userInfo && userInfo.email) {
      await updateGoogleAccounts(accountsPath, userInfo.email);
      console.log(`Authenticated as: ${userInfo.email}`);
    }
  } catch (error) {
    console.warn(`Warning: failed to fetch user info: ${error.message}`);
  }

  console.log('\nStored fresh OAuth credentials.');
  console.log(`Scope: ${credentials.scope}`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
