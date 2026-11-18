#!/usr/bin/env node
// Empacota uma pasta de mascote num arquivo .mascote (zip), pronto para arrastar até o LENKO.
// Uso: npm run mascote:empacotar -- <pasta> [saida.mascote]
// A validação completa acontece no app ao instalar (src-tauri/src/skin.rs); aqui só
// conferimos o básico para o criador descobrir cedo o que está faltando.
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');

const [dir, outArg] = process.argv.slice(2);
if (!dir) {
  console.error('Uso: npm run mascote:empacotar -- <pasta> [saida.mascote]');
  process.exit(1);
}

// Mesma lista de src-tauri/src/skin.rs
const REQUIRED = ['manifest.json', 'sprites.png', 'prop.png'];
const OPTIONAL = /^(preview\.png|sounds\/(done|waiting)\.(wav|ogg|mp3))$/;

function walk(base, rel = '') {
  return fs.readdirSync(path.join(base, rel), { withFileTypes: true }).flatMap(e => {
    const name = rel ? `${rel}/${e.name}` : e.name;
    return e.isDirectory() ? walk(base, name) : [name];
  });
}

const files = walk(dir).sort();
const extra = files.filter(f => !REQUIRED.includes(f) && !OPTIONAL.test(f));
const missing = REQUIRED.filter(f => !files.includes(f));
if (extra.length || missing.length) {
  if (missing.length) console.error(`Faltando: ${missing.join(', ')}`);
  if (extra.length) console.error(`Arquivos não permitidos: ${extra.join(', ')}`);
  process.exit(1);
}

let manifest;
try { manifest = JSON.parse(fs.readFileSync(path.join(dir, 'manifest.json'), 'utf8')); } catch (e) {
  console.error(`manifest.json com erro: ${e.message}`);
  process.exit(1);
}
const out = outArg || `${manifest.id || path.basename(path.resolve(dir))}.mascote`;

/* ---------- zip mínimo (deflate), só com a biblioteca padrão ---------- */
const locals = [], centrals = [];
let offset = 0;
for (const name of files) {
  const data = fs.readFileSync(path.join(dir, name));
  const packed = zlib.deflateRawSync(data, { level: 9 });
  const crc = zlib.crc32(data);
  const nameBuf = Buffer.from(name, 'utf8');

  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(20, 4);           // versão necessária
  local.writeUInt16LE(0x0800, 6);       // nomes em UTF-8
  local.writeUInt16LE(8, 8);            // deflate
  local.writeUInt16LE(0, 10);           // hora
  local.writeUInt16LE(0x21, 12);        // data (1980-01-01): pacote reproduzível
  local.writeUInt32LE(crc, 14);
  local.writeUInt32LE(packed.length, 18);
  local.writeUInt32LE(data.length, 22);
  local.writeUInt16LE(nameBuf.length, 26);
  locals.push(local, nameBuf, packed);

  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0);
  central.writeUInt16LE(20, 4);
  central.writeUInt16LE(20, 6);
  central.writeUInt16LE(0x0800, 8);
  central.writeUInt16LE(8, 10);
  central.writeUInt16LE(0, 12);
  central.writeUInt16LE(0x21, 14);
  central.writeUInt32LE(crc, 16);
  central.writeUInt32LE(packed.length, 20);
  central.writeUInt32LE(data.length, 24);
  central.writeUInt16LE(nameBuf.length, 28);
  central.writeUInt32LE(offset, 42);
  centrals.push(central, nameBuf);

  offset += 30 + nameBuf.length + packed.length;
}
const centralSize = centrals.reduce((n, b) => n + b.length, 0);
const end = Buffer.alloc(22);
end.writeUInt32LE(0x06054b50, 0);
end.writeUInt16LE(files.length, 8);
end.writeUInt16LE(files.length, 10);
end.writeUInt32LE(centralSize, 12);
end.writeUInt32LE(offset, 16);

const zip = Buffer.concat([...locals, ...centrals, end]);
if (zip.length > 2 * 1024 * 1024) {
  console.error(`O pacote ficou com ${(zip.length / 1048576).toFixed(1)} MB; o máximo é 2 MB.`);
  process.exit(1);
}
fs.writeFileSync(out, zip);
console.log(`${out} criado (${files.length} arquivos, ${(zip.length / 1024).toFixed(1)} KB)`);
