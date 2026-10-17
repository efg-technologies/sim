[English](README.md) | [日本語](README.ja.md) | **简体中文** | [繁體中文](README.zh-tw.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Português (Brasil)](README.pt-br.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Русский](README.ru.md)

# sim，新一代 Vim

**sim**（也称 *sim editor*，是 *Secure vIM* 的缩写）是新一代 Vim：用 Rust 从零写成，保留了 Vim 的模态编辑和核心按键。它不读取 modeline，不加载插件，不打开网络连接，并把平时隐藏的东西显示在屏幕上：不可见字符、编辑器之外的写入、会执行代码的仓库。

**没有插件。没有 modeline。没有网络。**

网站：<https://sim.efg-technologies.com/zh-cn/> · Linux 和 macOS · MIT OR Apache-2.0 · [更新日志](CHANGELOG.md) · [安全政策](SECURITY.md)

sim 的每一项承诺，都适用于其中打开的每一个文件：**没有明确的操作，任何人的文本都不会丢失**，无论是你的，还是其他进程的。

## 安装

可执行文件是 `sim`。软件包名称在 AUR 和 crates.io 上是 `sim-editor`，在 Homebrew 上是 `efg-technologies/tap/sim`；它所基于的库在 crates.io 上是 `sim-editor-core`。

sim 0.1.0 已经发布。可以用 Homebrew 安装：

```sh
brew install efg-technologies/tap/sim
```

