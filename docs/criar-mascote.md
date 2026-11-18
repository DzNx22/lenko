# Criar um mascote para o LENKO

Um mascote é um arquivo `.mascote`: um zip com imagens, um `manifest.json` e, se quiser, sons. Não tem código. Para instalar, basta arrastar o arquivo até o LENKO. Para trocar ou remover, use o menu **Mascote** no ícone da bandeja.

A preguiça padrão está neste formato em [`src/mascotes/preguica/`](../src/mascotes/preguica/). É o melhor ponto de partida.

## Regra de autoria

Só personagens originais. Nada de personagens de filmes, jogos, desenhos ou mascotes de marcas, nem versões "inspiradas" que lembrem um deles. Ao publicar, você declara que o personagem é seu ou que tem direito de distribuí-lo. Mascotes que desrespeitem isso são removidos.

## Arquivos

```text
meu-mascote/
├── manifest.json        obrigatório
├── sprites.png          obrigatório: todos os quadros do personagem
├── prop.png             obrigatório: onde ele se segura (galho, nuvem, prateleira...)
├── preview.png          opcional: imagem de vitrine
└── sounds/              opcional
    ├── done.wav         "terminei"  (.wav, .ogg ou .mp3)
    └── waiting.wav      "preciso de você"
```

Qualquer outro arquivo faz o LENKO recusar o pacote.

### `sprites.png`

Todos os quadros têm o mesmo tamanho e ficam em grade, sem espaço entre eles. Eles são numerados a partir de 0, da esquerda para a direita e de cima para baixo:

```text
┌───┬───┬───┬───┐
│ 0 │ 1 │ 2 │ 3 │
├───┼───┼───┼───┤
│ 4 │ 5 │ 6 │ 7 │
└───┴───┴───┴───┘
```

- Quadro de 8 a 64 pixels de lado. A preguiça usa 32×23.
- A imagem toda pode ter até 1024 pixels de lado.
- Fundo transparente. Desenhe no tamanho real, pixel por pixel: o LENKO amplia em escalas inteiras e sem suavizar.

### `prop.png`

O objeto que fica em cima do personagem. É por ele que o usuário arrasta o mascote pela tela. Até 128×32 pixels. O personagem aparece logo abaixo, centralizado.

### Sons

Opcionais. Sem eles, o LENKO toca o bipe 8-bit padrão. Até 512 KB cada; prefira sons curtos (menos de 1 segundo).

## `manifest.json`

```json
{
  "formatVersion": 1,
  "id": "coruja-noturna",
  "name": "Coruja Noturna",
  "author": "Seu Nome",
  "version": "1.0.0",
  "license": "CC-BY-4.0",
  "description": "Fica de olho no código enquanto você dorme.",
  "frame": { "width": 32, "height": 32 },
  "states": {
    "idle":    { "frames": [0] },
    "working": { "frames": [1, 2], "fps": 4, "blink": 3 },
    "done":    { "frames": [4, 5], "fps": 6 },
    "waiting": { "frames": [6, 7], "fps": 8 }
  }
}
```

| Campo | Obrigatório | Regra |
|---|---|---|
| `formatVersion` | sim | Versão do formato. Hoje é `1`. |
| `id` | sim | 3 a 40 caracteres: letras minúsculas, números e hífen. Instalar outro pacote com o mesmo id substitui o anterior. `preguica` é reservado. |
| `name` | sim | Nome mostrado no menu (até 40 caracteres). |
| `author` | sim | Até 60 caracteres. |
| `version` | sim | Versão do seu mascote, ex.: `1.0.0`. |
| `license` | sim | Ex.: `MIT`, `CC-BY-4.0`, `Todos os direitos reservados`. |
| `description` | não | Até 200 caracteres. |
| `frame` | sim | Largura e altura de um quadro, em pixels. |
| `states` | sim | Os quatro estados, cada um com sua animação. |

### Estados

| Estado | Quando aparece |
|---|---|
| `idle` | Nada acontecendo. A preguiça dorme. |
| `working` | A IA está trabalhando. |
| `done` | A IA terminou. Também é usado nas perguntas do LENKO ("Quer que eu me conecte?"). |
| `waiting` | A IA precisa de você (aprovação). |

Cada estado tem:

- `frames`: lista de quadros, de 1 a 32. Com mais de um, eles se repetem em loop.
- `fps`: quadros por segundo, de 1 a 24. O padrão é 4.
- `blink`: opcional. É um quadro mostrado por um instante, de tempos em tempos (a piscada da preguiça).

Os movimentos do corpo inteiro (balançar, pular, tremer) são do LENKO e valem para qualquer mascote. Com "reduzir movimento" ligado no sistema, só o primeiro quadro de cada estado aparece.

## Empacotar

Com o repositório clonado:

```bash
npm run mascote:empacotar -- caminho/para/meu-mascote
```

Isso confere os arquivos e gera `<id>.mascote`. O pacote pode ter até 2 MB. Arraste esse arquivo até o LENKO para testar. Se algo estiver errado, o balão diz o quê.

Sem o repositório, qualquer programa de zip serve: compacte o **conteúdo** da pasta (não a pasta em si) e troque a extensão para `.mascote`.

## Compatibilidade

O `formatVersion` garante que pacotes antigos continuem funcionando. Se um pacote usar um formato mais novo que o do app, o LENKO pede para ser atualizado em vez de quebrar. Campos desconhecidos no manifest são ignorados.
