# CLAUDE.md

## Pulling a build from Schemati

When asked to look at a file in Ben's Schemati, such as `temp.schem`, ask for a download link: `//download temp` in game gives one like `http://schemati.openredstone.org/download?file=TScdv9IuhGt1TPRk`, which anyone can open for seven days without logging in. Fetch it into a new directory in the scratchpad, then copy it where it's needed, such as `tests/fixtures/`:

```sh
curl -sS -o temp.schem "https://schemati.openredstone.org/download?file=TScdv9IuhGt1TPRk"
```

A file over 200 bytes that `file` calls gzip is the schematic; anything else is Schemati's error page for an expired link. Check it with `just rsdb inspect temp.schem` and `just rsdb outputs temp.schem`, which probes it as each category.

A player's folder itself opens only with their Schemati session, which the live site holds in `user.schemati_session` and `just setup live` leaves out of its copy. Reading it means a production read over SSH, which the permission check refuses, so ask for the link instead.
