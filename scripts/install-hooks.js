#!/usr/bin/env node
// Instala (ou remove com --uninstall) os hooks do LENKO no Claude Code.
// Mexe só nas entradas do LENKO e faz backup do settings.json antes.
const fs = require('fs');
const os = require('os');
const path = require('path');

const SETTINGS = path.join(os.homedir(), '.claude', 'settings.json');
const NOTIFY = path.resolve(__dirname, 'notify.js').replace(/\\/g, '/');
const MARK = '--preguica';
const EVENTS = {
  UserPromptSubmit: 'working', // você mandou um prompt → acordou
  Stop: 'done',                // terminou a resposta
  Notification: 'waiting',     // pediu permissão / está esperando você
};
const uninstall = process.argv.includes('--uninstall');

let settings = {};
if (fs.existsSync(SETTINGS)) {
  const text = fs.readFileSync(SETTINGS, 'utf8');
  try { settings = JSON.parse(text); } catch {
    console.error(`Não consegui ler ${SETTINGS} (JSON inválido). Corrija o arquivo e rode de novo.`);
    process.exit(1);
  }
  fs.writeFileSync(SETTINGS + '.bak-preguica', text);
} else {
  fs.mkdirSync(path.dirname(SETTINGS), { recursive: true });
}

settings.hooks ??= {};
const isOurs = h => typeof h.command === 'string' && h.command.includes(MARK);

for (const [event, type] of Object.entries(EVENTS)) {
  // Remove entradas antigas do LENKO (evita duplicar ao reinstalar)
  const groups = (settings.hooks[event] ?? [])
    .map(g => ({ ...g, hooks: (g.hooks ?? []).filter(h => !isOurs(h)) }))
    .filter(g => g.hooks.length > 0);

  if (!uninstall) {
    groups.push({ hooks: [{ type: 'command', command: `node "${NOTIFY}" ${type} claude-code ${MARK}` }] });
  }

  if (groups.length) settings.hooks[event] = groups;
  else delete settings.hooks[event];
}
if (!Object.keys(settings.hooks).length) delete settings.hooks;

fs.writeFileSync(SETTINGS, JSON.stringify(settings, null, 2) + '\n');
console.log(uninstall
  ? 'Hooks do LENKO removidos.'
  : `Hooks instalados em ${SETTINGS}\nBackup salvo em ${SETTINGS}.bak-preguica\nReinicie o Claude Code para valer.`);
