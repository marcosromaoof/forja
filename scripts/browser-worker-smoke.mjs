import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';

const worker = spawn(process.execPath, ['apps/browser-worker/index.mjs'], {
  cwd: process.cwd(),
  stdio: ['pipe', 'pipe', 'inherit'],
});
const lines = createInterface({input: worker.stdout, crlfDelay: Infinity});
const pending = new Map();
let nextId = 0;

lines.on('line', line => {
  const response = JSON.parse(line);
  const waiter = pending.get(response.id);
  if (!waiter) return;
  pending.delete(response.id);
  response.ok ? waiter.resolve(response.result) : waiter.reject(new Error(response.error));
});

function request(command, args = {}) {
  const id = String(++nextId);
  worker.stdin.write(`${JSON.stringify({id, command, args})}\n`);
  return new Promise((resolve, reject) => pending.set(id, {resolve, reject}));
}

try {
  const origin = process.argv[2] ?? 'http://127.0.0.1:8765';
  await request('start', {origins: [origin]});
  const navigation = await request('navigate', {url: `${origin}/`});
  const snapshot = await request('snapshot');
  const screenshot = await request('screenshot', {full_page: false});
  let blocked = false;
  try {
    await request('navigate', {url: 'https://example.com/'});
  } catch (error) {
    blocked = /Origem não autorizada/.test(String(error));
  }
  if (!snapshot.aria || screenshot.bytes < 1000 || !blocked) {
    throw new Error('O worker não comprovou snapshot, screenshot e bloqueio por origem');
  }
  await request('close');
  console.log(JSON.stringify({
    ok: true,
    url: navigation.url,
    title: navigation.title,
    ariaCharacters: snapshot.aria.length,
    screenshotBytes: screenshot.bytes,
    unauthorizedOriginBlocked: blocked,
  }, null, 2));
} finally {
  worker.stdin.end();
}
