# Contribuindo com o LENKO

Obrigado pelo interesse! Abra uma *issue* descrevendo a ideia antes de começar algo grande.

## Rodando localmente

Pré-requisitos: [Rust](https://rustup.rs), Node.js 18+ e as [dependências do Tauri para o seu sistema](https://v2.tauri.app/start/prerequisites/).

```bash
npm install
npm run dev              # a primeira compilação demora alguns minutos
```

Com o LENKO aberto, em outro terminal:

```bash
npm run hooks:install    # conecta ao Claude Code (faz backup do settings.json)
npm run test:done        # ela deve pular e dizer "Terminei!"
npm run test:aprovacao   # simula um pedido de aprovação; Enter "aprova"
```

Reinicie o Claude Code e pronto. Para desconectar: `npm run hooks:uninstall`.

> Para criar um mascote, veja [docs/criar-mascote.md](docs/criar-mascote.md) e empacote com `npm run mascote:empacotar -- <pasta>`.

> Para mexer só no visual, rode `npm run preview`, abra http://localhost:5173 e use as teclas **1–4** para simular os estados, **5** para a pergunta de conexão, **6** para o aviso de atualização e **7** para simular a volta ao terminal.

## Testes

```bash
cd src-tauri
cargo test               # hooks, modo --notify e validação de mascotes
```

## Estrutura

```
lenko/
├── src/index.html              mascote: sprite, estados, balão e som
├── src-tauri/src/main.rs       janela transparente + servidor local na porta 7777
├── src-tauri/src/notify.rs     modo --notify: o próprio executável serve de adaptador
├── src-tauri/src/hooks.rs      conecta e desconecta cada ferramenta de IA
├── src-tauri/src/skin.rs       formato .mascote: validação e instalação
├── src-tauri/src/focus.rs      percebe quando você volta ao terminal (Windows)
├── src-tauri/src/tray.rs       ícone e menu na bandeja do sistema
├── src-tauri/src/config.rs     preferências salvas (conexão, som, tamanho)
├── src-tauri/src/update.rs     atualização automática pelas Releases
├── src-tauri/tauri.conf.json   nome, identificador e janela do app
├── scripts/notify.js           adaptador genérico (hook → evento)
├── src/mascotes/preguica/      a preguiça no formato .mascote
├── scripts/install-hooks.js    conecta e desconecta o Claude Code
├── scripts/gerar-preguica.js   desenha a preguiça (pixel art como texto)
├── docs/                       página de download (GitHub Pages), GIF e imagem dos estados
└── .github/workflows/          testes a cada push
```

## Versões

Os instaladores e as atualizações são publicados pelo mantenedor na [página de Releases](https://github.com/DzNx22/lenko/releases). Cada push e pull request passa pelo workflow `ci`, que roda os testes no Windows e no Linux.

## Regras

- Textos, comentários e mensagens de commit em português do Brasil. Commits curtos e no imperativo: `adiciona adaptador do Codex`.
- Nenhum adaptador pode travar a ferramenta de IA: em caso de erro, sai com código 0 e com timeout curto.
- Pixel art sem suavização (`image-rendering: pixelated`, escalas inteiras) e respeito a `prefers-reduced-motion`.
- Só personagens originais. Nada de personagens de franquias, mascotes de marcas ou versões "inspiradas" reconhecíveis.
- O nome LENKO e a arte da preguiça não fazem parte da licença MIT; veja [TRADEMARKS.md](TRADEMARKS.md).