发布版本的源码在 [GitHub 的 v0.1.0 发布页](https://github.com/efg-technologies/sim/releases/tag/v0.1.0)（`sim-0.1.0.tar.gz`）。从源码构建：

```sh
git clone https://github.com/efg-technologies/sim.git
cd sim
cargo install --locked --path sim
```

AUR 软件包 `sim-editor`，以及 crates.io 上的 `sim-editor` 和 `sim-editor-core`，即将推出。

之后，如果你喜欢把它当作自己的编辑器：

```sh
export EDITOR=sim                     # Claude Code 的 Ctrl-g、crontab -e 等
git config --global core.editor sim   # git commit（ZZ 提交，:cq 中止）
export SUDO_EDITOR='sim --plain'      # sudoedit（在 Linux 上 sim 本身就是 plain 模式）
```

需要 Rust 1.88 或更高版本。可在 Linux 和 macOS 的任意终端中运行；当终端支持时，会使用颜色、kitty 键盘协议和 OSC 52（Ghostty、kitty、WezTerm、tmux）。git 和 ripgrep 是可选的：它们在 `PATH` 中时，sim 就会使用。图标需要 Nerd Font；图标在 Ghostty、kitty 和 WezTerm 中默认开启，在其他终端中默认关闭并给出一次性的提示，用 `zi` 开启或关闭。更多内容见[安装页面](https://sim.efg-technologies.com/zh-cn/install/)。

## 它能做什么

**显示被隐藏的东西**

- 从右到左覆盖字符、零宽空格或控制字符，会以品红色块显示为 `<U+202E>` 或 `^[`，无论单元格处于什么状态都一样，也绝不会原样发送给终端。文件中字面的文本 `<U+202E>` 没有色块。
- 行尾空白显示为 `·`，不间断空格显示为 `⍽`，制表符显示为 `»`。
- 当另一个进程改写了打开的文件时，改动的行会先闪一下，再在文本中被标出，边框上会写明磁盘上有多少处改动，标签页显示 `↯`，文件树也会标出该文件。`]c` 和 `[c` 跳到下一个和上一个标记。
- 相对 git HEAD 的改动显示为行号旁的标记、文件树中每个文件的状态字母（文件夹显示其下最强状态的字母），以及状态栏里的 `+n ~n -n`。
- 如果仓库自身的配置可能执行命令（`core.fsmonitor`、过滤器驱动等），就不会运行 git：边框上会以警示色块写出 `git not run`，状态栏会指明相关的键和文件。若 git 在仓库自身的设置（`core.fsmonitor`、hooks 路径、过滤器、外部 diff）被关闭后运行，边框上会写出被关闭的数量（`2 git settings off`）。
- Markdown 链接的文字所称的地址并不是其目标时（`[https://good.example](https://evil.example)`，也包括定义指向别处的引用式链接 `[good.example][1]`、跨多行的链接、目标里带反斜杠或实体的链接），目标（引用式链接还有文字）会显示在品红色块上；阅读视图和 calm 外观中都一样，阅读视图从不隐藏链接的方括号或 URL。

**该拒绝的拒绝，你的文本不丢**

- 设置和命令绝不会从文件内容中读取：没有 modeline，没有配置文件，没有插件，没有网络。
- 保存是原子的（临时文件、fsync、rename、目录 fsync），并且会先检查文件的哈希：如果文件在磁盘上已被改动，就不会覆盖任何内容。保存发现冲突时，`:w!` 在把对方的版本复制到状态目录之后保留你的版本，`:e!` 在为你的版本创建快照之后采用对方的版本。
- 未保存的更改每秒做一次快照；崩溃、`kill -9` 或关闭终端之后，重新打开文件时会提示恢复它们，作为一次撤销步骤。
- 搜索自己查找 ripgrep，以空环境和 `--no-config` 运行它，并且从不跟随符号链接走出项目。
- `sim --plain` 只有核心：单个文件，没有面板，也没有外部程序（不用 git，不用 ripgrep，不用过滤器，不用钩子）。以 root 身份运行或作为 sudoedit 的编辑器时，这是默认行为。

**触达、速度与阅读**

- 从不乱动的项目树，用 `Ctrl-p` 查找文件，用 `Ctrl-f` 搜索文本并带预览，可能要打开的文件用 `•` 标出，多个文件配有标签栏。
- 在项目自己的基准测试中，单次按键在 p99 下耗时不到 1 ms（100,000 行的文件，release 构建）；在有 100,000 个文件的仓库中，第一帧实测为 1.8 到 8.7 ms。
- 取自终端调色板的语法颜色，就地阅读的 Markdown（标记符号隐藏、表格对齐，光标所在行始终是文件原文），三种外观（`zl`），日文禁则折行和全角按键。
- 编辑器的标题是面包屑 `src › sim.rs › leader_key`：路径，以及光标所在的标题或函数。每个事实只出现在一个地方：状态栏有模式、分支、改动计数和位置，只有在确实如此时才会显示 `CRLF`、`BOM` 或非 UTF-8 文件；标签页显示文件和 `●`，滚动条显示位置，文件树不再有雾化效果。
- `NO_COLOR` 和少于 8 种颜色的终端只用粗细、下划线和反显来绘制，含义从不只靠颜色传达：改动的行是 `┆`，文件夹显示状态字母，标签页用 `↯` 表示文件在磁盘上已改动，滚动条的滑块是 `║` 轨道，而不是反显的色块。

**找得到的按键**

- `Space` 是 leader 键。在前缀键（`Space`、`g`、`z`、`Z`、`[`、`]`，或 `d` 这样的操作符）上稍等片刻，会打开一个小的 which-key 窗口，列出接下来可以按的键；继续输入的效果与没有它时完全一样。
- `:help` 和 `F1` 会打开一个可搜索的列表，列出每个按键和命令，并说明各自的作用；`:help tab` 能找到 `Tab`、`Shift-Tab` 和 `<Space>bp`。
- `<Space>bp` 会给每个标签页标上一个字母，按下该字母即可跳到对应的文件；`<Space>bc` 用同样的方式关闭标签页（有改动的文件保持打开，与 `:q` 一样）。第一次打开第二个文件时，会有提示告诉你 `Tab` 和 `Shift-Tab`，直到你用过它们三次为止。
- 搜索的匹配项高亮显示时，状态栏会显示你位于第几个匹配项，以及一共有多少个：`[3/12]`。
- `sim --help` 会打印用法和 sim 读取的环境变量（`NO_COLOR`、`SIM_ON_NORMAL`、`XDG_STATE_HOME`）；`sim --version` 会打印版本。两者都不会触碰终端。

## sim 一屏速览

| 按键 | 作用 |
|---|---|
| `i` | 插入 |
| `Ctrl-s` | 保存 |
| `ZZ` | 保存并退出 |
| `q` | 退出 |
| `ZQ` | 不保存退出 |
| `u` | 撤销 |
| `U` | 重做 |
| `Y` | 复制到行尾 |
| `/` | 向前搜索 |
| `n` | 下一个匹配 |
| `*` | 向前搜索当前单词 |
| `yy` | 复制行 |
| `]c` | 下一处来自外部的改动 |
| `[c` | 上一处来自外部的改动 |
| `:s` | 替换 |
| `:{range}!cmd` | 过滤 |
| `:seen` | 清除改动标记 |
| `Ctrl-n` | 开关文件树 |
| `Ctrl-p` | 查找文件 |
| `Ctrl-f` | 搜索文件内容 |
| `.` | 有改动的和最近的文件（在文件树中） |
| `Tab` | 下一个文件 |
| `Shift-Tab` | 上一个文件 |
| `zh` | 开关语法颜色 |
| `zl` | 下一种外观 |
| `zi` | 开关图标 |
| `<Space>` | leader 键 |
| `<Space>bp` | 按字母选择标签页 |
| `<Space>bc` | 选择要关闭的标签页 |
| `<Space>ge` | 显示相对于 HEAD 的改动 |
| `<Space>gi` | 防护对这个仓库做了什么 |
| `<Space>um` | 开关 Markdown 阅读视图 |
| `F1` | 帮助 |
| `:help` | 帮助 |

其余都是 vi：移动命令、操作符、文本对象、计数、`.`、可视模式、`:s`、范围。`Y` 复制到行尾，`U` 是重做，每次复制也会通过 OSC 52 送入系统剪贴板，在 tmux 里同样如此。重新打开文件时，光标会回到你离开的位置。开着输入法时，全角按键也能当作命令使用；`SIM_ON_NORMAL='fcitx5-remote -c'` 会在你每次回到普通模式时关闭输入法（旧名称 `SI_ON_NORMAL` 仍然有效）。所有按键都在同一张表 `sim/keys.tsv` 中，`:help` 和 which-key 读取它，测试也会对照按键的实际作用检查它。完整列表见[文档页面](https://sim.efg-technologies.com/zh-cn/docs/)。

## 安全模型

你会打开并非自己编写的文件和仓库。攻击者控制的是它们的内容，而不是你的机器。sim 努力确保这些内容无法对你隐藏、无法重新配置编辑器，也无法让编辑器运行代码。它**不是沙箱**：它以你的权限运行，不会检测其他文字中外观相似的字母，它的各项说法来自项目自己的规格和测试，而不是独立审计。语言服务器尚未实现；计划提供沙箱化的客户端。如需报告漏洞，请阅读[安全政策](SECURITY.md)。

完整的威胁模型，逐条威胁列在[安全页面](https://sim.efg-technologies.com/zh-cn/security/)上。三篇长文讲得更深入：[隐藏字符与双向字符](https://sim.efg-technologies.com/blog/hidden-characters/)、[会执行代码的仓库](https://sim.efg-technologies.com/blog/repositories-that-run-code/)，以及[安全保存与机密信息](https://sim.efg-technologies.com/blog/safe-saves/)。

## 与 Vim、Neovim 和 Helix 对比

Vim、Neovim 和 Helix 都是成熟的编辑器，而 sim 是新的。这里只对比一手来源所说的内容；[对比页面](https://sim.efg-technologies.com/zh-cn/compare/)引用了全部来源（核查于 2026-10-17：Vim 9.2.1202、Neovim 0.12.6、Helix 25.07.1）。

| | Vim | Neovim | Helix | sim |
|---|---|---|---|---|
| 扩展系统 | Vim script、软件包 | Lua、Vim script | 已发布版本中没有插件系统；基于 Steel 的 PR [#8675](https://github.com/helix-editor/helix/pull/8675) 是草稿 | 没有，这是有意为之 |
| Modeline | 开启（root 下关闭）；自 [9.2.0350](https://github.com/vim/vim/commit/4c2879471) 起有 `'modelinestrict'` | 开启（root 下关闭） | 不解析（[#729](https://github.com/helix-editor/helix/issues/729)） | 从不读取 |
| 项目本地配置 | `'exrc'`，默认关闭 | `'exrc'`，信任列表 | 25.07.1 会加载 `.helix/config.toml`；信任机制在开发分支上 | 不存在 |
| U+202E 及同类字符 | `<202e>` | `<202e>` | 25.07.1 中没有标记（我们的测试；[#11255](https://github.com/helix-editor/helix/issues/11255)） | 品红色块上的 `<U+202E>` |

其他编辑器目前仍做得更好的地方：Neovim 和 Helix 自带语言服务器客户端，Vim 和 Neovim 有插件和宏。sim 目前这些都没有。

## 路线图（计划中，尚未发布）

Vim 的寄存器、宏和标记 · 超出 OSC 52 的剪贴板集成 · 沙箱化的语言服务器客户端 · `:tutor` · git 差异树 · 真正有层次感的 GUI（原型） · AUR 和 crates.io 的软件包。

## 参与开发

sim 是一个由两个 crate 组成的程序：**核心**（`sim-editor-core`，位于 `core/`：缓冲区、vi 的语法、保存、与其他进程写入内容的合并、终端）和**编辑器**（`sim-editor`，位于 `sim/`：可执行文件 `sim`，包含文件树、查找、搜索、标记、语法、外观和鼠标）。每项选择背后的原因，连同其所依据的研究，都在 [CONCEPT.md](CONCEPT.md) 中。sim 必须做到什么写在 [SPEC.md](SPEC.md) 中：每条需求都有编号，每个编号都有测试。

金字塔：[CONCEPT](CONCEPT.md)（为什么）→ [SPEC](SPEC.md)（做什么，带编号）→ `core/src` 和 `sim/src`（怎么做）→ 测试（证明）。变更自上而下流动，证明自下而上流动。规则写在 [CLAUDE.md](CLAUDE.md) 中。

```sh
sh scripts/check.sh          # fmt、clippy、测试、SPEC↔测试对应、两个行数上限、核心的依赖、二进制大小
sh scripts/e2e.sh            # 核心（即 `sim --plain`）和在 tmux 中的编辑器（独立 socket）
cargo test --release --workspace -- --ignored bench   # 性能需求
```

属性测试、对各方文本建立“不丢失”模型的模糊测试器、对保存的每一步注入故障、以 nvim 为参照的差分测试，以及渲染屏幕的黄金快照（golden），共同让这些承诺名副其实。

## 许可证

MIT OR Apache-2.0，可任选其一：[MIT](LICENSE-MIT) 或 [Apache-2.0](LICENSE-APACHE)。版权所有 EFG Technologies。
