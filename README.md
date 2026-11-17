<div align="center">

# LENKO

<sub>ЛЕНКО</sub>

**Um companheiro 8-bit que avisa quando a sua IA de código terminou o trabalho.**

O LENKO vive no canto da tela, e o mascote padrão é uma preguiça. Pare de ficar olhando para o terminal esperando o agente acabar. A preguiça acorda quando você manda um prompt, pula quando a tarefa termina e balança os braços quando a IA precisa da sua aprovação.

![A preguiça, mascote padrão do LENKO, reagindo a uma sessão no terminal](docs/demo.gif)

[![Licença MIT](https://img.shields.io/badge/licença-MIT-6E9E4F)](LICENSE)
![Tauri 2](https://img.shields.io/badge/Tauri-2-8A6F55)
![Rust](https://img.shields.io/badge/Rust-backend-3B2A1E)
![Plataformas](https://img.shields.io/badge/Windows%20·%20macOS%20·%20Linux-E6D3A8)

</div>

## Por que existe

Agentes de IA para código levam de segundos a vários minutos por tarefa. Nesse intervalo, ou você fica preso olhando o terminal, ou troca de janela e esquece de voltar. Quando a IA para no meio pedindo permissão, o tempo perdido é ainda maior.

Notificações do sistema resolvem parte do problema, mas somem e se misturam com todo o resto. O LENKO fica num canto da tela, discreto enquanto dorme, e impossível de ignorar quando tem algo para você.

## Por que LENKO?

LENKO (ЛЕНКО) une duas palavras russas: *Lenivets* (ленивец), "preguiça", e *Kompanion* (компаньон), "companheiro". É um pequeno companheiro digital, de personalidade descontraída, que fica ao seu lado no desktop e aparece quando tem algo importante a dizer.

LENKO é a identidade de um universo de personagens. A preguiça é a primeira e o mascote padrão.

## Estados

| | Quando | O que ela faz |
|---|---|---|
| 💤 **Dormindo** | Nada acontecendo | Balança devagar no galho |
| 👀 **Trabalhando** | Você enviou um prompt | Acorda e mostra o projeto |
| 🎉 **Terminei!** | A IA concluiu a tarefa | Pula, toca um som 8-bit e mostra o projeto |
| ✋ **Preciso de você** | A IA pediu aprovação | Treme e abre um balão vermelho com a pergunta |

![Os quatro estados](docs/estados.png)

## Instalar

1. Baixe o instalador do seu sistema na [página de Releases](https://github.com/DzNx22/lenko/releases/latest):
   - **Windows:** `LENKO_x.y.z_x64-setup.exe`
   - **macOS:** `.dmg` (`aarch64` para Apple Silicon, `x64` para Intel)
   - **Linux:** `.AppImage` ou `.deb`
2. Instale e abra. A preguiça aparece no canto inferior direito da tela.
3. No balão "Quer que eu me conecte ao Claude Code?", clique em **Conectar** e reinicie o Claude Code.

Pronto: sem terminal e sem Node. Para desconectar depois, use o menu da bandeja.

> No Windows, enquanto o instalador não tiver assinatura digital, o SmartScreen pode mostrar "O Windows protegeu o computador". Clique em **Mais informações → Executar assim mesmo**.

O LENKO se atualiza sozinho. Quando sai uma versão nova, a preguiça avisa no balão e só instala quando você clica em **Atualizar**. Também dá para procurar na hora pelo menu da bandeja.

## Como funciona

```mermaid
flowchart LR
    A[IA de código] -- hook --> B[adaptador]
    B -- "POST /event" --> C["App Tauri<br/>127.0.0.1:7777"]
    C -- evento --> D[Mascote na tela]
```

O app não se integra a uma IDE específica, e sim à **ferramenta de IA**. Por isso funciona igual no VS Code, no JetBrains, no Cursor ou num terminal solto.

1. A ferramenta de IA dispara um *hook* em cada momento importante (prompt enviado, tarefa concluída, pedido de permissão).
2. Um adaptador transforma esse hook num evento padronizado e envia para um servidor HTTP local.
3. O app em Tauri recebe o evento e o repassa para o mascote, que muda de estado.

### Contrato de evento

Qualquer ferramenta que consiga fazer um POST pode acordar a preguiça:

```http
POST http://127.0.0.1:7777/event
Content-Type: application/json

{ "tool": "claude-code", "type": "done", "project": "meu-app", "message": "opcional" }
```

`type` aceita `working`, `done` e `waiting`. O servidor escuta apenas em `127.0.0.1`.

## Uso

Na preguiça:

- **Arraste o galho** para mover a preguiça pela tela.
- **Clique no balão ou na preguiça** para dispensar o aviso.
- **Passe o mouse por cima** para ver os botões de silenciar (♪) e fechar (×).

Na bandeja do sistema, o ícone do LENKO fica perto do relógio, na barra de tarefas. Se ele não aparecer, clique na setinha **^** para ver os ícones ocultos.

- **Clique esquerdo** no ícone mostra ou oculta a preguiça.
- **Clique direito** abre o menu:

| Opção | O que faz |
|---|---|
| **Ocultar / Mostrar** | Esconde a preguiça sem fechar o app. Como ela não aparece na barra de tarefas, é por aqui que ela volta. |
| **Silenciar sons** | Liga ou desliga os sons 8-bit. Fica sincronizado com o botão ♪ e é lembrado ao reabrir. |
| **Tamanho** | Pequeno, Médio ou Grande. Só escalas inteiras, para o pixel art continuar nítido. O canto inferior direito fica no lugar. |
| **Conectar / Desconectar do Claude Code** | Adiciona ou remove os hooks do LENKO no `~/.claude/settings.json`, sem mexer nos hooks de outras ferramentas. Reinicie o Claude Code depois. |
| **Iniciar com o sistema** | Abre o LENKO junto com o computador. Vem desligado. |
| **Procurar atualizações** | Verifica na hora se há versão nova. A verificação automática já acontece ao abrir e a cada 6 horas. |
| **Sair** | Fecha o LENKO. |

## Decisões técnicas

- **Tauri em vez de Electron.** Usa o WebView do próprio sistema, então o instalador fica com poucos megabytes em vez de mais de 100 MB, e o consumo de memória é mínimo para algo que fica aberto o dia inteiro.
- **Integração pela IA, não pela IDE.** Um único contrato HTTP desacopla o mascote das ferramentas. Suportar uma nova IA é escrever um adaptador de poucas linhas, sem tocar no app.
- **Adaptador que nunca trava a IA.** Se o app estiver fechado, o adaptador desiste em menos de um segundo e sai com sucesso. O mascote é opcional; o fluxo de trabalho não pode depender dele.
- **Instalação reversível.** O instalador de hooks marca as próprias entradas, preserva configurações de terceiros e faz backup antes de alterar qualquer arquivo.
- **Sprite como dados.** A pixel art é desenhada como uma grade de caracteres no código. Mudar uma expressão é editar uma linha.

## Limitação conhecida

Na extensão nativa do Claude Code para VS Code, o aviso de "Preciso de você" não dispara hoje ([issue #59718](https://github.com/anthropics/claude-code/issues/59718)). O "Terminei!" funciona normalmente. No Claude Code de terminal, inclusive no terminal integrado do VS Code, os três estados funcionam.

## Roadmap

- [x] Mascote com quatro estados, balão e som
- [x] Servidor local de eventos e adaptador para o Claude Code
- [x] Conexão com o Claude Code pelo próprio app, sem terminal
- [x] Ícone na bandeja: ocultar, silenciar, tamanho, conectar/desconectar, iniciar com o sistema e sair
- [x] Opção de iniciar com o sistema
- [x] **Instalador de um clique:** baixar, instalar e clicar em "Conectar", sem terminal
- [ ] Adaptadores para outras ferramentas de IA de código

## LENKO Pro

Este repositório é a versão aberta do LENKO, com a preguiça. Depois que tudo aqui estiver validado, o projeto será clonado para o **LENKO Pro**, uma continuação com fins comerciais. É lá que vão ficar a loja de mascotes e as atualizações para quem tiver interesse.

## Contribuindo

Contribuições são bem-vindas, principalmente adaptadores para outras ferramentas de IA e correções de bugs. Como rodar o projeto, testar e enviar mudanças está em [CONTRIBUTING.md](CONTRIBUTING.md).

## Licença

Código-fonte sob a licença [MIT](LICENSE) © Denzell Barbosa.

O nome LENKO / ЛЕНКО, o logo, os ícones e a arte dos personagens (incluindo a preguiça) não fazem parte da MIT e têm todos os direitos reservados. Forks podem usar o código à vontade, desde que troquem o nome e o mascote. Detalhes em [TRADEMARKS.md](TRADEMARKS.md).
