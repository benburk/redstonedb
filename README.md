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
just rsdb tag 23a9 torchless carry_lookahead
just rsdb export 23a9 --out ~/.minecraft/config/worldedit/schematics
just rsdb remove 23a9
just rsdb tags
just rsdb define-tag carry_lookahead --category other
just rsdb define-tag pipelined --category timing --token pipe
```

- `add` reads the size from the file, inside the corner markers, and rejects a file already stored. It accepts Sponge `.schem` files, versions 2 and 3.
- `add` lints the file against [conventions.md](conventions.md) first and refuses it if it breaks one, unless given `--force`. `just rsdb lint <file or id>` runs the same checks on their own.
- `--synchronized true` or `false` records whether every output bit settles on the same tick; left out, it is unknown.
- `set --unset latency` makes a field unknown again; `--unset` takes `width`, `latency`, `synchronized`, `author`, or `built-on` and repeats.
- Ids can be shortened to any unique prefix.
- Tags must exist before use; `define-tag` creates one. A tag is named in full words joined by underscores, and a timing tag also gets the abbreviated token filenames use. Commands accept a tag by name or by token. Through `just`, write a name with underscores, since `just` splits quoted arguments at spaces.
- Width, latency, author, and build date may be left unknown. The generated filename then omits them.

## Website

The server is built from `template-site`: axum, maud, fixi, and sqlx over the same `redstone.db`.

```sh
cp .env.example .env
just serve
```

- `/` shows every schematic as a card with a thumbnail, its name, tag badges coloured by category, and its latency, size, volume, and author. `/?view=list` shows the same schematics as a table instead.
- `/schematics/<id>/thumbnail.png` draws the build from the south-east and above, one flat colour per block, leaving out the corner markers. It is rendered from the stored file on request; cards link to it with the file's hash in the URL, so browsers cache it until the file changes.
- `/schematics/<id>` shows one schematic's fields, tags, a 3D viewer, and a download link. A logged-in user can add and remove tags there. Below, a table lists the build's inputs and outputs as read from the stored file: each bus or named line, its block, bit count, LSB position, spacing, the side power enters from, whether it needs hard power, and its required strength. Any conventions the file breaks are listed under it.
- The viewer is [schematic-renderer](https://github.com/Schem-at/schematic-renderer), loaded from jsDelivr together with its vanilla resource pack. The pack is about 7 MB and the browser caches it after the first view.
- `/schematics/<id>/download` returns the stored file under its generated filename.
- Accounts are needed only to edit tags. Signing up takes an invite code; `seeds/seed.sql` creates some.
- Styles are Tailwind. `styles/app.css` is the source, and `just css` builds `static/app.css`, which is committed so the server and the Nix build need no Tailwind. `just serve` rebuilds it first, and `just check` fails when it is stale.

After changing a query, run `just prepare` to refresh the offline query metadata in `.sqlx/`.

## Related

- <https://github.com/Schem-at/schematic-renderer>: the viewer.
