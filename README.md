# explore

Browse arbitrary nested data in ranger-style columns.

```sh
cargo run -- examples/sample.json
cat data.json | cargo run -- -          # stdin
cargo run -- data.txt -p json           # force a provider
cargo run -- config.yaml                # EDN/YAML/Transit via jet (see below)
cat data.edn | cargo run -- - --from edn
cargo run -- data.json -c 'enter; bottom'   # run commands on startup
cargo run -- --list-providers
```

Keys: `hjkl`/arrows to move, `gg`/`G` top/bottom, `PgUp`/`PgDn` (or `C-u`/`C-d`) to page,
`R` reload, `:` command line, `q` quit.

Commands (for `:` and `-c`, separated by `;`): `up [n]`, `down [n]`, `move <n>`,
`top`, `bottom`, `enter`, `leave`, `reload`, `quit`.

## EDN, YAML and Transit

Non-JSON input is converted to JSON by [jet](https://github.com/borkdude/jet) before
the app sees it, so the browser itself only knows JSON. jet is not bundled: the
`jet` on your `PATH` is used, and it is only needed when a conversion runs. Plain
JSON never touches it.

The format is picked like this:

1. `-i`/`--from edn|yaml|transit|json` (the same names jet uses), if given.
2. Otherwise the file extension: `.edn`, `.yaml`/`.yml`, `.transit`/`.transit.json`,
   `.json`/`.geojson`.
3. Otherwise JSON. This includes stdin, which has no extension, so piped
   non-JSON input needs `--from`. If JSON parsing fails and you didn't pass
   `--from`, the error says so.

`R` (reload) runs the conversion again. The whole input is converted up front, so
large files take as long as jet needs.

### Conversion caveats

JSON has fewer types than these formats, so some information is lost or changed.
This is what jet 0.7.27 does:

**All formats**

- **Map keys become strings.** Keywords lose their colon (`:a` → `"a"`), namespaced
  keywords keep the namespace (`:ns/k` → `"ns/k"`), and non-string keys are printed
  as EDN (`1` → `"1"`, `[1 2]` → `"[1 2]"`, `nil` → `""`). Keys that become equal
  collide: `{"a" 1 :a 2}` shows only one `a`.
- **Big and exact numbers become floats.** Integers outside the 64-bit range
  (`1N`, Transit `~n`, long YAML integers) and decimals (`1.5M`) are stored as
  64-bit floats and can lose precision. This also happens with plain JSON.
- **NaN and infinity become the strings** `"NaN"` and `"Infinity"`.
- **Dates become ISO-8601 strings in UTC** (`#inst`, Transit `~t`/`~m`, YAML
  timestamps). YAML timestamps also lose fractional seconds.
- **Sets and lists become arrays.** Set order is arbitrary, and a list looks
  exactly like a vector.

**EDN**

- Keywords, symbols and characters all become strings, so `:foo`, `foo`, `"foo"`
  look the same, and so do `\a` and `"a"`.
- Ratios become floats (`1/3` → `0.3333333333333333`); `#uuid` becomes a string.
- Metadata (`^{...}`) is dropped.
- Unknown tagged literals such as `#my/tag [1 2]` are shown as
  `{"tag": "my/tag", "value": [1 2]}`. Without this, jet would fail on them.
- A file with several top-level values (`{:a 1} {:b 2}`) is shown as one
  top-level array.

**YAML**

- YAML 1.1 rules apply. `yes`/`no`/`on`/`off` become booleans, `012` is octal (10),
  `0x1F` is 31, and `1_000` is 1000. If you want strings, quote them.
- Anchors and aliases are expanded, so shared nodes appear as separate copies.
- `!!binary` becomes a base64 string and `!!set` becomes an array. Custom tags
  (`!foo`) are an error.
- Multi-document streams (`---` between documents) are rejected.
- jet sometimes ignores malformed YAML (e.g. an unclosed `[`) and prints nothing.
  The app reports this as "jet produced no output", not as a syntax error.

**Transit**

- Only transit+json is supported (not MessagePack).
- Keywords and symbols become strings. Sets and lists become arrays. `~#cmap`
  keys are printed as EDN.
- Custom tagged values (`["~#point", [1, 2]]`) can't be converted. jet fails on
  them, and so does the app.
- Several top-level values are shown as one top-level array, like EDN.

## Architecture

```
main.rs          CLI, terminal setup, event loop
app.rs           browser state + App::execute(Command); terminal-agnostic
command.rs       Command enum + text parser: the one vocabulary for all actions
keymap.rs        vim-style key specs ("gg", "<C-d>") -> Command
ui.rs            renders App (pure function of state)
theme.rs         Kind -> Style
provider/mod.rs  Provider / ProviderFactory traits, registry, auto-detection
provider/json.rs the JSON plugin
convert.rs       EDN/YAML/Transit -> JSON by shelling out to `jet`
```

Data flow: key → `Keymap` → `Command` → `App::execute` → `Provider` → `ui::draw`.
Key bindings, the `:` prompt and `-c` all produce `Command`s, so a scripting layer
can plug in at the same point.

### Adding a provider

Implement `Provider` (list children of a `NodePath`, preview leaves, format paths)
and `ProviderFactory` (name, `detect`, `open`), then add the factory to
`provider::registry()`. See `provider/json.rs` for the reference implementation.
