[English](README.md) | [日本語](README.ja.md) | [简体中文](README.zh-cn.md) | **繁體中文** | [한국어](README.ko.md) | [Español](README.es.md) | [Português (Brasil)](README.pt-br.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Русский](README.ru.md)

# sim，新世代 Vim

**sim**（也稱 *sim editor*，是 *Secure vIM* 的縮寫）是新世代 Vim：以 Rust 從零寫成，保留了 Vim 的模式編輯與核心按鍵。它不讀取 modeline，不載入外掛，不開啟網路連線，並把平常隱藏的東西顯示在畫面上：不可見字元、編輯器之外的寫入、會執行程式碼的儲存庫。

**沒有外掛。沒有 modeline。沒有網路。**

網站：<https://sim.efg-technologies.com/zh-tw/> · Linux 與 macOS · MIT OR Apache-2.0 · [變更記錄](CHANGELOG.md) · [安全性政策](SECURITY.md)

sim 的每一項承諾，都適用於其中開啟的每一個檔案：**沒有明確的操作，任何人的文字都不會遺失**，無論是你的，還是其他行程的。

## 安裝

執行檔是 `sim`，Homebrew 上的套件名稱是 `efg-technologies/tap/sim`。AUR 套件與 crates.io 上的 crate（`sim-editor`，以及它所依賴的函式庫 `sim-editor-core`）尚未發布。

sim 0.2.1 已經推出。可以用 Homebrew 安裝：

```sh
brew install efg-technologies/tap/sim
```

