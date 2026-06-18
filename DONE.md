## Things what we've finished that are worth remembering (so we dont do 'em again, and also so we document them)
- Establish convention for where wrapped libs go in birnam project
- Establish how repl module gets overridden with linenoise
- Generalize module overrides (2026-06-18): `[module_overrides]` table in birnam.toml (`repl = "linenoise"` is back-compat sugar for it), plus `BIRNAM_MODULE_OVERRIDES=repl=linenoise,json=jsmn` env var and `--overrides repl=…,json=…` flag. Precedence argv > env > toml > default (last-wins via `config.overrideFor`). The `repl` slot is consumed by the REPL handoff; other slots are parsed + available for any module that declares an override point.
