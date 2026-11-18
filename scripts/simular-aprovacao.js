#!/usr/bin/env node
// Simula um pedido de aprovação do Claude Code, do jeito que os hooks mandam:
// Notification (waiting) → você aprova → PostToolUse (working) → Stop (done).
// Uso, com o LENKO aberto: npm run test:aprovacao
const http = require('http');
const readline = require('readline');

const project = 'teste-aprovacao';

function send(type, message) {
  const body = JSON.stringify({ tool: 'claude-code', type, project, message });
  return new Promise(resolve => {
    const req = http.request(
      { host: '127.0.0.1', port: 7777, path: '/event', method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) },
        timeout: 800 },
      res => { res.resume(); res.on('end', () => resolve(true)); });
    req.on('error', () => resolve(false));
    req.on('timeout', () => { req.destroy(); resolve(false); });
    req.end(body);
  });
}

const wait = ms => new Promise(r => setTimeout(r, ms));

(async () => {
  if (!await send('working')) {
    console.error('O LENKO não respondeu em 127.0.0.1:7777. Abra o app e tente de novo.');
    process.exit(1);
  }
  console.log('1/3  Claude trabalhando…');
  await wait(1500);

  await send('waiting', 'Posso usar Bash?');
  console.log('2/3  Pedido de aprovação enviado. A preguiça deve chamar você.');
  console.log('     Espere o lembrete (30 s) se quiser, e aperte Enter para "aprovar".');
  const rl = readline.createInterface({ input: process.stdin });
  await new Promise(r => rl.once('line', r));
  rl.close();

  await send('working');
  console.log('     Aprovado: a preguiça deve voltar para "…" e parar de insistir.');
  await wait(4000);

  await send('done');
  console.log('3/3  Tarefa concluída: "Terminei!".');
})();
