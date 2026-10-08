# redstone-database

A collection of Minecraft redstone schematics, named so they can be sorted, searched, and parsed.

- [naming.md](naming.md): the filename scheme.
- [glossary.md](glossary.md): category names, modifiers, and their abbreviations.
- [conventions.md](conventions.md): how builds are laid out in-world, and the block palette.
- [metadata.md](metadata.md): candidate fields not yet in the database.
- [features.md](features.md): planned lint checks and export options.

Each part of the scheme is marked either decided or open.

## rsdb

`rsdb` is a command-line wrapper around a SQLite database, `redstone.db`. The database is the source of truth: it holds each `.schem` file, its fields, and its tags. Filenames are generated from it on export; see [naming.md](naming.md). `migrations/` defines the tables, squashed into one schema on 2026-10-08; the server and the CLI apply pending migrations when they open the database.

`redstone.db` is not in git, so back it up on its own. A fresh checkout starts with no database; the CLI or the server creates an empty one on first use. `rsdb` writes every change into the file before it exits, so copying `redstone.db` alone is a complete backup.

```sh
just rsdb add adder.schem --category adder --orientation vert --encoding bin \
    --width 8 --latency 3 --builder cullback --built-on 2026-10-07 --tag alpha
just rsdb list --tag alpha
just rsdb search register vertical alpha -torchless
just rsdb show 23a9
just rsdb set 23a9 --latency 4
just rsdb tag 23a9 torchless carry_lookahead
just rsdb export 23a9 --out ~/.minecraft/config/worldedit/schematics
just rsdb remove 23a9
just rsdb add slice.schem --category adder --orientation horz --encoding bin \
    --width 1 --stack west=2,max=7 --stack north=4,max=3
just rsdb export 23a9 --copies west=7,north=3 --out .
just rsdb tags
just rsdb define-tag carry_lookahead --kind other
just rsdb define-tag pipelined --kind timing --token pipe
```

- `add` takes any glass corner markers out of the file and fits its box tightly, as uploads do; see [conventions.md](conventions.md). It reads the size from that box, leaving out any outer layer of nothing but the interface, and rejects a file already stored. `rsdb reread` reshapes and measures every stored build again after the rules change. It accepts Sponge `.schem` files, versions 2 and 3.
- `add` lints the file against [conventions.md](conventions.md) first and refuses it if it breaks one, unless given `--force`. `just rsdb lint <file or id>` runs the same checks on their own.
- `--standardize` brings a downloaded build into the palette before importing it. Slabs, stairs, and glass become white stained glass, and every other solid block becomes white concrete, keeping redstone components, inputs, and outputs. Old corner markers, and any other plain glass touching nothing but air, are removed, and the box is fitted tightly. Inputs (levers on lime, light blue, or magenta blocks) and outputs (yellow concrete or yellow stained glass) must already be marked, since they are what survives.
- `--conventional false` marks a build, such as a display, that follows its own layout. `add` skips the lint for it, and its page does not read inputs and outputs from it. It is measured by its whole box.
- `--stack` marks a build as a stackable slice, once for each direction it stacks along: how far each copy sits `west`, `up`, or `north` of the one before, and optionally the `max` copies that work. Giving `--stack` to `set` replaces the stored directions; `--unset stacking` makes a build unstackable. `export --copies` writes the copies joined into one schematic, as [conventions.md](conventions.md) describes: a plain count for a build that stacks one way, or a count per direction such as `west=7,north=3`. `export --markers` puts a glass block off each corner, for WorldEdit to select the build by.
- `--synchronized true` or `false` records whether every output bit settles on the same tick; left out, it is unknown.
- `set --unset latency` makes a field unknown again; `--unset` takes `width`, `latency`, `synchronized`, `builders`, `contributors`, or `built-on` and repeats.
- Ids can be shortened to any unique prefix.
- A build's category is a standard one, such as `adder`, or any other name; every build also has a design, `other` unless its category lists one such as `carry_lookahead`. `rsdb categories` shows the standard categories with their designs and flags, and `define-category`, `define-design`, and `define-flag` add to them. A flag is a yes-or-no fact a category's builds record, such as an adder's carry in or a shift register being bidirectional; the details form shows the chosen category's as tick boxes, a build's page lists the ones it has, and search finds them, as in `bidirectional` or `cin`. `--design` sets the design and `--flag carry_in`, repeated, the flags; `add` also sets the carry flags the file's signs show.
- Tags must exist before use; `define-tag` creates one. A tag is named in full words joined by underscores, and may also get a short token. Commands accept a tag by name or by token. A name with spaces needs quotes, as in `just rsdb tag 23a9 "carry lookahead"`.
- `--builder` records an in-game name of someone who built it, and repeats for a build with several builders, kept in the order given. Giving `--builder` to `set` replaces the stored builders. `--contributor` does the same for people who helped without building it, such as with the design.
- Width, latency, builders, and build date may be left unknown. The generated filename omits an unknown width.
- `--description` stores free text about the build, such as how to use it. The build's page shows it under the title, and search matches words in it.
- `--imported-by` records the in-game name of whoever imported the build. It is shown on the site.

## Website

The server is built from `template-site`: axum, maud, and sqlx over the same `redstone.db`. Pages are plain server-rendered HTML with forms; `static/form.js` only enhances the details form.

```sh
cp .env.example .env
just serve
```

