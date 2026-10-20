# Linewright

A fast, native text and source-code editor for macOS, written in Rust. The UI is drawn on the GPU with [GPUI](https://www.gpui.rs/), the framework behind the Zed editor.

Inspired by Notepad++. Not affiliated with or endorsed by Don Ho or the Notepad++ project.

The editor engine is written from scratch for this project. It is built on a rope buffer ([ropey](https://crates.io/crates/ropey)), and syntax highlighting comes from [syntect](https://crates.io/crates/syntect) and [two-face](https://crates.io/crates/two-face), which cover about 190 languages.

## Build and run

Requirements: macOS 11 or later, a Rust toolchain (`rustup`), and Xcode or the Command Line Tools (GPUI compiles Metal shaders).

```sh
cargo run --release -- path/to/file.txt        # run directly
packaging/bundle.sh                            # build target/Linewright.app
packaging/bundle.sh --install                  # …and copy it to /Applications
```

The `.app` registers itself for text and source documents, so you can use **Open With → Linewright** in Finder or drop files on the Dock icon. From a terminal:

```sh
open -a Linewright file1 file2
```

The app icon is drawn in `packaging/icon/linewright.svg`. After editing it, run `packaging/make-icon.sh` to regenerate `packaging/AppIcon.icns`.

The first build takes a few minutes because GPUI is large. Later builds are incremental.

## Features

Features are listed by menu.

**File**
- New, Open, Open Folder, Reload from Disk.
- Save, Save As, Save a Copy As, Save All.
- Rename, and Move to Trash (instead of Delete).
- Close, Close All, and Close Others: all but current, all but pinned, to the left, to the right, all unchanged.
- Load and save session files.
- Print (renders through the browser's print dialog).
- Recent files list, Reopen Closed Tab, Open All Recent Files, Clear Recent Files.
- Open Containing Folder (Finder or Terminal), Open in Default Viewer, New Instance.

**Edit**
- Undo, Redo, Cut, Copy, Paste, Delete, Select All.
- Selection anchor (set, then extend), including in column mode.
- Insert date/time (short, long, or a custom format).
- Copy to Clipboard: file path, file name, folder, all names, all paths.
- Indent and outdent.
- Change Case: UPPERCASE, lowercase, Title Case, Title Case (keep capitals), Sentence case, Sentence case (keep capitals), Invert, Random.
- Line Operations:
  - Duplicate, delete, move up or down, transpose, join, split.
  - Remove duplicate, consecutive-duplicate, empty, or blank lines.
  - Insert a blank line above or below.
  - Reverse or randomize line order.
  - Sort ascending or descending: lexicographic, case-insensitive, integer, decimal (dot or comma), or by length. With a column selection, lines sort by the selected columns.
- Comments: toggle, comment lines, uncomment lines, block comment, block uncomment. Comment tokens are set per language.
- Line endings: convert to CRLF, LF or CR.
- Whitespace: trim trailing, leading or both; join lines with spaces; convert tabs to spaces and back (all or leading).
- Selection: Open File, Search on Internet.
- Multiple carets: add next occurrence, select all occurrences, add caret above or below.
- Insert in Column (text, or numbers in dec/hex/oct/bin with leading zeros or spaces).
- Character Codes and Clipboard History panels.
- Read-Only, and Make File Writable.
- Overwrite mode (INS/OVR).

**Search**
- Find, Replace, Find in Files and Mark, all in one dialog with tabs.
- Search modes: Normal, Extended (`\n \r \t \0 \xHH \uHHHH …`), and Regular expression, with optional `.` matches newline. Backreferences and lookaround work.
- Options: Match whole word, Match case, Wrap around, In selection, direction.
- Replacement templates: `$1`, `\1`, `${name}`, `$&`, plus case changes with `\U … \E`, `\L`, `\u`, `\l`.
- Actions: Count, Find All in Current or All Opened Documents, Replace All (one undo step), Replace All in All Opened Documents.
- Find in Files: folder, filters such as `*.rs *.toml !*.min.js`, sub-folders, hidden folders. Replace in Files is also available. The search runs in the background and can be stopped.
- Search Results panel: a collapsible tree with highlighted matches. **F4 / ⇧F4** jump to the next or previous result.
- Mark All, with options to bookmark the line and purge marks; also Clear Marks and Copy Marked Text.
- Find Next and Previous, Select and Find Next/Previous, Find Next/Previous Occurrence of Word, Incremental Search.
- Go To (line or offset), Go to Matching Brace, Select All In-between braces.
- Highlight All Occurrences and Highlight This Occurrence (5 colors), Clear Highlights, Next/Previous Highlight, Copy Highlighted Text.
- Bookmarks: toggle, next, previous, clear, cut/copy/remove bookmarked lines, replace them with the clipboard, keep only bookmarked lines, invert.
- Find Characters by Code.

**View**
- Always on Top, Full Screen, Floating Note, Focus Mode.
- View Current File in Safari, Chrome, Firefox or the default browser.
- Show Invisibles: spaces and tabs, line endings (CRLF/LF/CR boxes), all invisibles, indent guides, wrap markers.
- Zoom (also ⌘ + scroll wheel) and Word Wrap.
- Split view: Move or Clone to Other View, Focus Other View, rotate the split, draggable splitter.
- Synchronized vertical and horizontal scrolling.
- Tabs: go to tab 1–8 or last, next/previous, move forward/backward, sort, pin. Tabs can be dragged to reorder or between views, closed with a middle-click, and have a "list all tabs" menu.
- Folding: fold or unfold all, current level, or level 1–8, with a fold margin. Hide Lines (with markers to unhide).
- Document Statistics: characters, words, lines, bytes and selection.
- Panels: Project Folder, Minimap, Open Documents, Function List.
- Follow File Changes (tail -f).
- Toggles for the toolbar, status bar, tab bar, line numbers, bookmark margin, fold margin, change history, current line highlight, vertical edge, and smart highlighting.

**Encoding**
- ANSI, UTF-8, UTF-8-BOM, UTF-16 BE BOM, and UTF-16 LE BOM, each available to reinterpret the file or to convert it.
- About 35 character sets, grouped by region.
- Detection uses the BOM, UTF-8 validation, a UTF-16 heuristic, and statistical detection.

**Language**
- About 190 languages, grouped A–Z, detected from the file name, extension, or the first line (shebang).

**Settings**
- Preferences: General, Editing, Margins & Display, New Document, Highlighting, Auto-Completion, Backup & Session, Searching, Misc.
- Themes: about 25, including **Classic**, Basalt, Zenburn, Monokai, Solarized and Dracula.
- Keyboard Shortcuts (a searchable list). Custom shortcuts go in `keymap.json` and are reloaded when you save it.

**Tools**
- MD5, SHA-1, SHA-256 and SHA-512: hash typed text, files, or the selection (copied to the clipboard).

**Macro**
- Start/Stop Recording, Playback, Save Recorded Macro, Run Macro Repeatedly (N times or until end of file), Trim Trailing Whitespace and Save.
- Saved macros appear in the menu.

**Run**
- Run… shell commands with `$(FULL_CURRENT_PATH)`, `$(CURRENT_DIRECTORY)`, `$(FILE_NAME)`, `$(NAME_PART)`, `$(EXT_PART)`, `$(CURRENT_WORD)`, `$(CURRENT_LINE)`, `$(CURRENT_COLUMN)`.
- Launch in a browser, Open Terminal Here.

**Extras**
- Encode / Decode: Base64 and URL.
- Hex: ASCII ↔ HEX.
- JSON format and minify.
- XML pretty print.
- **Compare**: line diff of the two views, with red and green line highlights.

**Window**
- Windows… dialog and a document list with checkmarks.

**Editor engine**
- Multiple carets: ⌘-click to add a caret, ⌘⌥↑/↓ to add one above or below. Rectangular selection with ⌥-drag or ⌥⇧↑/↓. Pasting several lines into the same number of carets puts one line at each caret.
- Smart highlighting, brace matching, and HTML/XML matching-tag highlighting.
- Change History margin: orange marks unsaved edits, green marks saved ones.
- Auto-indent (smart about `{` and `:`), auto-close brackets, quotes and HTML tags.
- Word completion from the document plus language keywords (**⌃Space** or **⌃Enter**).
- Control characters are shown as boxes (`NUL`, `ESC`, …).
- Large files: a 70 MB, 1,000,000-line file opens, scrolls, wraps and edits interactively. Highlighting is incremental and only covers what is on screen; it turns off above 64 MB.
- Line endings are kept exactly as they are in the file (CR LF, LF or CR), including mixed endings.

**Session and safety**
- Session snapshot and periodic backup: unsaved and untitled documents survive quitting, along with carets, folds, bookmarks, panels, split and window size. With snapshots turned off, you are asked to save on quit.
- Detects files changed by other programs and offers to reload; can reload unmodified files silently.
- Native macOS menu bar with real checkmarks, native open/save dialogs and prompts, and a title bar that follows the theme (light or dark).

## Keyboard shortcuts

Shortcuts follow familiar Windows-editor conventions, mapped to ⌘ where macOS allows. Some differ where macOS already reserves the key, for example ⌘H (Hide) or ⌘Q (Quit).

| Action | Shortcut |
|---|---|
| New / Open / Save | ⌘N / ⌘O / ⌘S |
| Save As / Save All | ⌘⇧S / ⌘⌥S |
| Close / Close All | ⌘W / ⌘⇧W |
| Restore recent closed | ⌘⇧T |
| Find / Replace / Find in Files / Mark | ⌘F / ⌘⌥F / ⌘⇧F / ⌘⇧M |
| Find Next / Previous | ⌘G / ⌘⇧G (also F3 / ⇧F3) |
| Select and Find Next / Previous | ⌘F3 / ⌘⇧F3 |
| Incremental search | ⌘⌥I |
| Go To line | ⌘L (also ⌃G) |
| Next / previous search result | F4 / ⇧F4 |
| Go to matching brace | ⌘B |
| Duplicate line / Delete line | ⌘D / ⌘⇧L |
| Move line up / down | ⌃⇧↑/↓ (also ⌘⌃↑/↓) |
| Join lines / Split lines | ⌘J / ⌘I |
| Toggle comment / Block comment | ⌘/ / ⌘⌥/ |
| UPPERCASE / lowercase | ⌘⇧U / ⌘U |
| Column select | ⌥⇧↑/↓ (then ⌥⇧←/→), ⌥-drag |
| Toggle / next / previous bookmark | ⌘F2 / F2 / ⇧F2 |
| Fold all / Unfold all | ⌥0 / ⌥⇧0 |
| Fold / unfold level N | ⌥N / ⌥⇧N |
| Zoom in / out / reset | ⌘= / ⌘- / ⌘0 |
| Next / previous tab | ⌃Tab / ⌃⇧Tab, ⌘⇧] / ⌘⇧[ |
| Go to tab N | ⌘1 … ⌘9 |
| Focus other view | F8 |
| Word completion | ⌃Enter / ⌃Space |
| Record / play macro | ⌃⇧R / ⌃⇧P |
| Run | F5 |
| Full screen / Floating Note | ⌃⌘F, F11 / F12 |
| Preferences | ⌘, |

To customize shortcuts, choose **Settings → Edit keymap.json** and map a keystroke to an action, for example `"ctrl-q": "editor::ToggleLineComment"`. **Settings → Shortcut Mapper** lists every action and its current binding.

## Where things are stored

`~/Library/Application Support/Linewright/` holds:

- `config.json`: preferences.
- `session.json`: open tabs, carets, folds, bookmarks and layout.
- `state.json`: recent files, find history, saved macros.
- `keymap.json`: custom shortcuts.
- `backup/`: snapshots of unsaved documents.

## Architecture

```
src/
  buffer.rs        rope text, undo/redo transactions, bookmarks, marks, change history, file I/O
  encoding.rs      encoding detection, decoding and encoding, EOL handling
  syntax.rs        syntect integration with checkpointed incremental highlighting,
                   comment tokens, function-list patterns, completion keywords
  search.rs        Normal/Extended/Regex engine, replacement templates, Find in Files
  tools.rs         case conversion, line operations, hashes, Base64/URL/hex, JSON/XML, line diff
  editor/          the text editor view
    display.rs     tab expansion, control-char boxes, soft wrap, folding, line↔row map
    element.rs     custom GPUI element: layout, painting, mouse handling
    movement.rs    caret movement, column selection, multi-cursor, brace/tag matching
    editing.rs     all editing commands, clipboard, completion
    input.rs       macOS text input / IME
  workspace/       the window: tabs, split views, panels, dialogs, find dialog, results
  menus.rs         native menu bar
  keymap.rs        default shortcuts and keymap.json loading
  session.rs       session, backups and app state persistence
  platform.rs      small AppKit integrations (menu checkmarks, window level, appearance)
```

## Tests

```sh
cargo test
```

The suite has 54 tests. Besides unit tests, it includes headless end-to-end tests built on GPUI's test platform. They drive the real editor and workspace with simulated keystrokes and typing, covering:

- typing and undo
- multi-cursor and column editing
- folding and word wrap
- find, replace, Find All, Mark and Find in Files
- tabs and split/clone views
- save prompts and Save As
- encodings and EOL conversion
- macros
- session snapshot round-trips
- Compare
- Insert in Column

There is also a development hook that runs a script against the app and saves PNG snapshots of its own window, for UI checks:

```sh
LINEWRIGHT_DEBUG_SCRIPT="sleep 1000; action workspace::Find; snap /tmp/find.png; quit" cargo run -- file.rs
```

Script commands: `sleep`, `action`, `key`, `type`, `open`, `resize`, `snap`, `menudump`, `ctxmenu`, `mark`, `quit`.

## Third-party notices

`THIRD_PARTY_LICENSES.md` collects the license notices of every bundled dependency, the toolbar icons (Lucide) and the bundled syntax definitions and color themes. The app shows it under **Help → Third-Party Licenses**, and the `.app` bundle includes it in `Contents/Resources`.

Regenerate it after changing dependencies:

```sh
cargo run --example export_themes         # only if the bundled theme list changes
packaging/third-party-licenses.sh         # needs: cargo install --locked cargo-about --features cli
```

`deny.toml` holds the license policy for `cargo deny check licenses`.

## Not included yet

- User-defined languages
- A plugin system (common tools are built in instead: Base64/URL encoding, hex conversion, JSON, XML, Compare)
- API-based function call tips
- Spell checking
- Drag-and-drop of text inside the editor
- Native printing (Print goes through the browser)

Folding is based on indentation rather than on each language's grammar.

## Support

If this app is useful to you, you can buy me a beer. Thank you! 🍺

[![Buy me a beer](https://img.buymeacoffee.com/button-api/?text=Buy%20me%20a%20beer&emoji=%F0%9F%8D%BA&slug=sonatina_dream&button_colour=FFDD00&font_colour=000000&font_family=Cookie&outline_colour=000000&coffee_colour=ffffff)](https://buymeacoffee.com/sonatina_dream)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

