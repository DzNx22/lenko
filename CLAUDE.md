# LENKO — guia para o Claude Code

Este arquivo é o contexto do projeto. Leia inteiro antes de mexer em qualquer coisa.

## O que é

LENKO é um mascote de desktop 8-bit que avisa quando uma IA de código termina uma tarefa ou precisa de aprovação. Começou com o Claude Code; o objetivo é atender qualquer ferramenta de IA de código, em qualquer IDE ou terminal. O mascote padrão é uma preguiça pendurada num galho.

**Origem do nome.** LENKO (ЛЕНКО) une duas palavras russas: *Lenivets* (ленивец), "preguiça", e *Kompanion* (компаньон), "companheiro". O nome representa um pequeno companheiro digital, de personalidade descontraída, que fica ao lado do usuário no desktop e aparece quando tem algo importante a comunicar. LENKO é a identidade de um universo de personagens; a preguiça é a primeira e o mascote padrão.

Em todo lugar técnico ou de busca, usar a forma latina (`lenko`, `LENKO`, `com.lenko.app`). ЛЕНКО aparece só como detalhe visual no README, sempre junto de LENKO. A marca `--preguica` dos hooks e o backup `settings.json.bak-preguica` continuam com o nome antigo de propósito: trocar deixaria entradas órfãs no `settings.json` de quem já instalou.

O projeto tem dois papéis ao mesmo tempo:

1. **Portfólio** do autor (Denzell, analista de sistemas e dados com foco em integração de sistemas). Código limpo, README claro e histórico de commits legível importam.
2. **Produto** que pode virar uma loja de mascotes (skins) no futuro.

Idioma do projeto: **português do Brasil** em textos de interface, README, comentários e mensagens de commit.

## Arquitetura atual (MVP)

```
Ferramenta de IA ──hook──► adaptador ──POST /event──► app Tauri (porta 7777) ──evento──► mascote (webview)
```

- `src-tauri/src/main.rs` — janela transparente, sem borda, sempre no topo, posicionada no canto inferior direito. Sobe um servidor HTTP (`tiny_http`) em `127.0.0.1:7777` e repassa cada evento para o webview como `ai-event`.
- `src/index.html` — o mascote inteiro: sprite desenhado como grade de caracteres (`PAL`, `BRANCH`, `body()`), estados, balão de fala, som 8-bit via Web Audio. Aberto direto no navegador, as teclas 1–4 simulam os estados.
- `scripts/notify.js` — adaptador genérico: lê o JSON do hook pelo stdin, extrai o nome do projeto do `cwd` e faz o POST. Nunca falha (se o app estiver fechado, sai com código 0 para não travar a IA).
- `scripts/install-hooks.js` — adiciona/remove os hooks em `~/.claude/settings.json`. Marca as entradas com `--preguica`, preserva hooks de terceiros, faz backup em `settings.json.bak-preguica`.

### Contrato de evento (não quebrar sem versionar)

```json
POST http://127.0.0.1:7777/event
{ "tool": "claude-code", "type": "working|done|waiting", "project": "nome-da-pasta", "message": "opcional" }
```

`GET /health` responde 200.

### Hooks usados no Claude Code

| Hook | Estado |
|---|---|
| `UserPromptSubmit` | `working` |
| `Stop` | `done` |
| `Notification` | `waiting` |

