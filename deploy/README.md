# Deploying

The site runs on the Hetzner server (Ubuntu, aarch64) at <https://redstonedb.benburk.ca>, as the `redstonedb` service behind Caddy.

`just deploy` ships the committed HEAD: it builds on the server, installs the binaries in `/usr/local/lib/redstonedb`, installs the unit files here, and restarts the service. The service applies any new migrations as it starts.

| Path                      | What                                    |
| ------------------------- | --------------------------------------- |
| `/var/lib/redstonedb`     | The database, owned by `redstonedb`     |
| `/etc/redstonedb.env`     | Settings and the Discord secret, `0640` |
| `/var/backups/redstonedb` | Daily copies, kept for 14 days          |
| `/etc/caddy/Caddyfile`    | Has the block in `deploy/Caddyfile`     |

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
