# explore

Browse arbitrary nested data in ranger-style columns.

```sh
cargo run -- examples/sample.json
cat data.json | cargo run -- -          # stdin
cargo run -- data.txt -p json           # force a provider
cargo run -- data.json -c 'enter; bottom'   # run commands on startup
cargo run -- --list-providers
```

Keys: `hjkl`/arrows to move, `gg`/`G` top/bottom, `PgUp`/`PgDn` (or `C-u`/`C-d`) to page,
`R` reload, `:` command line, `q` quit.

Commands (for `:` and `-c`, separated by `;`): `up [n]`, `down [n]`, `move <n>`,
`top`, `bottom`, `enter`, `leave`, `reload`, `quit`.

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
```

Data flow: key → `Keymap` → `Command` → `App::execute` → `Provider` → `ui::draw`.
Key bindings, the `:` prompt and `-c` all produce `Command`s, so a scripting layer
can plug in at the same point.

### Adding a provider

Implement `Provider` (list children of a `NodePath`, preview leaves, format paths)
and `ProviderFactory` (name, `detect`, `open`), then add the factory to
`provider::registry()`. See `provider/json.rs` for the reference implementation.
