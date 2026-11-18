#!/usr/bin/env node
// Gera a preguiça padrão no formato de mascote (src/mascotes/preguica/).
// O desenho fica aqui como grade de caracteres: cada letra é uma cor da paleta.
// Uso: npm run mascote:preguica
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');

const OUT = path.join(__dirname, '..', 'src', 'mascotes', 'preguica');

const PAL = {
  O: '#3B2A1E', B: '#8A6F55', L: '#E6D3A8', E: '#4A3426', W: '#FFFFFF',
  N: '#2A1D14', M: '#5A3A28', C: '#F2E6C9', T: '#6B4A2E', t: '#4F341F', g: '#6E9E4F',
};

const BRANCH = [
  '................................',
  '....gg.........g.........gg.....',
  'TTTTTTTCCCTTTTTTTTTTTTCCCTTTTTTT',
  'tttttttCCCttttttttttttCCCttttttt',
];

const face = mid => '.......OBB' + mid + 'BBO.......';
const EYES = {
  open:   [face('LLEEELLEEELL'), face('LEEWELLEWEEL')],
  closed: [face('LLEEELLEEELL'), face('LEEEELLEEEEL')],
};
const MOUTH = {
  flat:  [face('LLLLLLLLLLLL'), face('LLLLMMMMLLLL')],
  smile: [face('LLLLMLLMLLLL'), face('LLLLLMMLLLLL')],
  open:  [face('LLLLLMMLLLLL'), face('LLLLLMMLLLLL')],
};
function body(eyes, mouth) {
  return [
    '.......OBBO..........OBBO.......',
    '.......OBBO..........OBBO.......',
    '.......OBBO.OOOOOOOO.OBBO.......',
    '.......OBBOOBBBBBBBBOOBBO.......',
    '.......OBBBBBLLLLLLBBBBBO.......',
    '.......OBBBLLLLLLLLLLBBBO.......',
    ...EYES[eyes],
    face('LEELLLLLLEEL'),
    face('LLLLLNNLLLLL'),
    ...MOUTH[mouth],
    '.......OBBBLLLLLLLLLLBBBO.......',
    '.......OBBBBBLLLLLLBBBBBO.......',
    '........OBBBBBBBBBBBBBBO........',
    '........OBBBBBBBBBBBBBBO........',
    '........OBBBBBBBBBBBBBBO........',
    '........OBBBBBBBBBBBBBBO........',
    '.........OBBBBBBBBBBBBO.........',
    '.........OBBBBBBBBBBBBO.........',
    '..........OBBO....OBBO..........',
    '..........OBBO....OBBO..........',
    '..........CCCO....OCCC..........',
  ];
}

// Quadros da spritesheet, da esquerda para a direita
const FRAMES = [
  body('open', 'flat'),    // 0 trabalhando
  body('closed', 'flat'),  // 1 dormindo / piscando
  body('open', 'smile'),   // 2 terminou
  body('closed', 'smile'), // 3 terminou, piscando
  body('open', 'open'),    // 4 chamando
  body('closed', 'open'),  // 5 chamando, piscando
];

const MANIFEST = {
  formatVersion: 1,
  id: 'preguica',
  name: 'Preguiça',
  author: 'Denzell',
  version: '1.0.0',
  license: 'MIT',
  description: 'Pendurada no galho, acorda quando a IA precisa de você. O mascote padrão do LENKO.',
  frame: { width: 32, height: FRAMES[0].length },
  states: {
    idle:    { frames: [1] },
    working: { frames: [0], blink: 1 },
    done:    { frames: [2], blink: 3 },
    waiting: { frames: [4], blink: 5 },
  },
};

/* ---------- PNG RGBA mínimo ---------- */
function png(width, height, rgba) {
  const chunk = (type, data) => {
    const head = Buffer.alloc(8);
    head.writeUInt32BE(data.length, 0);
    head.write(type, 4, 'ascii');
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(zlib.crc32(Buffer.concat([head.subarray(4), data])));
    return Buffer.concat([head, data, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; ihdr[9] = 6; // 8 bits, RGBA
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (width * 4 + 1)] = 0; // sem filtro
    rgba.copy(raw, y * (width * 4 + 1) + 1, y * width * 4, (y + 1) * width * 4);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// Junta grades lado a lado numa imagem só
function sheet(grids) {
  const w = grids[0][0].length, h = grids[0].length;
  const width = w * grids.length;
  const rgba = Buffer.alloc(width * h * 4);
  grids.forEach((rows, i) => rows.forEach((row, y) => [...row].forEach((c, x) => {
    if (c === '.') return;
    const hex = PAL[c];
    const o = (y * width + i * w + x) * 4;
    rgba[o] = parseInt(hex.slice(1, 3), 16);
    rgba[o + 1] = parseInt(hex.slice(3, 5), 16);
    rgba[o + 2] = parseInt(hex.slice(5, 7), 16);
    rgba[o + 3] = 255;
  })));
  return png(width, h, rgba);
}

fs.mkdirSync(OUT, { recursive: true });
fs.writeFileSync(path.join(OUT, 'manifest.json'), JSON.stringify(MANIFEST, null, 2) + '\n');
fs.writeFileSync(path.join(OUT, 'sprites.png'), sheet(FRAMES));
fs.writeFileSync(path.join(OUT, 'prop.png'), sheet([BRANCH]));
console.log(`Preguiça gerada em ${path.relative(process.cwd(), OUT)}`);
