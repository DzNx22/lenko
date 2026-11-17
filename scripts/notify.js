#!/usr/bin/env node
// Adaptador genérico: recebe o JSON do hook pelo stdin e avisa o LENKO.
// Uso: node notify.js <working|done|waiting> [nome-da-ferramenta]
// Nunca falha: se o LENKO estiver fechado, sai em silêncio para não travar a IA.
const http = require('http');
const path = require('path');

const requested = process.argv[2] || 'done';
const tool = process.argv[3] || 'claude-code';
let raw = '';
let sent = false;

function send() {
  if (sent) return;
  sent = true;

  let hook = {};
  try { hook = JSON.parse(raw || '{}'); } catch {}
  const cwd = hook.cwd || process.cwd();
  const { type, message } = classify(requested, hook);

  const body = JSON.stringify({ tool, type, project: path.basename(cwd), message });

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

// O hook Notification do Claude Code dispara em dois casos bem diferentes:
// pedido de aprovação (precisa do dev) e "ocioso há 60s" (a tarefa já terminou).
// Também chega com texto em inglês, então traduzimos aqui.
function classify(type, hook) {
  if (type !== 'waiting') return { type };
  const kind = hook.notification_type || '';
  const text = String(hook.message || '');

  if (kind === 'idle_prompt' || /waiting for your input/i.test(text)) {
    return { type: 'done' }; // terminou e o dev ainda não voltou
  }
  const perm = text.match(/permission to use (.+)$/i);
  if (perm) return { type, message: `Posso usar ${perm[1].trim()}?` };
  if (kind === 'permission_prompt' || /permission/i.test(text)) {
    return { type, message: 'Preciso da sua aprovação.' };
  }
  return { type, message: 'Preciso de você aqui.' };
}

if (process.stdin.isTTY) {
  send(); // rodado à mão no terminal, sem stdin
} else {
  process.stdin.on('data', c => (raw += c));
  process.stdin.on('end', send);
  setTimeout(send, 500); // garantia caso o stdin nunca feche
}
