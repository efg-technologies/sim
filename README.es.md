[English](README.md) | [日本語](README.ja.md) | [简体中文](README.zh-cn.md) | [繁體中文](README.zh-tw.md) | [한국어](README.ko.md) | **Español** | [Português (Brasil)](README.pt-br.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Русский](README.ru.md)

# sim, el Vim de nueva generación

**sim** (también llamado *sim editor*, abreviatura de *Secure vIM*) es el Vim de nueva generación: escrito desde cero en Rust, conserva la edición modal y las teclas esenciales de Vim. No lee modelines, no carga plugins y no abre ninguna conexión de red, y muestra en pantalla lo que normalmente queda oculto: caracteres invisibles, escrituras desde fuera del editor y repositorios que ejecutarían código.

**Sin plugins. Sin modelines. Sin red.**

Sitio web: <https://sim.efg-technologies.com/es/> · Linux y macOS · MIT OR Apache-2.0 · [Registro de cambios](CHANGELOG.md) · [Política de seguridad](SECURITY.md)

Todo lo que sim promete, lo promete para cada archivo abierto en él: **sin una acción explícita, el texto de nadie se pierde**, ni el propio ni el de otro proceso.

## Instalación

El binario es `sim`. El paquete se llama `sim-editor` en AUR y crates.io, y `efg-technologies/tap/sim` en Homebrew; la biblioteca sobre la que se construye es `sim-editor-core`, en crates.io.

sim 0.1.0 ya está publicado. Se instala con Homebrew:

```sh
brew install efg-technologies/tap/sim
```

