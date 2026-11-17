#!/usr/bin/env node
// Adaptador genérico: recebe o JSON do hook pelo stdin e avisa a Preguiça.
// Uso: node notify.js <working|done|waiting> [nome-da-ferramenta]
// Nunca falha: se a Preguiça estiver fechada, sai em silêncio para não travar a IA.
const http = require('http');
const path = require('path');

const type = process.argv[2] || 'done';
const tool = process.argv[3] || 'claude-code';
let raw = '';
let sent = false;

function send() {
  if (sent) return;
  sent = true;

  let hook = {};
  try { hook = JSON.parse(raw || '{}'); } catch {}
  const cwd = hook.cwd || process.cwd();

  const body = JSON.stringify({
    tool,
    type,
    project: path.basename(cwd),
    message: type === 'waiting' ? hook.message : undefined,
  });

  const req = http.request(
    { host: '127.0.0.1', port: 7777, path: '/event', method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) },
      timeout: 800 },
    res => { res.resume(); res.on('end', () => process.exit(0)); }
  );
  req.on('error', () => process.exit(0));
  req.on('timeout', () => { req.destroy(); process.exit(0); });
  req.end(body);
}

if (process.stdin.isTTY) {
  send(); // rodado à mão no terminal, sem stdin
} else {
  process.stdin.on('data', c => (raw += c));
  process.stdin.on('end', send);
  setTimeout(send, 500); // garantia caso o stdin nunca feche
}