Limitação conhecida: na extensão nativa do Claude Code para VS Code, o hook `Notification` não dispara (issue anthropics/claude-code#59718). `Stop` funciona em todos os modos.

## Estado atual

- O frontend e os scripts Node foram testados.
- O Rust compila sem erros nem avisos no Windows 11 (Rust 1.100, MSVC, Tauri 2.12). Janela, servidor HTTP, `/health` e os três estados foram testados com o app aberto.

## Roadmap

Trabalhe uma fase por vez, em commits pequenos. Ao terminar cada item, marque aqui com `[x]`.

### Fase 0 — Fazer rodar
- [x] `npm install` e `npm run dev` compilando sem erros no Windows
- [x] Testar `npm run hooks:install` + `npm run test:done` com o app aberto
- [ ] Testar com uma sessão real do Claude Code

### Fase 1 — Instalador de um clique (prioridade: é o que permite divulgar)
Meta: o usuário baixa o instalador, instala e clica em "Conectar". Zero terminal, zero Node.
- [ ] **Modo `--notify` no executável**: `lenko.exe --notify done --tool claude-code` lê o stdin do hook, manda o POST para a instância aberta e sai em milissegundos, sem abrir janela. Substitui `scripts/notify.js` como adaptador padrão. Considerar `tauri-plugin-single-instance`.
- [ ] **Conectar ao Claude Code pela interface**: no primeiro uso, balão "Quer que eu me conecte ao Claude Code?" → o app edita `~/.claude/settings.json` com a mesma lógica do `install-hooks.js` (portar para Rust), apontando para o próprio executável.
- [ ] Opção de desconectar (remove só as entradas `--preguica`).
- [ ] Ícone na bandeja do sistema: mostrar/ocultar, silenciar, conectar/desconectar, sair.
- [ ] Iniciar com o sistema (`tauri-plugin-autostart`), desligado por padrão.
- [ ] **Atualização automática (obrigatória antes do primeiro lançamento público).** Quem instalar uma versão sem updater fica preso nela para sempre.
  - Adicionar `tauri-plugin-updater` e `tauri-plugin-process` (para reiniciar após atualizar).
  - Gerar o par de chaves com `npx tauri signer generate`. A chave pública vai em `tauri.conf.json` (`plugins.updater.pubkey`); a privada e a senha vão como secrets do GitHub (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). **Nunca commitar a chave privada.** Avisar o autor para guardar uma cópia fora do computador: perder a chave impede atualizar os apps já instalados.
  - Ativar `bundle.createUpdaterArtifacts` e passar os secrets e `includeUpdaterJson: true` no `tauri-action` do workflow de release, para publicar o `latest.json` junto dos instaladores.
  - Endpoint: `https://github.com/<usuario>/lenko/releases/latest/download/latest.json`.
  - UX: verificar ao abrir e a cada poucas horas; a preguiça mostra o balão "Tenho uma atualização!" e só instala com o clique do usuário.
- [ ] Assinatura de código: deixar o workflow pronto para receber o certificado do Windows via secrets quando o autor tiver um, sem quebrar o build enquanto não houver.
- [ ] Validar o workflow `.github/workflows/release.yml`: criar a tag `v0.1.0` e conferir os instaladores e o `latest.json` na página de Releases. Testar a atualização instalando a v0.1.0 e lançando uma v0.1.1.
- [ ] Atualizar o README: seção "Instalar" vira "baixe aqui"; a parte de desenvolvimento vai para `CONTRIBUTING.md`.

### Fase 2 — Sistema de skins (fundação da loja)
Princípio: depois desta fase, **personagens novos nunca exigem atualizar o app**. Funcionalidades (adaptadores, telas) chegam pelo updater; personagens chegam como pacotes baixados.
- [ ] Definir o formato de pacote `.mascote` (zip): `manifest.json` (id, nome, autor, versão, licença, `formatVersion`), sprites por estado (spritesheet PNG + frames), sons opcionais. Versionar o formato desde o início para pacotes antigos continuarem funcionando.
- [ ] Pacotes são dados, nunca código: sem JS/HTML executável dentro do `.mascote`. Validar o manifest e limitar o tamanho ao instalar.
- [ ] Guardar pacotes instalados na pasta de dados do app (`app_data_dir`), não na pasta do executável.
- [ ] Extrair a preguiça atual para esse formato e tratá-la como skin padrão.
- [ ] Instalar skin arrastando o arquivo para o mascote; trocar pela bandeja.
- [ ] Documentar o formato em `docs/criar-mascote.md` para criadores.

### Fase 3 — Mais ferramentas e atualização
- [ ] Adaptadores para outras ferramentas de IA de código (usar o mecanismo de hook/notify de cada uma; se não houver, um wrapper que roda o processo e avisa ao sair).
- [ ] Vários projetos ao mesmo tempo (fila de balões).
- [ ] Cada adaptador novo é lançado como versão nova do app (chega aos usuários pelo updater da Fase 1).

### Fase 4 — Loja (futuro, fora deste repositório)

Separação entre portfólio e negócio:
- **Este repositório (público, MIT):** o app LENKO e a preguiça. É o portfólio. Aqui fica só o *cliente* da loja: tela de catálogo, fluxo de compra, download e verificação de licença.
- **Repositório privado separado (`lenko-loja`):** servidor da loja, catálogo e pacotes pagos. Nada disso entra aqui, nem em exemplos, testes ou commits.

Arquitetura planejada:
- **Catálogo:** JSON servido por um endpoint próprio (Cloudflare Workers + R2), com id, nome, preço, prévia e versão de cada mascote. Adicionar um personagem = editar o catálogo e subir o pacote; não exige versão nova do app.
- **Compra:** o app abre o checkout no navegador; o webhook do provedor de pagamento chega ao servidor, que gera uma chave de licença vinculada ao e-mail do comprador.
- **Download:** pacotes pagos só saem por URL assinada e temporária, entregue mediante licença válida. Pacotes gratuitos baixam direto.
- **Pagamento:** Mercado Pago para o Brasil; Lemon Squeezy (merchant of record, cuida de impostos internacionais) para vendas fora.
- **No app:** guardar a licença localmente; funcionar offline com o que já foi baixado; nunca bloquear a preguiça padrão.
- Futuro: kit para criadores com divisão de receita.

**Regra fixa: só personagens originais.** Nada de Mario, Deadpool, Clippy, personagens de franquias, mascotes de marcas ou versões "inspiradas" reconhecíveis, nem em skins enviadas por terceiros. Se o autor pedir um personagem protegido, lembrar dessa regra e propor um personagem original com o mesmo tipo de comportamento (ex.: um encanador original com chave inglesa). As regras de envio da loja para criadores devem exigir declaração de autoria e permitir remoção.

## Convenções

- Commits em português, no imperativo e curtos: `adiciona modo --notify no executável`.
- **Autoria só do Denzell.** Nunca adicionar `Co-Authored-By`, "Generated with Claude Code" ou qualquer atribuição ao Claude em commits, PRs, tags ou releases. O Claude nunca faz `git push` nem abre PR: deixa os commits locais e o autor sobe.
- Nada de dependência pesada no frontend; continua HTML/CSS/JS puro até a Fase 2 exigir outra coisa.
- O mascote nunca pode travar a ferramenta de IA: qualquer adaptador sai com código 0 em caso de erro, com timeout curto.
- Pixel art sem suavização: `image-rendering: pixelated`, escalas inteiras.
- Respeitar `prefers-reduced-motion`.
- Sempre que mudar o visual, regenerar `docs/demo.gif` ou `docs/estados.png` se eles ficarem desatualizados.
