# Leafpress

A full-stack Rust blog engine — **Leptos** (SSR + WASM hydration) + **Axum** + **SQLite**, shipped as a single self-updating binary. The server never compiles anything.

- Markdown as content — hot-reload on every change, no restart
- `/admin` as a pure git console: status, auto pull, per-file staging, commit+push, dual-pane diff with inline editing
- Comments / likes in a single SQLite file; light/dark theme toggle
- Content decoupled from the program: `content_dir` points at any directory (typically a separate private repo)

## Local development

Requires the Rust toolchain, `cargo-leptos`, and the `wasm32-unknown-unknown` target.

```bash
cp config.example.toml config.toml    # set admin_password
git clone <content-repo> content      # your content repo; or start with an empty content/
cargo leptos watch
```

Runs at http://127.0.0.1:3000 (admin at `/admin`).

**config.toml** (gitignored; only `admin_password` is required). Lookup order: `$LEAFPRESS_CONFIG` → `./config.toml` → `~/.config/leafpress/config.toml` (`$XDG_CONFIG_HOME` respected); `init` always writes `./config.toml`:

| Key | Default | Purpose |
|---|---|---|
| `site_name` | `"My Blog"` | site title |
| `content_dir` | `"content"` | where the markdown lives |
| `database` | `"site.db"` | comments/likes database |

Relative `content_dir` / `database` paths resolve against the **config file's directory**, so the systemd service and the `leafpress` CLI see the same paths no matter where they run from (a relative `./config.toml` — the local dev case — keeps resolving against the working directory).
| `content_pull_interval_secs` | `300` | built-in `git pull --ff-only` on the content dir; `0` disables |
| `git_proxy` | unset (direct) | proxy for git fetch/pull/push, e.g. `"http://127.0.0.1:7890"` |

The listen address is set by `[package.metadata.leptos] site-addr` in `Cargo.toml` (or the `LEPTOS_SITE_ADDR` env).

**CLI** (the same binary):

```bash
leafpress            # = serve: what the systemd unit runs
leafpress init       # interactive setup: write config.toml, prepare the content dir
leafpress doctor     # read-only health check: config / content / git sync / database / embedded assets
leafpress passwd     # change the admin password (takes effect after restart)
leafpress update     # self-update: download the latest release, replace itself, restart the service
```

## Server deployment

Release flow: `git push` → CI builds dual-arch releases (static assets embedded) → the server updates itself.

Install (as root, interactive prompts; re-running is a pure upgrade — config / content / database are never touched, and it also repairs the systemd unit and git safe.directory if they are missing):

```bash
curl -fsSL https://raw.githubusercontent.com/YXalix/leafpress/main/deploy/install.sh | sudo bash
```

Upgrade:

```bash
sudo leafpress update
```

For SSH content remotes (`git@host:…`), the installer also seeds SSH for the `leafpress` service user (its `$HOME` is the install dir): the host key is copied from root's `known_hosts` (or scanned, printing the fingerprint to verify against your git host's published one), and root's key pair is reused when present — otherwise a new key is generated and printed; add it as a deploy key **with write access** on your git host, or pull/push will fail.

The installer drops a `leafpress` wrapper into `/usr/local/bin` (already on PATH — no PATH edits needed); it pins `LEAFPRESS_CONFIG=/opt/leafpress/config.toml` so the CLI works from any directory.

All state lives in explicit locations and survives reinstalls:

| State | Location |
|---|---|
| `config.toml` | `/opt/leafpress/config.toml` (written once, never overwritten) |
| database | `/opt/leafpress/data/site.db` |
| content | the directory chosen at install time (e.g. `/srv/leafpress-content`) |

Run `sudo leafpress doctor` afterwards to verify.

## Content

Content is just a directory of `.md` files in its own private repo, flowing one way: local → GitHub → server.

`posts/my-post.md`:

```markdown
---
title: "My Post"
date: "2026-01-01 12:00"
tags: [rust]
category: "notes"
status: published    # anything else stays out of the frontend
---

Body in GFM: tables, task lists, code blocks all work; `$...$` / `$$...$$` math renders via KaTeX (server-side, no client JS).
```

- `posts/**/*.md` → `/posts/<slug>`, `pages/**/*.md` → `/pages/<slug>`, `images/foo.png` → `/images/foo.png`
- Subdirectories are for organization only — URLs stay flat, slugs are globally unique; `category` defaults to the folder name

**Server auto-sync**: at install time choose "clone git repo" as the content source. The server fast-forwards the content dir itself every `content_pull_interval_secs` (default 300, `0` disables) and hot-reloads — no cron job needed. If a sync fails (host key verification, unreachable remote, …), the timestamped error is surfaced in the /admin console instead of only landing in the service log.

**/admin git console**: /admin is git-only — branch/ahead/behind status (remote refs refreshed in the background, so a dead network never stalls page load), a manual **pull** button (applies immediately, no waiting for the periodic sync), per-file **stage/unstage** plus one-click **stage all**, and **commit+push** of the staged set (message optional, auto-generated when empty). The dual-pane (side-by-side) diff viewer shows the working tree vs HEAD — untracked files included — and the working-tree copy can be fixed right there: click a line for inline single-line editing, or open the full-file editor. For push to work, the server's content remote needs write credentials (a read/write token in the URL, `https://<token>@github.com/you/content.git`, or a deploy key with write access).

**Proxy**: git network ops are direct by default (any `http_proxy` env inherited by the server process is explicitly ignored). If pull/push fails because the remote is unreachable without a proxy (e.g. GitHub behind a firewall), set `git_proxy = "http://127.0.0.1:7890"` in config.toml and restart — it applies to the periodic auto-pull, the status fetch, and the manual pull/push buttons. Network-looking failures also surface a hint in the console.

**Writing flow**: all writing happens locally — write/edit → push → the server auto-pulls. Quick server-side fixes can be done in the /admin diff editor and pushed from there. If pulls start failing, content probably diverged — check `git -C <content-dir> status`.

## Adding a page

1. Add a component in `src/pages/` (see `archive.rs`)
2. Add a `<Route>` in `src/app.rs`
3. Add a nav link in `src/components/layout.rs`

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