- `/?q=register+vertical+alpha+-torchless` narrows the list to builds matching every word. A word names a category, orientation, encoding, tag, width such as `8b`, builder, contributor, or importer, `stackable`, or `sync` or `unsync`; a word that names none of those matches part of a build's name instead. `-` or `not` before a word leaves those builds out, and quotes or underscores keep a multi-word tag together. `rsdb search` takes the same words.
- `/` shows every schematic as a card with a thumbnail, its name, tag badges coloured by kind, and its latency, size, volume, and builders. `/?view=list` shows the same schematics as a table instead.
- A sidebar lists each category with how many builds match the search, such as Adders and Multipliers; `/?category=adder` shows only that category, and every category off the standard list is grouped under Other, `/?category=other`. Separate parameters narrow together, and commas within one mean any of them, as in `/?category=adder,subtractor`. `/?tag=alpha` shows builds with that tag; `tag` may repeat, each needing a match, so `/?tag=alpha&tag=torchless` is both and `/?tag=alpha,torchless` either. `/?sort=` takes fields separated by commas, each with `-` for descending: `added_at`, `volume`, `latency`, and `downloads`, as in `/?sort=-downloads,volume`. The default is newest first, ties fall back to it, and an unknown latency goes last either way. The page offers Newest, Volume (smallest first), Latency (fastest first), and Downloads (most first). The links keep the search, category, sort, and layout together. Tags link to their tag filter, and builders, contributors, and a build page's details link to a search for them; a name searches for it in its role, as in `builder:embizone`, `contributor:alex_you`, or `importer:lookaside`, and a build's category links to that category's list. Categories are listed alphabetically. A category such as Bussing lets each build give its own name, which the details form asks for and the build's title uses, as in 8-bit Bus Crossing. Each standard category stores its plural, such as `ALUs`; `rsdb define-category --plural` sets it, and otherwise it is the label with an `s`.
- `/schematics/<id>/thumbnail.png` draws the build from the south-east and above, one flat colour per block. It is rendered from the stored file on request; cards link to it with the file's hash in the URL, so browsers cache it until the file changes.
- `/schematics/<id>` shows one schematic's tags, Download button, and fields on the left and a 3D viewer on the right. Below, a table lists the build's inputs and outputs as read from the stored file: each bus or named line, its bit count, its required strength, and whether it needs hard power. Any conventions the file breaks are listed under it.
- `/schematics/<id>/edit` is where a mod or admin edits a build's details and tags, laid out like the upload preview. The build's page links to it with an Edit button.
- The viewer is [schematic-renderer](https://github.com/Schem-at/schematic-renderer), loaded from jsDelivr together with its vanilla resource pack. The pack is about 7 MB and the browser caches it after the first view.
- `/schematics/<id>/download` returns the stored file under its generated filename. A build's page also shows when it was uploaded and, once a person has changed its details on the edit page or with `rsdb set`, when it was last edited. Each download is counted and shown on the build's page; the 3D viewer fetches the file with `?preview=yes`, which is not counted, and neither is a HEAD request. For a stackable build, a count per direction, such as `?west=7&north=3`, returns the copies joined, and `?markers=yes` adds a glass block off each corner for WorldEdit. The page's Download button opens a dialog with both choices.
- `/upload` lets anyone logged in upload a build, theirs or someone else's: choose a file, optionally standardize it, check the preview of what the site reads from it, fill in its details and tags, and publish. Uploads go live at once.
- The details form's Stacking section marks a build stackable: each row is a direction (west, up, north, or west and up), the blocks from one copy to the next, and optionally the most copies. A second row is for a build that also stacks another way. The first row starts from the build's size the way its bits run, and saving stacks two copies each way and refuses a step whose copies collide.
- Builders, contributors, and tags are typed as pills: pick a suggestion, or type a new name and press Enter, and × removes one. New tags are of the `other` kind. The build date has a "Don't know" box. Synchronized is a toggle, on for a new upload; for a stored build where it is unknown, the toggle shows a dash and saving leaves it unknown until it is clicked. Without scripting, the boxes take one value each and the server checks everything again.

### Accounts

People log in with Discord; an account is made at their first login. Each account has a role:

- **user** uploads builds. Anyone logged in can give their Minecraft name on their profile, at `/users/<username>`; the profile then lists the builds credited to it as builder, then as contributor, above the builds the account uploaded. One name belongs to one account, whatever its case.
- **mod** also edits any build's details and tags.
- **admin** also deletes builds, from the Delete button on a build's page, and sets roles, at `/users`.

Discord login needs `DISCORD_CLIENT_ID`, `DISCORD_CLIENT_SECRET`, and `DISCORD_REDIRECT_URI` in `.env`; see `.env.example`. The redirect must also be listed on the app's OAuth2 page in the Discord Developer Portal. Without them the site still runs, with login turned off.

The first admin is made from the command line, after they have logged in once:

```sh
just rsdb users
just rsdb role <discord username> admin
```

- Styles are Tailwind. `styles/app.css` is the source, and `just css` builds `static/app.css`, which is committed so the server and the Nix build need no Tailwind. `just serve` rebuilds it first, and `just check` fails when it is stale.

After changing a query, run `just prepare` to refresh the offline query metadata in `.sqlx/`.

## Related

- <https://github.com/Schem-at/schematic-renderer>: the viewer.
