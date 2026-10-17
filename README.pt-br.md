[English](README.md) | [日本語](README.ja.md) | [简体中文](README.zh-cn.md) | [繁體中文](README.zh-tw.md) | [한국어](README.ko.md) | [Español](README.es.md) | **Português (Brasil)** | [Français](README.fr.md) | [Deutsch](README.de.md) | [Русский](README.ru.md)

# sim, o Vim de nova geração

O **sim** (também chamado de *sim editor*, abreviação de *Secure vIM*) é o Vim de nova geração: escrito do zero em Rust, mantém a edição modal e as teclas básicas do Vim. Ele não lê modelines, não carrega plugins e não abre conexões de rede, e coloca na tela o que normalmente fica oculto: caracteres invisíveis, escritas feitas de fora do editor, repositórios que executariam código.

**Sem plugins. Sem modelines. Sem rede.**

Site: <https://sim.efg-technologies.com/pt-br/> · Linux e macOS · MIT OR Apache-2.0 · [Histórico de mudanças](CHANGELOG.md) · [Política de segurança](SECURITY.md)

Tudo o que o sim promete, ele promete para todo arquivo aberto nele: **sem uma ação explícita, o texto de ninguém se perde**, nem o seu nem o de outro processo.

## Instalação

O binário é `sim`. O nome do pacote é `sim-editor` no AUR e no crates.io, e `efg-technologies/tap/sim` no Homebrew; a biblioteca sobre a qual ele é construído é `sim-editor-core`, no crates.io.

O sim 0.1.0 já foi publicado. Instale com o Homebrew:

```sh
brew install efg-technologies/tap/sim
```

