# Deploying

The site runs on the Hetzner server (Ubuntu, aarch64) at <https://redstonedb.benburk.ca>, as the `redstonedb` service behind Caddy.

`just deploy` runs the tests, then ships the committed HEAD:

1. It builds on the server; a failed build leaves the site as it was.
2. It backs up the live database, since the new binary applies any new migrations as it starts.
3. It installs the binaries in `/usr/local/lib/redstonedb`, keeping the old ones as `*.previous`, and the unit files here, then restarts the site.
4. It waits for the site to answer, and prints its log if it does not. `/usr/local/lib/redstonedb/DEPLOYED` names the commit running.

To roll back, move each `*.previous` binary back over its new one and `systemctl restart redstonedb`. If the bad deploy migrated the database, the old binary refuses it; with the site stopped, restore the copy taken before the deploy, `redstone-<date>-before-<commit>.db` in `/var/backups/redstonedb`, with `runuser -u redstonedb -- sqlite3 /var/lib/redstonedb/redstone.db ".restore <copy>"`.

`just logs` follows the site's log, and `just status` shows the live commit, the service, and the latest backups.

`just pull-db` replaces the local `redstone.db` with a copy of the live one, backing the local one up to `backups/` first. The live database is the real one: changes to data belong there, through `rsdb` on the server. Run commands against the live database as the site's user, never as root, which could leave files beside it the site cannot open.

| Path                      | What                                    |
| ------------------------- | --------------------------------------- |
| `/var/lib/redstonedb`     | The database, owned by `redstonedb`     |
| `/etc/redstonedb.env`     | Settings and the Discord secret, `0640` |
| `/var/backups/redstonedb` | Daily copies, kept for 14 days          |
| `/etc/caddy/Caddyfile`    | `deploy/Caddyfile`, installed on deploy |

On the server, `rsdb` runs against the live database as the site's user, as in `rsdb role <name> admin`. `journalctl -u redstonedb` has the log.

## One-time setup

Done once, already, on the current server:

1. `apt-get install build-essential pkg-config cmake curl`, then rustup's minimal stable toolchain for root.
2. `useradd --system --home-dir /var/lib/redstonedb --shell /usr/sbin/nologin
   redstonedb`, and `/var/lib/redstonedb` and `/var/backups/redstonedb` owned by it, mode `750`.
3. `/etc/redstonedb.env`, owned `root:redstonedb`, mode `640`: `DATABASE_URL=sqlite:/var/lib/redstonedb/redstone.db`, `PORT=3100`, `RUST_LOG`, and the three `DISCORD_` settings, the redirect being `https://redstonedb.benburk.ca/auth/discord/callback`. That redirect is also listed in the Discord Developer Portal.
4. The database copied in with `sqlite3 .backup`, never `cp`, which misses what is still in the WAL.
5. `/etc/caddy/Caddyfile` is `deploy/Caddyfile`, the server's only site, then `systemctl reload caddy`. DNS for the name points at the server: a plain A record on benburk.ca, beside the dynamic ones crimson keeps current.
6. SSH takes keys only (`/etc/ssh/sshd_config.d/00-hardening.conf`); the Hetzner console is the way in if every key is lost.
7. `ufw` lets in only 22/tcp, 80/tcp, and 443 over TCP and UDP; the site's own port, 3100, is reachable only through Caddy.
