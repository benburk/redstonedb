# redstone-database

A collection of Minecraft redstone schematics, named so they can be sorted, searched, and parsed.

- [naming.md](naming.md): the filename scheme.
- [glossary.md](glossary.md): function names, modifiers, and their abbreviations.
- [conventions.md](conventions.md): how builds are laid out in-world, and the block palette.
- [metadata.md](metadata.md): candidate fields not yet in the database.
- [features.md](features.md): planned whitelist, linter, and export options.

Each part of the scheme is marked either decided or open.

## rsdb

`rsdb` is a command-line wrapper around a SQLite database, `redstone.db`. The database is the source of truth: it holds each `.schem` file, its fields, and its tags. Filenames are generated from it on export. `migrations/` defines the tables; the server and the CLI apply pending migrations when they open the database.

```sh
just rsdb add adder.schem --function adder --orientation vert --encoding bin \
    --width 8 --latency 3 --author cullback --built-on 2026-10-07 --tag alpha
just rsdb list --tag alpha
just rsdb show 23a9
just rsdb set 23a9 --latency 4
just rsdb tag 23a9 torchless
just rsdb export 23a9 --out ~/.minecraft/config/worldedit/schematics
just rsdb tags
just rsdb define-tag <name> --category other
```

- `add` reads the size from the file and rejects a file already stored. It accepts Sponge `.schem` files, versions 2 and 3.
- Ids can be shortened to any unique prefix.
- Tags must exist before use; `define-tag` creates one.
- Width, latency, author, and build date may be left unknown. The generated filename then omits them.

## Website

The server is built from `template-site`: axum, maud, fixi, and sqlx over the same `redstone.db`.

```sh
cp .env.example .env
just serve
```

- `/` lists every schematic in a table, named from its width, restriction tags, other tags, and function.
- `/schematics/<id>` shows one schematic's fields, a 3D viewer, and a download link.
- The viewer is [schematic-renderer](https://github.com/Schem-at/schematic-renderer), loaded from jsDelivr together with its vanilla resource pack. The pack is about 7 MB and the browser caches it after the first view.
- `/schematics/<id>/download` returns the stored file under its generated filename.
- The template's accounts, sessions, and invite codes are kept for later upload support but are not required to browse.

After changing a query, run `just prepare` to refresh the offline query metadata in `.sqlx/`.

## Related

- <https://github.com/Schem-at/schematic-renderer>: the viewer.
