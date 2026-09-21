// Test-only MCP server. It never ships in the desktop installer.
const fs = require('node:fs');
const readline = require('node:readline');
const {spawn} = require('node:child_process');
const [version, directory, scenario] = process.argv.slice(2);
const log = (value) => fs.appendFileSync(directory + '/messages.jsonl', JSON.stringify(value) + '\n');
let initialized = false;
readline.createInterface({input: process.stdin}).on('line', (line) => {
  const req = JSON.parse(line);
  log(req);
  if (req.error) return;
  const modern = version === '2026-07-28';
  if (modern && req.params?._meta?.['io.modelcontextprotocol/protocolVersion'] !== version) process.exit(11);
  if (!modern && req.method === 'notifications/initialized') { initialized = true; return; }
  let result;
  switch (req.method) {
    case 'initialize':
      if (modern) process.exit(12);
      result = {protocolVersion: version, capabilities: {tools: {}}, serverInfo: {name: 'stdio-fixture', version: '1'}};
      break;
    case 'server/discover':
      if (!modern) process.exit(13);
      result = {capabilities: {tools: {}}, serverInfo: {name: 'stdio-fixture', version: '1'}};
      break;
    case 'tools/list':
      if (!modern && !initialized) process.exit(14);
      result = {tools: [{name: 'echo', inputSchema: {type: 'object', properties: {text: {type: 'string'}}, required: ['text'], additionalProperties: false}}]};
      break;
    case 'tools/call':
      if (scenario === 'slow') {
        // The child belongs to the same Windows Job Object. Its heartbeat must stop on cancellation.
        const child = spawn(process.execPath, ['-e', "let n=0;setInterval(()=>require('node:fs').writeFileSync(process.argv[1],String(++n)),20)", directory + '/heartbeat'], {stdio: 'ignore'});
        child.unref();
        return;
      }
      if (scenario === 'malformed') { process.stdout.write('{broken\n'); return; }
      if (!modern) process.stdout.write(JSON.stringify({jsonrpc: '2.0', id: 900, method: 'sampling/createMessage', params: {messages: []}}) + '\n');
      result = {content: [{type: 'text', text: req.params.arguments.text}]};
      break;
    default: process.exit(15);
  }
  const buffer = Buffer.from(JSON.stringify({jsonrpc: '2.0', id: req.id, result}) + '\n');
  // Split UTF-8 bytes and framing across separate writes.
  for (const byte of buffer) process.stdout.write(Buffer.from([byte]));
});