發行版本的原始碼在 [GitHub 的 v0.2.1 發行頁面](https://github.com/efg-technologies/sim/releases/tag/v0.2.1)（`sim-0.2.1.tar.gz`）。從原始碼建置：

```sh
git clone https://github.com/efg-technologies/sim.git
cd sim
cargo install --locked --path sim
```

AUR 套件 `sim-editor`，以及 crates.io 上的 `sim-editor` 與 `sim-editor-core`，即將推出。

之後，如果你喜歡把它當作自己的編輯器：

```sh
export EDITOR=sim                     # Claude Code 的 Ctrl-g、crontab -e 等
git config --global core.editor sim   # git commit（ZZ 提交，:cq 中止）
export SUDO_EDITOR='sim --plain'      # sudoedit（在 Linux 上 sim 本身就是 plain 模式）
```

需要 Rust 1.88 或更新版本。可在 Linux 與 macOS 的任何終端機中執行；當終端機支援時，會使用色彩、kitty 鍵盤協定與 OSC 52（Ghostty、kitty、WezTerm、tmux）。git 與 ripgrep 是選用的：它們在 `PATH` 中時，sim 就會使用。圖示需要 Nerd Font；圖示在 Ghostty、kitty 與 WezTerm 中預設開啟，在其他終端機中預設關閉並給出一次性的提示，以 `zi` 開啟或關閉。更多內容見[安裝頁面](https://sim.efg-technologies.com/zh-tw/install/)。

## 它能做什麼

**顯示被隱藏的東西**

- 由右至左覆寫字元、零寬空格或控制字元，會以洋紅色色塊顯示為 `<U+202E>` 或 `^[`，無論儲存格處於什麼狀態都一樣，也絕不會原封不動地送給終端機。檔案中字面的文字 `<U+202E>` 沒有色塊。
- 其他看不見的東西也同樣顯示：標籤字元（向語言模型偷偷塞指令的手法）、異體字選擇符（把程式碼藏進已發布套件的手法）、名稱裡的諺文填充符或軟連字號、什麼也不連接的零寬連接符、行分隔符、沒有人分配過的碼位。樹、分頁、搜尋和狀態列裡的名稱與內文用同樣的方式顯示。表情符號序列、旗幟、按鍵帽和 Nerd Font 圖示保持原樣。漢字後面的異體字選擇符也會顯示出來。
- 行尾空白顯示為 `·`，不換行空格顯示為 `⍽`，Tab 顯示為 `»`。
- 當另一個行程改寫了開啟的檔案時，變更的行會先閃一下，再在文字中被標出，邊框上會寫明磁碟上有多少處變更，分頁顯示 `↯`，檔案樹也會標出該檔案。`]c` 與 `[c` 跳到下一個與上一個標記。
- 與 git HEAD 的差異顯示為行號旁的標記、檔案樹中每個檔案的狀態字母（資料夾顯示其下最強狀態的字母），以及狀態列中的 `+n ~n -n`。
- 如果儲存庫自身的設定可能執行指令（`core.sshCommand`、`include` 等），就不會執行 git：邊框上會以警示色塊寫出 `git not run`，狀態列會指出相關的鍵與檔案。若 git 在儲存庫自身的設定（`core.fsmonitor`、hooks 路徑、過濾器、外部 diff）被關閉後執行，邊框上會寫出被關閉的數量（`2 git settings off`）。
- Markdown 連結的文字所稱的位址並不是其目標時（`[https://good.example](https://evil.example)`，也包括定義指向別處的參照式連結 `[good.example][1]`、跨多行的連結、目標裡帶反斜線或實體的連結、HTML 的 `<a href="…">` 連結），目標（參照式連結還有文字）會顯示在洋紅色色塊上；閱讀檢視與 calm 外觀中都一樣，閱讀檢視從不隱藏連結的方括號或 URL。

**該拒絕的就拒絕，你的文字不會遺失**

- 設定與指令絕不會從檔案內容中讀取：沒有 modeline，沒有設定檔，沒有外掛，沒有網路。
- 儲存是原子性的（暫存檔、fsync、rename、目錄 fsync），並且會先檢查檔案的雜湊值：如果檔案在磁碟上已被改動，就不會覆蓋任何內容。儲存發現衝突時，`:w!` 在把對方的版本複製到狀態目錄之後保留你的版本，`:e!` 在替你的版本建立快照之後採用對方的版本。
- 未儲存的變更每秒做一次快照；當機、`kill -9` 或關閉終端機之後，重新開啟檔案時會提示還原它們，作為一次復原步驟。
- 搜尋自己尋找 ripgrep，以空環境與 `--no-config` 執行它，並且從不跟隨符號連結走出專案。
- `sim --plain` 只有核心：單一檔案，沒有面板，也沒有外部程式（不用 git，不用 ripgrep，不用過濾器，不用掛鉤）。以 root 身分執行或作為 sudoedit 的編輯器時，這是預設行為。

**觸及、速度與閱讀**

- 從不亂動的專案樹，以 `Ctrl-p` 尋找檔案，以 `Ctrl-f` 搜尋文字並附預覽，可能要開啟的檔案以 `•` 標出，多個檔案配有分頁列。
- 在專案自己的基準測試中，單次按鍵在 p99 下耗時不到 1 ms（100,000 行的檔案，release 建置）；在有 100,000 個檔案的儲存庫中，第一個畫面實測為 1.8 至 8.7 ms。
- 取自終端機調色盤的語法色彩，就地閱讀的 Markdown（標記符號隱藏、表格對齊，游標所在行始終是檔案原文），三種外觀（`zl`），日文禁則換行與全形按鍵。
- 編輯器的標題是麵包屑 `src › sim.rs › leader_key`：路徑，以及游標所在的標題或函式。每個事實只出現在一個地方：狀態列有模式、分支、變更計數和位置，只有在確實如此時才會顯示 `CRLF`、`BOM` 或非 UTF-8 檔案；分頁顯示檔案和 `●`，捲軸顯示位置，檔案樹不再有霧化效果。
- `NO_COLOR` 與少於 8 色的終端機只用粗細、底線與反白來繪製，意義從不只靠顏色傳達：變更的行是 `┆`，資料夾顯示狀態字母，分頁以 `↯` 表示檔案在磁碟上已有變動，捲軸的滑塊是 `║` 軌道，而不是反白的色塊。

**找得到的按鍵**

- `Space` 是 leader 鍵。在前綴鍵（`Space`、`Ctrl-w`、`g`、`z`、`Z`、`[`、`]`，或 `d` 這類運算子）上稍待片刻，會開啟一個小的 which-key 視窗，列出接下來可以按的鍵；繼續輸入的效果與沒有它時完全相同。
- `:help` 與 `F1` 會開啟一份可搜尋的清單，列出每個按鍵與指令，並說明各自的作用；`:help tab` 能找到 `Tab`、`Shift-Tab` 與 `<Space>bp`。
- `:tutor` 會在一份用完即棄的副本裡開啟約 15 分鐘的簡短課程（不會寫入任何內容），練習做對後會自動標上 ✓；語言由 `LC_ALL`、`LC_MESSAGES`、`LANG` 決定，與本 README 相同的十種語言。
- `<Space>bp` 會為每個分頁標上一個字母，按下該字母即可跳到對應的檔案；`<Space>bc` 以同樣的方式關閉分頁（有變更的檔案會保持開啟，與 `:q` 一樣）。第一次開啟第二個檔案時，會有提示告訴你 `Tab` 與 `Shift-Tab`，直到你用過它們三次為止。
- 搜尋的比對結果以醒目提示標出時，狀態列會顯示你位於第幾個比對結果，以及總共有多少個：`[3/12]`。
- `sim --help` 會印出用法與 sim 讀取的環境變數（`NO_COLOR`、`SIM_ON_NORMAL`、`XDG_STATE_HOME`）；`sim --version` 會印出版本。兩者都不會碰終端機。

## sim 一個畫面看完

| 按鍵 | 作用 |
|---|---|
| `i` | 插入 |
| `Ctrl-s` | 儲存 |
| `ZZ` | 儲存並結束 |
| `q` | 結束 |
| `ZQ` | 不儲存結束 |
| `u` | 復原 |
| `U` | 重做 |
| `Y` | 複製到行尾 |
| `/` | 向前搜尋 |
| `n` | 下一個符合項目 |
| `*` | 向前搜尋目前的單字 |
| `yy` | 複製行 |
| `"a` | 暫存器：`"ayw` 複製到 `a`，`"ap` 貼上（`:registers` 列出） |
| `ma` | 設定標記；`'a` 跳到其所在行，`` `a `` 跳到其位置（`:marks` 列出） |
| `]c` | 下一處來自外部的變更 |
| `[c` | 上一處來自外部的變更 |
| `:s` | 取代 |
| `:{range}!cmd` | 過濾 |
| `:seen` | 清除變更標記 |
| `Ctrl-n` | 開關檔案樹 |
| `Ctrl-p` | 尋找檔案 |
| `Ctrl-f` | 搜尋檔案內容 |
| `.` | 有變更的與最近的檔案（在檔案樹中） |
| `Enter` | 展開或收合資料夾（在檔案樹中） |
| `C` | 收合所在的資料夾（在檔案樹中） |
| `Z` | 展開資料夾及其下所有內容（git 忽略的除外）（在檔案樹中） |
| `Ctrl-w w` | 在內文與樹之間切換焦點，樹保持開啟（`Ctrl-w h` 到樹，`Ctrl-w l` 到內文） |
| `Tab` | 下一個檔案 |
| `Shift-Tab` | 上一個檔案 |
| `zh` | 開關語法色彩 |
| `zl` | 下一種外觀 |
| `zi` | 開關圖示 |
| `<Space>` | leader 鍵 |
| `<Space>bp` | 以字母選擇分頁 |
| `<Space>bc` | 選擇要關閉的分頁 |
| `<Space>bk` | 被 `:w!` 或 `:e!` 放棄的版本存放在哪裡 |
| `<Space>ge` | 顯示相對於 HEAD 的變更 |
| `<Space>gi` | 防護對這個儲存庫做了什麼 |
| `<Space>um` | 開關 Markdown 閱讀檢視 |
| `F1` | 說明 |
| `:help` | 說明 |

其餘都是 vi：移動指令、運算子、文字物件、計數、`.`、視覺模式、`:s`、範圍。`Y` 複製到行尾，`U` 是重做，沒有暫存器名稱的複製也會透過 OSC 52 送進系統剪貼簿，在 tmux 裡同樣如此（帶名稱的 `"ay` 只留在 sim 裡）。重新開啟檔案時，游標會回到你離開的位置。開著輸入法時，全形按鍵也能當作指令使用；`SIM_ON_NORMAL='fcitx5-remote -c'` 會在你每次回到一般模式時關閉輸入法（舊名稱 `SI_ON_NORMAL` 仍然有效）。所有按鍵都在同一張表 `sim/keys.tsv` 中，`:help` 與 which-key 讀取它，測試也會對照按鍵的實際作用檢查它。完整清單見[文件頁面](https://sim.efg-technologies.com/zh-tw/docs/)。

## 安全模型

你會開啟並非自己撰寫的檔案與儲存庫。攻擊者控制的是它們的內容，而不是你的機器。sim 努力確保這些內容無法對你隱藏、無法重新設定編輯器，也無法讓編輯器執行程式碼。它**不是沙盒**：它以你的權限執行，不會偵測其他文字中外觀相似的字母，它的各項說法來自專案自己的規格與測試，而不是獨立稽核。語言伺服器尚未實作；計畫提供沙盒化的用戶端。如需回報漏洞，請閱讀[安全性政策](SECURITY.md)。

完整的威脅模型，逐項威脅列在[安全性頁面](https://sim.efg-technologies.com/zh-tw/security/)上。三篇長文講得更深入：[隱藏字元與雙向字元](https://sim.efg-technologies.com/blog/hidden-characters/)、[會執行程式碼的儲存庫](https://sim.efg-technologies.com/blog/repositories-that-run-code/)，以及[安全儲存與機密資訊](https://sim.efg-technologies.com/blog/safe-saves/)。

## 與 Vim、Neovim 和 Helix 比較

Vim、Neovim 與 Helix 都是成熟的編輯器，而 sim 是新的。這裡只比較第一手來源所說的內容；[比較頁面](https://sim.efg-technologies.com/zh-tw/compare/)引用了全部來源（查證於 2026-10-17：Vim 9.2.1202、Neovim 0.12.6、Helix 25.07.1）。

| | Vim | Neovim | Helix | sim |
|---|---|---|---|---|
| 擴充系統 | Vim script、套件 | Lua、Vim script | 已發行版本中沒有外掛系統；以 Steel 為基礎的 PR [#8675](https://github.com/helix-editor/helix/pull/8675) 是草稿 | 沒有，這是刻意的 |
| Modeline | 開啟（root 下關閉）；自 [9.2.0350](https://github.com/vim/vim/commit/4c2879471) 起有 `'modelinestrict'` | 開啟（root 下關閉） | 不剖析（[#729](https://github.com/helix-editor/helix/issues/729)） | 從不讀取 |
| 專案本地設定 | `'exrc'`，預設關閉 | `'exrc'`，信任清單 | 25.07.1 會載入 `.helix/config.toml`；信任機制在開發分支上 | 不存在 |
| U+202E 等字元 | `<202e>` | `<202e>` | 25.07.1 中沒有標記（我們的測試；[#11255](https://github.com/helix-editor/helix/issues/11255)） | 洋紅色色塊上的 `<U+202E>` |

其他編輯器目前仍做得更好的地方：Neovim 與 Helix 內建語言伺服器用戶端，Vim 與 Neovim 有外掛與巨集。sim 目前這些都沒有。

## 路線圖（規劃中，尚未推出）

Vim 的巨集 · 超出 OSC 52 的剪貼簿整合 · 沙盒化的語言伺服器用戶端 · 真正有層次感的 GUI（原型） · AUR 與 crates.io 的套件。

## 參與開發

sim 是一個由兩個 crate 組成的程式：**核心**（`sim-editor-core`，位於 `core/`：緩衝區、vi 的文法、儲存、與其他行程寫入內容的合併、終端機）與**編輯器**（`sim-editor`，位於 `sim/`：可執行檔 `sim`，包含檔案樹、尋找、搜尋、標記、語法、外觀與滑鼠）。每項選擇背後的原因，連同其所依據的研究，都在 [CONCEPT.md](CONCEPT.md) 中。sim 必須做到什麼寫在 [SPEC.md](SPEC.md) 中：每條需求都有編號，每個編號都有測試。

金字塔：[CONCEPT](CONCEPT.md)（為什麼）→ [SPEC](SPEC.md)（做什麼，附編號）→ `core/src` 與 `sim/src`（怎麼做）→ 測試（證明）。變更自上而下流動，證明自下而上流動。規則寫在 [CLAUDE.md](CLAUDE.md) 中。

```sh
sh scripts/check.sh          # fmt、clippy、測試、SPEC↔測試對應、兩個行數上限、核心的依賴、二進位檔大小
sh scripts/e2e.sh            # 核心（即 `sim --plain`）與在 tmux 中的編輯器（獨立 socket）
cargo test --release --workspace -- --ignored bench   # 效能需求
```

屬性測試、對各方文字建立「不遺失」模型的模糊測試器、對儲存的每一步注入故障、以 nvim 為參照的差分測試，以及算繪畫面的黃金快照（golden），共同讓這些承諾名副其實。

## 授權條款

MIT OR Apache-2.0，可任選其一：[MIT](LICENSE-MIT) 或 [Apache-2.0](LICENSE-APACHE)。著作權所有 EFG Technologies。