El código fuente de la versión está en la [página de la versión v0.1.0 en GitHub](https://github.com/efg-technologies/sim/releases/tag/v0.1.0) (`sim-0.1.0.tar.gz`). Para compilarlo desde el código fuente:

```sh
git clone https://github.com/efg-technologies/sim.git
cd sim
cargo install --locked --path sim
```

El paquete `sim-editor` de AUR y los crates `sim-editor` y `sim-editor-core` de crates.io llegarán próximamente.

Después, si se quiere usar como editor:

```sh
export EDITOR=sim                     # Ctrl-g de Claude Code, crontab -e, …
git config --global core.editor sim   # git commit (ZZ confirma, :cq cancela)
export SUDO_EDITOR='sim --plain'      # sudoedit (en Linux sim ya es plain por sí mismo)
```

Requiere Rust 1.88 o posterior. Funciona en Linux y macOS, en cualquier terminal; se usan los colores, el protocolo de teclado de kitty y OSC 52 cuando la terminal los ofrece (Ghostty, kitty, WezTerm, tmux). git y ripgrep son opcionales: sim los usa cuando están en `PATH`. Los iconos requieren una Nerd Font; empiezan activados en Ghostty, kitty y WezTerm y desactivados en el resto, con una pista que se muestra una sola vez, y `zi` los activa y los desactiva. Más información en la [página de instalación](https://sim.efg-technologies.com/es/install/).

## Qué hace

**Muestra lo oculto**

- Una anulación de derecha a izquierda, un espacio de ancho cero o un carácter de control se dibuja como `<U+202E>` o `^[` sobre un bloque magenta, en cualquier estado en que pueda estar una celda, y nunca se envía en bruto a la terminal. El texto literal `<U+202E>` de un archivo no recibe bloque.
- Los espacios finales se ven como `·`, un espacio de no separación como `⍽` y un tabulador como `»`.
- Cuando otro proceso reescribe el archivo abierto, las líneas cambiadas destellan un instante y luego se marcan en el texto, el recuadro indica cuántos cambios hay en disco, la pestaña muestra `↯` y el árbol marca el archivo. `]c` y `[c` van a la marca siguiente y a la anterior.
- Los cambios frente a git HEAD aparecen como signos en el margen, una letra de estado para cada archivo del árbol (una carpeta muestra la letra del estado más fuerte que hay debajo) y `+n ~n -n` en la línea de estado.
- No se ejecuta git donde la propia configuración del repositorio podría ejecutar un comando (`core.fsmonitor`, un controlador de filtro y más): el recuadro dice `git not run` sobre el bloque de alerta y la línea de estado indica la clave y el archivo. Donde git sí se ejecuta con ajustes del propio repositorio desactivados (`core.fsmonitor`, la ruta de hooks, filtros o diff externo), el recuadro dice cuántos (`2 git settings off`).
- Un enlace de Markdown cuyo texto nombra una dirección que su destino no es (`[https://good.example](https://evil.example)`, también un enlace de referencia `[good.example][1]` cuya definición apunta a otro sitio, un enlace que ocupa varias líneas o que tiene una barra invertida o una entidad en el destino, un enlace de HTML `<a href="…">`) tiene su destino, y en una referencia también su texto, sobre el bloque magenta, en la vista de lectura y también en calm; la vista de lectura nunca oculta los corchetes ni la URL de un enlace.

**Rechaza lo que debe y conserva el texto**

- Los ajustes y comandos nunca se leen del contenido de un archivo: sin modelines, sin archivos de configuración, sin plugins, sin red.
- Los guardados son atómicos (archivo temporal, fsync, renombrado, fsync del directorio) y comprueban primero el hash del archivo: si el archivo cambió en disco, no se sobrescribe nada. Cuando un guardado detecta un conflicto, `:w!` conserva la versión propia tras copiar la ajena al directorio de estado, y `:e!` toma la ajena tras una instantánea de la propia.
- Los cambios sin guardar se capturan en una instantánea cada segundo; tras un fallo, un `kill -9` o una terminal cerrada, al volver a abrir el archivo se ofrecen de nuevo como un único paso de deshacer.
- La búsqueda localiza ripgrep por sí misma, lo ejecuta con un entorno vacío y `--no-config`, y nunca sigue un enlace simbólico fuera del proyecto.
- `sim --plain` es solo el núcleo: un solo archivo, sin panel y sin ningún programa externo (ni git, ni ripgrep, ni filtro, ni hook). Es el modo predeterminado como root y como editor de sudoedit.

**Alcance, velocidad y lectura**

- Un árbol de proyecto que nunca se mueve, `Ctrl-p` para buscar archivos, `Ctrl-f` para buscar texto con vista previa, archivos probables marcados con `•` y varios archivos con una barra de pestañas.
- En el benchmark del propio proyecto, una tecla tarda menos de 1 ms en el percentil 99 (archivo de 100 000 líneas, compilación release); el primer fotograma en un repositorio de 100 000 archivos se midió en 1,8 a 8,7 ms.
- Colores de sintaxis a partir de la paleta de la terminal, Markdown que se lee en su sitio (marcas ocultas, tablas alineadas y la línea donde estás siempre es el texto del archivo), tres apariencias (`zl`), ajuste de línea japonés con kinsoku y teclas de ancho completo.
- El título del editor es una ruta de migas, `src › sim.rs › leader_key`: la ruta y el encabezado o la función donde está el cursor. Cada dato está en un solo lugar: la línea de estado tiene el modo, la rama, los contadores y la posición, y dice `CRLF`, `BOM` o que el archivo no es UTF-8 solo cuando es así; la pestaña tiene el archivo y `●`, la barra de desplazamiento la posición, y el árbol no está en niebla.
- Con `NO_COLOR` y en las terminales con menos de 8 colores se dibuja solo con grosor, subrayado y vídeo inverso, y el significado nunca depende solo del color: una línea cambiada es `┆`, las carpetas muestran letras de estado y una pestaña muestra `↯` cuando un archivo cambió en disco, y el tirador de la barra de desplazamiento es un raíl `║`, no un bloque invertido.

**Teclas que se pueden encontrar**

- `Space` es la tecla líder. Una pausa sobre un prefijo (`Space`, `g`, `z`, `Z`, `[`, `]` o un operador como `d`) abre una pequeña ventana which-key que lista las teclas que pueden venir a continuación; seguir escribiendo funciona exactamente igual que sin ella.
- `:help` y `F1` abren una lista de todas las teclas y comandos en la que se puede buscar, con lo que hace cada una; `:help tab` encuentra `Tab`, `Shift-Tab` y `<Space>bp`.
- `<Space>bp` pone una letra en cada pestaña, y la letra salta a su archivo; `<Space>bc` hace lo mismo para cerrar una (un archivo con cambios sigue abierto, igual que con `:q`). Una pista nombra `Tab` y `Shift-Tab` la primera vez que hay un segundo archivo abierto, hasta que se han usado tres veces.
- Mientras las coincidencias de una búsqueda están resaltadas, la línea de estado indica en cuál se está y cuántas hay, `[3/12]`.
- `sim --help` imprime el uso y el entorno que lee sim (`NO_COLOR`, `SIM_ON_NORMAL`, `XDG_STATE_HOME`); `sim --version` imprime la versión. Ninguno de los dos toca la terminal.

## sim en una pantalla

| Teclas | Qué hacen |
|---|---|
| `i` | insertar |
| `Ctrl-s` | guardar |
| `ZZ` | guardar y salir |
| `q` | salir |
| `ZQ` | salir sin guardar |
| `u` | deshacer |
| `U` | rehacer |
| `Y` | copiar hasta el final de la línea |
| `/` | buscar hacia adelante |
| `n` | siguiente coincidencia |
| `*` | buscar la palabra hacia adelante |
| `yy` | copiar líneas |
| `]c` | siguiente cambio externo |
| `[c` | cambio externo anterior |
| `:s` | sustituir |
| `:{range}!cmd` | filtrar |
| `:seen` | borrar las marcas de cambio |
| `Ctrl-n` | mostrar u ocultar el árbol |
| `Ctrl-p` | buscar archivos |
| `Ctrl-f` | buscar en el contenido de los archivos |
| `.` | archivos cambiados y recientes (en el árbol) |
| `Tab` | archivo siguiente |
| `Shift-Tab` | archivo anterior |
| `zh` | colores de sintaxis activados/desactivados |
| `zl` | siguiente apariencia |
| `zi` | iconos activados/desactivados |
| `<Space>` | tecla líder |
| `<Space>bp` | elegir una pestaña por letra |
| `<Space>bc` | elegir una pestaña para cerrarla |
| `<Space>bk` | dónde fue la versión que se dejó (`:w!`, `:e!`) |
| `<Space>ge` | ver los cambios respecto a HEAD |
| `<Space>gi` | qué hizo la protección con este repositorio |
| `<Space>um` | vista de lectura de Markdown activada/desactivada |
| `F1` | ayuda |
| `:help` | ayuda |

Todo lo demás es vi: movimientos, operadores, objetos de texto, contadores, `.`, modo Visual, `:s`, rangos. `Y` copia hasta el final de la línea, `U` rehace, y cada yank llega también al portapapeles del sistema mediante OSC 52, también a través de tmux. Al volver a abrir un archivo, el cursor queda donde se dejó. Con el IME activado, las teclas de ancho completo funcionan como comandos; `SIM_ON_NORMAL='fcitx5-remote -c'` desactiva el IME cada vez que se vuelve al modo Normal (`SI_ON_NORMAL`, su nombre antiguo, sigue funcionando). Cada tecla está en una sola tabla, `sim/keys.tsv`, que leen `:help` y which-key y que las pruebas comprueban frente a lo que hacen las teclas. La lista completa está en la [página de documentación](https://sim.efg-technologies.com/es/docs/).

## Modelo de seguridad

Se abren archivos y repositorios que no se han escrito. El atacante controla su contenido, no la máquina del usuario. sim procura que ese contenido no pueda ocultarse, no pueda reconfigurar el editor y no pueda hacer que el editor ejecute código. **No es un sandbox**: se ejecuta con los privilegios del usuario, no detecta letras parecidas de otras escrituras y sus afirmaciones proceden de la especificación y las pruebas del propio proyecto, no de una auditoría independiente. Todavía no hay servidores de lenguaje; hay previsto un cliente en un sandbox. Para informar de una vulnerabilidad, véase la [política de seguridad](SECURITY.md).

El modelo de amenazas completo, amenaza por amenaza, está en la [página de seguridad](https://sim.efg-technologies.com/es/security/). Tres artículos extensos profundizan más: [caracteres ocultos y bidireccionales](https://sim.efg-technologies.com/blog/hidden-characters/), [repositorios que ejecutan código](https://sim.efg-technologies.com/blog/repositories-that-run-code/) y [guardados seguros y secretos](https://sim.efg-technologies.com/blog/safe-saves/).

## Comparación con Vim, Neovim y Helix

Vim, Neovim y Helix son editores maduros y sim es nuevo. Solo se compara lo que dice una fuente primaria; la [página de comparación](https://sim.efg-technologies.com/es/compare/) las cita todas (comprobado el 2026-10-17: Vim 9.2.1202, Neovim 0.12.6, Helix 25.07.1).

| | Vim | Neovim | Helix | sim |
|---|---|---|---|---|
| Extensiones | Vim script, paquetes | Lua, Vim script | ningún sistema de plugins en una versión publicada; el PR de Steel [#8675](https://github.com/helix-editor/helix/pull/8675) es un borrador | ninguna, por diseño |
| Modelines | activadas (desactivadas para root); `'modelinestrict'` desde [9.2.0350](https://github.com/vim/vim/commit/4c2879471) | activadas (desactivadas para root) | no las interpreta ([#729](https://github.com/helix-editor/helix/issues/729)) | nunca se leen |
| Configuración local del proyecto | `'exrc'`, desactivada por defecto | `'exrc'`, lista de confianza | `.helix/config.toml` se carga en la 25.07.1; la confianza (trust) está en la rama de desarrollo | no existe |
| U+202E y similares | `<202e>` | `<202e>` | sin marcador en la 25.07.1 (nuestra prueba; [#11255](https://github.com/helix-editor/helix/issues/11255)) | `<U+202E>` sobre un bloque magenta |

En qué siguen siendo mejores los demás, por ahora: Neovim y Helix incluyen un cliente de servidores de lenguaje, y Vim y Neovim tienen plugins y macros. sim todavía no tiene nada de esto.

## Hoja de ruta (prevista, no publicada)

Registros, macros y marcas de Vim · integración con el portapapeles más allá de OSC 52 · un cliente de servidores de lenguaje en un sandbox · `:tutor` · un árbol de diff de git · una GUI con profundidad real (un prototipo) · paquetes para AUR y crates.io.

## Desarrollo

sim es un programa en dos crates: el **núcleo** (`sim-editor-core`, en `core/`: el búfer, la gramática de vi, los guardados, la fusión con lo que escriben otros procesos, la terminal) y el **editor** (`sim-editor`, en `sim/`: el binario `sim`, con el árbol, la búsqueda de archivos, la búsqueda de texto, las marcas, la sintaxis, las apariencias y el ratón). El porqué de cada decisión está en [CONCEPT.md](CONCEPT.md), junto con la investigación en la que se apoya. Lo que sim debe hacer está en [SPEC.md](SPEC.md): cada requisito tiene un ID, y cada ID tiene pruebas.

La pirámide: [CONCEPT](CONCEPT.md) (por qué) → [SPEC](SPEC.md) (qué, con IDs) → `core/src` y `sim/src` (cómo) → pruebas (evidencia). Los cambios fluyen hacia abajo; la evidencia fluye hacia arriba. [CLAUDE.md](CLAUDE.md) contiene las reglas.

```sh
sh scripts/check.sh          # fmt, clippy, pruebas, trazado SPEC↔prueba, ambos límites de líneas, las dependencias del núcleo, tamaño del binario
sh scripts/e2e.sh            # el núcleo (como `sim --plain`) y el editor en tmux (socket propio)
cargo test --release --workspace -- --ignored bench   # requisitos de rendimiento
```

Pruebas de propiedades, fuzzers con un modelo de no pérdida del texto de cada parte, inyección de fallos en cada paso de un guardado, un oráculo diferencial con nvim y golden files de la pantalla renderizada mantienen honestas las promesas.

## Licencia

MIT OR Apache-2.0, a elección del usuario: [MIT](LICENSE-MIT) o [Apache-2.0](LICENSE-APACHE). Copyright EFG Technologies.
