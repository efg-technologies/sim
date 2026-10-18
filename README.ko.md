[English](README.md) | [日本語](README.ja.md) | [简体中文](README.zh-cn.md) | [繁體中文](README.zh-tw.md) | **한국어** | [Español](README.es.md) | [Português (Brasil)](README.pt-br.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Русский](README.ru.md)

# sim, 차세대 Vim

**sim**(*sim editor*라고도 부르며, *Secure vIM*의 줄임말입니다)은 차세대 Vim입니다. Rust로 처음부터 작성했고, Vim의 모달 편집과 핵심 키를 그대로 유지합니다. 모드라인을 읽지 않고, 플러그인을 불러오지 않으며, 네트워크 연결도 열지 않습니다. 그리고 보통은 숨어 있는 것을 화면에 드러냅니다. 보이지 않는 문자, 에디터 밖에서 일어난 쓰기, 코드를 실행하게 될 저장소가 그것입니다.

**플러그인 없음. 모드라인 없음. 네트워크 없음.**

웹사이트: <https://sim.efg-technologies.com/ko/> · Linux 및 macOS · MIT OR Apache-2.0 · [변경 내역](CHANGELOG.md) · [보안 정책](SECURITY.md)

sim이 하는 모든 약속은 sim에서 열려 있는 모든 파일에 대한 약속입니다. **명시적인 동작 없이는 누구의 텍스트도 사라지지 않습니다.** 내 텍스트도, 다른 프로세스의 텍스트도 마찬가지입니다.

## 설치

바이너리는 `sim`입니다. 패키지 이름은 AUR과 crates.io에서는 `sim-editor`, Homebrew에서는 `efg-technologies/tap/sim`이며, sim의 기반이 되는 라이브러리는 crates.io의 `sim-editor-core`입니다.

sim 0.1.0을 공개했습니다. Homebrew로 설치할 수 있습니다:

```sh
brew install efg-technologies/tap/sim
```