O código-fonte da versão está na [página da versão v0.1.0 no GitHub](https://github.com/efg-technologies/sim/releases/tag/v0.1.0) (`sim-0.1.0.tar.gz`). Para compilar a partir do código-fonte:

```sh
git clone https://github.com/efg-technologies/sim.git
cd sim
cargo install --locked --path sim
```

O pacote `sim-editor` do AUR e os crates `sim-editor` e `sim-editor-core` do crates.io virão em breve.

Depois, se você gostar dele como editor:

```sh
export EDITOR=sim                     # Ctrl-g do Claude Code, crontab -e, …
git config --global core.editor sim   # git commit (ZZ confirma, :cq cancela)
export SUDO_EDITOR='sim --plain'      # sudoedit (no Linux, o sim já fica em modo plain sozinho)
```

É preciso Rust 1.88 ou posterior. Roda no Linux e no macOS, em qualquer terminal; cores, o protocolo de teclado do kitty e o OSC 52 são usados quando o terminal os oferece (Ghostty, kitty, WezTerm, tmux). git e ripgrep são opcionais: o sim os usa quando estão no `PATH`. Os ícones exigem uma Nerd Font; começam ligados no Ghostty, no kitty e no WezTerm e desligados nos demais, com uma dica exibida uma única vez, e `zi` os liga e desliga. Mais na [página de instalação](https://sim.efg-technologies.com/pt-br/install/).

## O que ele faz

**Mostra o que está oculto**

- Uma substituição da direita para a esquerda, um espaço de largura zero ou um caractere de controle é desenhado como `<U+202E>` ou `^[` sobre um bloco magenta, em qualquer estado em que uma célula possa estar, e nunca é enviado cru ao terminal. O texto literal `<U+202E>` em um arquivo não recebe bloco.
- Espaços no fim da linha aparecem como `·`, um espaço não separável como `⍽` e uma tabulação como `»`.
- Quando outro processo reescreve o arquivo aberto, as linhas alteradas são marcadas no texto, a caixa indica quantas mudanças estão no disco, a aba mostra `↯` e a árvore marca o arquivo. `]c` e `[c` vão para a marca seguinte e para a anterior.
- As mudanças em relação ao git HEAD aparecem como sinais na margem, uma letra de status para cada arquivo na árvore (uma pasta mostra a letra do status mais forte abaixo dela) e `+n ~n -n` na linha de status.
- O git não é executado onde a configuração do próprio repositório poderia executar um comando (`core.fsmonitor`, um driver de filtro e mais). A linha de status indica a chave e o arquivo.

**Recusa o que deve recusar e preserva o seu texto**

- Configurações e comandos nunca são lidos do conteúdo dos arquivos: sem modelines, sem arquivos de configuração, sem plugins, sem rede.
- Os salvamentos são atômicos (arquivo temporário, fsync, rename, fsync do diretório) e verificam antes o hash do arquivo: se o arquivo mudou no disco, nada é sobrescrito. Quando um salvamento encontra um conflito, `:w!` mantém a sua versão depois de copiar a do disco para o diretório de estado, e `:e!` adota a do disco depois de um snapshot da sua.
- As mudanças não salvas ganham um snapshot a cada segundo; depois de uma falha, de um `kill -9` ou de um terminal fechado, reabrir o arquivo as oferece de volta como uma única etapa de desfazer.
- A busca encontra o ripgrep sozinha, o executa com um ambiente vazio e `--no-config` e nunca segue um link simbólico para fora do projeto.
- `sim --plain` é só o núcleo: um só arquivo, nenhum painel e nenhum programa externo (sem git, sem ripgrep, sem filtro, sem hook). É o padrão como root e como o editor do sudoedit.

**Alcance, velocidade e leitura**

- Uma árvore de projeto que nunca se mexe, `Ctrl-p` para encontrar arquivos, `Ctrl-f` para buscar texto com prévia, arquivos prováveis marcados com `•`, vários arquivos com uma barra de abas.
- No benchmark do próprio projeto, uma tecla leva menos de 1 ms no p99 (arquivo de 100 mil linhas, build de release); o primeiro quadro em um repositório com 100 mil arquivos foi medido em 1,8 a 8,7 ms.
- Cores de sintaxe tiradas da paleta do seu terminal, Markdown desenhado sem ocultar nenhum caractere, três aparências (`zl`), quebra de linha com regras kinsoku do japonês e teclas de largura total.
- `NO_COLOR` e terminais com menos de 8 cores desenham só com peso, sublinhado e vídeo inverso, e o significado nunca depende só da cor: uma linha alterada é `┆`, as pastas mostram letras de status, uma aba mostra `↯` para um arquivo alterado no disco.

**Teclas fáceis de encontrar**

- `Space` é a tecla líder. Uma pausa em um prefixo (`Space`, `g`, `z`, `Z`, `[`, `]` ou um operador como `d`) abre uma pequena janela which-key que lista as teclas que podem vir em seguida; continuar digitando funciona exatamente como sem ela.
- `:help` e `F1` abrem uma lista pesquisável de todas as teclas e comandos, com o que cada um faz; `:help tab` encontra `Tab`, `Shift-Tab` e `<Space>bp`.
- `<Space>bp` põe uma letra em cada aba, e a letra leva ao seu arquivo; `<Space>bc` faz o mesmo para fechar uma (um arquivo com mudanças continua aberto, como com `:q`). Uma dica cita `Tab` e `Shift-Tab` na primeira vez que um segundo arquivo é aberto, até você usá-las três vezes.
- Enquanto as ocorrências de uma busca estão destacadas, a linha de status diz em qual delas você está e quantas existem, `[3/12]`.
- `sim --help` imprime o uso e o ambiente que o sim lê (`NO_COLOR`, `SIM_ON_NORMAL`, `XDG_STATE_HOME`); `sim --version` imprime a versão. Nenhum dos dois toca no terminal.

## O sim em uma tela

| Teclas | O que fazem |
|---|---|
| `i` | inserir |
| `Ctrl-s` | salvar |
| `ZZ` | salvar e sair |
| `q` | sair |
| `ZQ` | sair sem salvar |
| `u` | desfazer |
| `U` | refazer |
| `Y` | copiar até o fim da linha |
| `/` | buscar para a frente |
| `n` | próxima ocorrência |
| `*` | buscar a palavra para a frente |
| `yy` | copiar linhas |
| `]c` | próxima mudança vinda de fora |
| `[c` | mudança anterior vinda de fora |
| `:s` | substituir |
| `:{range}!cmd` | filtrar |
| `:seen` | limpar as marcas de mudança |
| `Ctrl-n` | mostrar ou ocultar a árvore |
| `Ctrl-p` | encontrar arquivos |
| `Ctrl-f` | buscar no conteúdo dos arquivos |
| `.` | arquivos alterados e recentes (na árvore) |
| `Tab` | próximo arquivo |
| `Shift-Tab` | arquivo anterior |
| `zh` | cores de sintaxe ligadas ou desligadas |
| `zl` | próxima aparência |
| `zi` | ícones ligados ou desligados |
| `<Space>` | líder |
| `<Space>bp` | escolher uma aba por letra |
| `<Space>bc` | escolher uma aba para fechar |
| `<Space>ge` | ver as mudanças em relação ao HEAD |
| `F1` | ajuda |
| `:help` | ajuda |

Tudo o mais é vi: movimentos, operadores, objetos de texto, contadores, `.`, modo Visual, `:s`, intervalos. `Y` copia até o fim da linha, `U` refaz, e todo yank também chega à área de transferência do sistema por meio do OSC 52, inclusive através do tmux. Reabrir um arquivo leva você de volta ao ponto em que o deixou. Com o IME ligado, as teclas de largura total funcionam como comandos; `SIM_ON_NORMAL='fcitx5-remote -c'` desliga o IME sempre que você volta ao modo Normal (`SI_ON_NORMAL`, o nome antigo, continua funcionando). Todas as teclas estão em uma única tabela, `sim/keys.tsv`, que `:help` e o which-key leem e que os testes conferem com o que as teclas fazem. A lista completa está na [página de documentação](https://sim.efg-technologies.com/pt-br/docs/).

## Modelo de segurança

Você abre arquivos e repositórios que não escreveu. O atacante controla o conteúdo deles, não a sua máquina. O sim procura garantir que esse conteúdo não consiga se esconder de você, não consiga reconfigurar o editor e não consiga fazer o editor executar código. Ele **não é um sandbox**: roda com os seus privilégios, não detecta letras parecidas de outros sistemas de escrita, e as afirmações dele vêm da especificação e dos testes do próprio projeto, não de uma auditoria independente. O suporte a servidores de linguagem ainda não existe; um cliente em sandbox está planejado. Para relatar uma vulnerabilidade, leia a [política de segurança](SECURITY.md).

O modelo de ameaças completo, uma ameaça por vez, está na [página de segurança](https://sim.efg-technologies.com/pt-br/security/). Três artigos longos aprofundam o assunto: [caracteres ocultos e bidirecionais](https://sim.efg-technologies.com/blog/hidden-characters/), [repositórios que executam código](https://sim.efg-technologies.com/blog/repositories-that-run-code/) e [salvamentos seguros e segredos](https://sim.efg-technologies.com/blog/safe-saves/).

## Comparação com Vim, Neovim e Helix

Vim, Neovim e Helix são editores maduros e o sim é novo. Só se compara o que uma fonte primária diz; a [página de comparação](https://sim.efg-technologies.com/pt-br/compare/) cita tudo (verificado em 2026-10-17: Vim 9.2.1202, Neovim 0.12.6, Helix 25.07.1).

| | Vim | Neovim | Helix | sim |
|---|---|---|---|---|
| Extensões | Vim script, pacotes | Lua, Vim script | nenhum sistema de plugins em uma versão lançada; o PR do Steel [#8675](https://github.com/helix-editor/helix/pull/8675) é um rascunho | nenhuma, por decisão de projeto |
| Modelines | ativadas (desativadas para root); `'modelinestrict'` desde [9.2.0350](https://github.com/vim/vim/commit/4c2879471) | ativadas (desativadas para root) | não interpretadas ([#729](https://github.com/helix-editor/helix/issues/729)) | nunca lidas |
| Configuração local do projeto | `'exrc'`, desativada por padrão | `'exrc'`, lista de confiança | `.helix/config.toml` carregado na 25.07.1; a confiança no workspace está na branch de desenvolvimento | não existe |
| U+202E e companhia | `<202e>` | `<202e>` | sem marcador na 25.07.1 (nosso teste; [#11255](https://github.com/helix-editor/helix/issues/11255)) | `<U+202E>` sobre um bloco magenta |

O que os outros ainda fazem melhor, por enquanto: Neovim e Helix incluem um cliente de servidor de linguagem, e Vim e Neovim têm plugins e macros. O sim ainda não tem nada disso.

## Roadmap (planejado, não lançado)

Registradores, macros e marcas do Vim · integração com a área de transferência além do OSC 52 · um cliente de servidor de linguagem em sandbox · `:tutor` · uma visualização de leitura de Markdown · uma árvore de diff do git · uma GUI com profundidade de verdade (um protótipo) · pacotes para o AUR e o crates.io.

## Desenvolvimento

O sim é um programa em dois crates: o **núcleo** (`sim-editor-core`, em `core/`: o buffer, a gramática do vi, os salvamentos, a mescla com o que outros processos escrevem, o terminal) e o **editor** (`sim-editor`, em `sim/`: o binário `sim`, com a árvore, a busca de arquivos e de texto, as marcas, a sintaxe, as aparências e o mouse). O porquê de cada escolha está em [CONCEPT.md](CONCEPT.md), com a pesquisa em que ele se apoia. O que o sim precisa fazer está em [SPEC.md](SPEC.md): cada requisito tem um ID, e cada ID tem testes.

A pirâmide: [CONCEPT](CONCEPT.md) (por quê) → [SPEC](SPEC.md) (o quê, com IDs) → `core/src` e `sim/src` (como) → testes (prova). As mudanças descem; a prova sobe. O [CLAUDE.md](CLAUDE.md) reúne as regras.

```sh
sh scripts/check.sh          # fmt, clippy, testes, rastreio SPEC↔teste, os dois limites de linhas, as dependências do núcleo, tamanho do binário
sh scripts/e2e.sh            # o núcleo (como `sim --plain`) e o editor no tmux (socket próprio)
cargo test --release --workspace -- --ignored bench   # requisitos de desempenho
```

Testes de propriedade, fuzzers com um modelo de não perda do texto de cada lado, injeção de falhas em cada etapa de um salvamento, um oráculo diferencial com o nvim e goldens da tela renderizada mantêm as promessas honestas.

## Licença

MIT OR Apache-2.0, à sua escolha: [MIT](LICENSE-MIT) ou [Apache-2.0](LICENSE-APACHE). Copyright EFG Technologies.