릴리스의 소스는 [GitHub의 v0.1.0 릴리스](https://github.com/efg-technologies/sim/releases/tag/v0.1.0)(`sim-0.1.0.tar.gz`)에 있습니다. 소스에서 직접 빌드하려면 다음과 같이 합니다:

```sh
git clone https://github.com/efg-technologies/sim.git
cd sim
cargo install --locked --path sim
```

AUR 패키지 `sim-editor`와 crates.io의 `sim-editor`, `sim-editor-core`는 곧 공개됩니다.

마음에 든다면 에디터로 지정하세요:

```sh
export EDITOR=sim                     # Claude Code의 Ctrl-g, crontab -e, …
git config --global core.editor sim   # git commit (ZZ는 커밋, :cq는 중단)
export SUDO_EDITOR='sim --plain'      # sudoedit (Linux에서는 sim이 알아서 plain 모드가 됩니다)
```

Rust 1.88 이상이 필요합니다. Linux와 macOS의 아무 터미널에서나 동작하며, 터미널이 지원하면 색, kitty 키보드 프로토콜, OSC 52를 사용합니다(Ghostty, kitty, WezTerm, tmux). git과 ripgrep은 선택 사항이며, `PATH`에 있으면 sim이 사용합니다. 아이콘에는 Nerd Font가 필요합니다. Ghostty, kitty, WezTerm에서는 켜진 채로, 그 밖의 터미널에서는 꺼진 채로 시작하며 힌트가 한 번만 나오고, `zi`로 켜고 끌 수 있습니다. 자세한 내용은 [설치 페이지](https://sim.efg-technologies.com/ko/install/)에 있습니다.

## 기능

**숨은 것을 보여 줍니다**

- 오른쪽에서 왼쪽 방향 재정의 문자, 너비가 0인 공백, 제어 문자는 셀이 어떤 상태이든 마젠타 블록 위에 `<U+202E>`나 `^[`로 그려지며, 터미널에 그대로 전송되지 않습니다. 파일에 문자 그대로 적힌 `<U+202E>` 텍스트에는 블록이 붙지 않습니다.
- 줄 끝 공백은 `·`, 줄 바꿈 없는 공백은 `⍽`, 탭은 `»`로 표시됩니다.
- 다른 프로세스가 열린 파일을 다시 쓰면, 바뀐 줄이 잠깐 번쩍인 뒤 텍스트에 표시되고, 상자가 디스크에 변경이 몇 개 있는지 알려 주며, 탭에는 `↯`가 나타나고, 트리에서도 해당 파일에 표시가 붙습니다. `]c`와 `[c`는 다음 표시와 이전 표시로 이동합니다.
- git HEAD 대비 변경은 거터 표시, 트리에서 파일마다 붙는 상태 문자(폴더에는 그 아래에서 가장 강한 상태의 문자가 표시됩니다), 상태 줄의 `+n ~n -n`으로 보입니다.
- 저장소 자체의 설정이 명령을 실행할 수 있는 곳(`core.fsmonitor`, 필터 드라이버 등)에서는 git을 실행하지 않습니다. 상자에 경고 블록으로 `git not run`이 표시되고, 상태 줄에 원인이 된 키와 파일이 표시됩니다. 저장소 자체의 설정(`core.fsmonitor`, 훅 경로, 필터, 외부 diff)을 꺼 두고 git을 실행한 곳에서는 상자에 그 개수가 표시됩니다(`2 git settings off`).
- 텍스트가 대상이 아닌 다른 주소를 내세우는 Markdown 링크(`[https://good.example](https://evil.example)`, 정의가 다른 곳을 가리키는 참조 링크 `[good.example][1]`, 여러 줄에 걸친 링크, 대상에 백슬래시나 엔티티가 있는 링크, HTML `<a href="…">` 링크 포함)는 대상(참조 링크는 텍스트도)이 마젠타 블록으로 표시됩니다. 읽기 보기와 calm에서도 마찬가지이며, 읽기 보기는 링크의 대괄호와 URL을 절대 숨기지 않습니다.

**거부할 것은 거부하고, 텍스트는 지킵니다**

- 설정과 명령은 파일 내용에서 절대 읽지 않습니다. 모드라인도, 설정 파일도, 플러그인도, 네트워크도 없습니다.
- 저장은 원자적(임시 파일, fsync, rename, 디렉터리 fsync)이며 먼저 파일의 해시를 확인합니다. 디스크의 파일이 바뀌었다면 아무것도 덮어쓰지 않습니다. 저장하려는데 충돌이 발견되면, `:w!` 명령은 상대 버전을 상태 디렉터리에 복사한 뒤 내 버전을 남기고, `:e!` 명령은 내 버전의 스냅샷을 만든 뒤 상대 버전을 가져옵니다.
- 저장하지 않은 변경은 1초마다 스냅샷으로 저장됩니다. 비정상 종료, `kill -9`, 터미널 닫기 뒤에 파일을 다시 열면 하나의 실행 취소 단계로 복원을 제안합니다.
- 검색은 ripgrep을 직접 찾아 빈 환경과 `--no-config`로 실행하며, 프로젝트 밖으로 나가는 심볼릭 링크는 따라가지 않습니다.
- `sim --plain`은 핵심만 남긴 모드입니다. 파일 하나, 패널 없음, 외부 프로그램 없음(git도, ripgrep도, 필터도, 훅도 없음). root로 실행할 때와 sudoedit의 에디터로 쓸 때의 기본값입니다.

**찾아가기, 속도, 읽기**

- 움직이지 않는 프로젝트 트리, 파일을 찾는 `Ctrl-p`, 미리 보기와 함께 텍스트를 검색하는 `Ctrl-f`, `•`로 표시되는 열 가능성이 높은 파일, 탭 바로 다루는 여러 파일.
- 프로젝트 자체 벤치마크에서 키 입력 하나는 p99 기준 1 ms 미만이며(10만 줄 파일, 릴리스 빌드), 파일이 10만 개인 저장소의 첫 프레임은 1.8~8.7 ms로 측정되었습니다.
- 터미널의 팔레트에서 가져오는 구문 색, 제자리에서 읽히는 Markdown(기호는 숨기고 표는 열을 맞추며, 커서가 있는 줄은 늘 파일 그대로), 세 가지 외관(`zl`), 일본어 금칙 처리 줄바꿈과 전각 키.
- 편집기의 제목은 이동 경로(`src › sim.rs › leader_key`)로, 경로와 커서가 있는 제목 또는 함수를 보여 줍니다. 하나의 사실은 한 곳에만 있습니다. 상태 줄에는 모드, 브랜치, 변경 수, 위치가 있고 `CRLF`, `BOM`, UTF-8이 아닌 파일은 해당될 때만 표시하며, 파일과 `●`은 탭에, 위치는 스크롤바에 있고, 트리에는 안개가 끼지 않습니다.
- `NO_COLOR`와 8색 미만의 터미널에서는 굵기, 밑줄, 반전만으로 그리며, 의미가 색에만 의존하는 일은 없습니다. 변경된 줄은 `┆`, 폴더에는 상태 문자, 디스크에서 바뀐 파일의 탭에는 `↯`가 표시되고, 스크롤바의 손잡이는 반전 블록이 아니라 `║` 레일입니다.

**찾을 수 있는 키**

- `Space`는 리더 키입니다. 접두 키(`Space`, `g`, `z`, `Z`, `[`, `]` 또는 `d` 같은 오퍼레이터)에서 잠시 멈추면 다음에 올 수 있는 키를 나열한 작은 which-key 창이 열립니다. 계속 입력하면 창이 없을 때와 똑같이 동작합니다.
- `:help`와 `F1`은 모든 키와 명령을 검색할 수 있는 목록을 열며, 각각이 하는 일도 함께 보여 줍니다. `:help tab`은 `Tab`, `Shift-Tab`, `<Space>bp`를 찾아 줍니다.
- `<Space>bp`는 각 탭에 글자를 붙이고, 그 글자를 누르면 해당 파일로 이동합니다. `<Space>bc`는 같은 방식으로 탭을 닫습니다(변경 사항이 있는 파일은 `:q`와 마찬가지로 열린 채로 남습니다). 두 번째 파일이 처음 열릴 때 `Tab`과 `Shift-Tab`을 알려 주는 힌트가 표시되며, 이 키를 세 번 쓸 때까지 계속됩니다.
- 검색 결과가 강조 표시되어 있는 동안, 상태 줄에 커서가 몇 번째 결과에 있는지와 전체 개수가 `[3/12]`처럼 표시됩니다.
- `sim --help`는 사용법과 sim이 읽는 환경 변수(`NO_COLOR`, `SIM_ON_NORMAL`, `XDG_STATE_HOME`)를 출력하고, `sim --version`은 버전을 출력합니다. 둘 다 터미널을 건드리지 않습니다.

## 한 화면으로 보는 sim

| 키 | 동작 |
|---|---|
| `i` | 삽입 |
| `Ctrl-s` | 저장 |
| `ZZ` | 저장하고 종료 |
| `q` | 종료 |
| `ZQ` | 저장하지 않고 종료 |
| `u` | 실행 취소 |
| `U` | 다시 실행(redo) |
| `Y` | 줄 끝까지 yank |
| `/` | 앞으로 검색 |
| `n` | 다음 일치 항목 |
| `*` | 커서 위치의 단어를 앞으로 검색 |
| `yy` | 줄 yank |
| `]c` | 외부에서 생긴 다음 변경 |
| `[c` | 외부에서 생긴 이전 변경 |
| `:s` | 치환 |
| `:{range}!cmd` | 필터 |
| `:seen` | 변경 표시 지우기 |
| `Ctrl-n` | 트리 열기/닫기 |
| `Ctrl-p` | 파일 찾기 |
| `Ctrl-f` | 파일 내용 검색 |
| `.` | 변경된 파일과 최근 파일(트리 안에서) |
| `Tab` | 다음 파일 |
| `Shift-Tab` | 이전 파일 |
| `zh` | 구문 색 켜기/끄기 |
| `zl` | 다음 외관 |
| `zi` | 아이콘 켜기/끄기 |
| `<Space>` | 리더 |
| `<Space>bp` | 글자로 탭 고르기 |
| `<Space>bc` | 탭을 골라 닫기 |
| `<Space>bk` | `:w!` 또는 `:e!`로 내준 버전이 간 곳 |
| `<Space>ge` | HEAD 기준 변경 사항 보기 |
| `<Space>gi` | 이 저장소에 방어 장치가 한 일 |
| `<Space>um` | Markdown 읽기 보기 켜기/끄기 |
| `F1` | 도움말 |
| `:help` | 도움말 |

나머지는 모두 vi입니다. 모션, 오퍼레이터, 텍스트 오브젝트, 횟수, `.`, 비주얼 모드, `:s`, 범위 지정이 그렇습니다. `Y`는 줄 끝까지 yank하고, `U`는 다시 실행(redo)하며, 모든 yank는 OSC 52를 통해 시스템 클립보드에도 들어갑니다(tmux 안에서도 마찬가지입니다). 파일을 다시 열면 마지막으로 있던 위치로 돌아갑니다. IME가 켜져 있어도 전각 키가 명령으로 동작하며, `SIM_ON_NORMAL='fcitx5-remote -c'`를 지정하면 노멀 모드로 돌아올 때마다 IME가 꺼집니다(예전 이름인 `SI_ON_NORMAL`도 계속 동작합니다). 모든 키는 하나의 표 `sim/keys.tsv`에 있으며, `:help`와 which-key가 이 표를 읽고 테스트가 이 표를 키의 실제 동작과 대조합니다. 전체 목록은 [문서 페이지](https://sim.efg-technologies.com/ko/docs/)에 있습니다.

## 보안 모델

직접 작성하지 않은 파일과 저장소를 열게 됩니다. 공격자가 통제하는 것은 그 내용이지 사용자의 컴퓨터가 아닙니다. sim은 그 내용이 사용자에게서 숨지 못하고, 에디터의 설정을 바꾸지 못하며, 에디터가 코드를 실행하게 만들지 못하도록 하려고 합니다. sim은 **샌드박스가 아닙니다**. 사용자의 권한으로 실행되고, 다른 문자 체계의 비슷하게 생긴 글자는 감지하지 못하며, 주장의 근거는 독립적인 감사가 아니라 프로젝트 자체의 사양과 테스트입니다. 언어 서버는 아직 만들어지지 않았으며, 샌드박스 안에서 실행되는 클라이언트를 계획하고 있습니다. 취약점을 신고하려면 [보안 정책](SECURITY.md)을 읽어 주세요.

위협을 하나씩 짚은 전체 위협 모델은 [보안 페이지](https://sim.efg-technologies.com/ko/security/)에 있습니다. 세 편의 장문 글이 더 깊이 다룹니다. [숨은 문자와 양방향 문자](https://sim.efg-technologies.com/blog/hidden-characters/), [코드를 실행하는 저장소](https://sim.efg-technologies.com/blog/repositories-that-run-code/), [안전한 저장과 비밀 정보](https://sim.efg-technologies.com/blog/safe-saves/)입니다.

## Vim, Neovim, Helix와의 비교

Vim, Neovim, Helix는 성숙한 에디터이고 sim은 새로 나온 에디터입니다. 1차 출처가 말하는 내용만 비교하며, [비교 페이지](https://sim.efg-technologies.com/ko/compare/)에 모든 출처가 인용되어 있습니다(2026-10-17에 확인: Vim 9.2.1202, Neovim 0.12.6, Helix 25.07.1).

| | Vim | Neovim | Helix | sim |
|---|---|---|---|---|
| 확장 | Vim script, 패키지 | Lua, Vim script | 릴리스에는 플러그인 시스템 없음. Steel PR [#8675](https://github.com/helix-editor/helix/pull/8675)는 초안 | 의도적으로 없음 |
| 모드라인 | 켜짐(root는 꺼짐). [9.2.0350](https://github.com/vim/vim/commit/4c2879471)부터 `'modelinestrict'` | 켜짐(root는 꺼짐) | 해석하지 않음([#729](https://github.com/helix-editor/helix/issues/729)) | 읽지 않음 |
| 프로젝트별 설정 | `'exrc'`, 기본값 꺼짐 | `'exrc'`, 신뢰 목록 | 25.07.1에서 `.helix/config.toml`을 불러옴. 신뢰 기능은 개발 브랜치에 있음 | 존재하지 않음 |
| U+202E 등 | `<202e>` | `<202e>` | 25.07.1에는 표시 없음(저희 테스트, [#11255](https://github.com/helix-editor/helix/issues/11255)) | 마젠타 블록 위의 `<U+202E>` |

다른 에디터가 아직은 더 잘하는 부분: Neovim과 Helix는 언어 서버 클라이언트를 제공하고, Vim과 Neovim에는 플러그인과 매크로가 있습니다. sim에는 아직 이 중 어느 것도 없습니다.

## 로드맵(예정, 미제공)

Vim 레지스터, 매크로, 마크 · OSC 52를 넘어서는 클립보드 연동 · 샌드박스 안에서 실행되는 언어 서버 클라이언트 · `:tutor` · git diff 트리 · 진짜 깊이감을 가진 GUI(프로토타입) · AUR, crates.io 패키지.

## 개발에 참여하기

sim은 두 개의 크레이트로 이루어진 하나의 프로그램입니다. **코어**(`sim-editor-core`, `core/` 안: 버퍼, vi 문법, 저장, 다른 프로세스가 쓰는 내용과의 병합, 터미널)와 **에디터**(`sim-editor`, `sim/` 안: 트리, 파일 찾기, 검색, 마크, 구문, 외관, 마우스를 갖춘 `sim` 바이너리)입니다. 각 선택의 이유는 그 근거가 된 조사와 함께 [CONCEPT.md](CONCEPT.md)에 있습니다. sim이 해야 할 일은 [SPEC.md](SPEC.md)에 있으며, 모든 요구 사항에는 ID가 있고 모든 ID에는 테스트가 있습니다.

피라미드: [CONCEPT](CONCEPT.md)(왜) → [SPEC](SPEC.md)(무엇을, ID와 함께) → `core/src`와 `sim/src`(어떻게) → 테스트(증명). 변경은 아래로 흐르고, 증명은 위로 흐릅니다. 규칙은 [CLAUDE.md](CLAUDE.md)에 있습니다.

```sh
sh scripts/check.sh          # fmt, clippy, 테스트, SPEC↔테스트 추적, 두 가지 줄 수 상한, 코어의 의존성, 바이너리 크기
sh scripts/e2e.sh            # 코어(`sim --plain`)와 tmux 안의 에디터(별도 소켓)
cargo test --release --workspace -- --ignored bench   # 성능 요구 사항
```

속성 기반 테스트, 모든 쪽 텍스트의 무손실 모델을 갖춘 퍼저, 저장의 각 단계에서의 장애 주입, nvim과 비교하는 차분 오라클, 렌더링된 화면의 골든 파일이 약속을 지키게 합니다.

## 라이선스

MIT OR Apache-2.0, 둘 중 원하는 쪽을 선택할 수 있습니다. [MIT](LICENSE-MIT) 또는 [Apache-2.0](LICENSE-APACHE). Copyright EFG Technologies.
