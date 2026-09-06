# Leafpress

A full-stack Rust blog engine — **Leptos** (SSR + WASM hydration) + **Axum** + **SQLite**, shipped as a single self-updating binary. The server never compiles anything.

- Markdown as content — hot-reload on every change, no restart
- `/admin` for online writing and comment moderation (can be made read-only so content flows only through git)
- Comments / likes in a single SQLite file; light/dark theme toggle
- Content decoupled from the program: `content_dir` points at any directory (typically a separate private repo)

## Local development

Requires the Rust toolchain, `cargo-leptos`, and the `wasm32-unknown-unknown` target.

```bash
cp config.example.toml config.toml    # set site_name and admin_password
git clone <content-repo> content      # your content repo; or start with an empty content/
cargo leptos watch
```

Runs at http://127.0.0.1:3000 (admin at `/admin`).

**config.toml** (gitignored; only `admin_password` is required):

| Key | Default | Purpose |
|---|---|---|
| `site_name` | `"My Site"` | site title |
| `content_dir` | `"content"` | where the markdown lives |
| `database` | `"site.db"` | comments/likes database |
| `admin_readonly` | `false` | `true` = /admin is read-only |

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

Install (as root, interactive prompts; re-running is a pure upgrade — config / content / database are never touched):

```bash
curl -fsSL https://raw.githubusercontent.com/YXalix/leafpress/main/deploy/install.sh | sudo bash
```

Upgrade:

```bash
sudo /opt/leafpress/leafpress update
```

All state lives in explicit locations and survives reinstalls:

| State | Location |
|---|---|
| `config.toml` | `/opt/leafpress/config.toml` (written once, never overwritten) |
| database | `/opt/leafpress/data/site.db` |
| content | the directory chosen at install time (e.g. `/srv/leafpress-content`) |

Run `sudo /opt/leafpress/leafpress doctor` afterwards to verify.

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

Body in GFM: tables, task lists, code blocks all work.
```

- `posts/**/*.md` → `/posts/<slug>`, `pages/**/*.md` → `/pages/<slug>`, `images/foo.png` → `/images/foo.png`
- Subdirectories are for organization only — URLs stay flat, slugs are globally unique; `category` defaults to the folder name

**Server auto-sync**: at install time choose "clone git repo" as the content source (read-only credential: a token embedded in the URL, `https://<token>@github.com/you/content.git`, or a deploy key), then pull on a root crontab:

```cron
*/5 * * * * git -C /srv/leafpress-content pull -q --ff-only
```

**Single writing entry point**: with `admin_readonly = true` in config.toml (restart to apply), /admin can browse and moderate but all create/edit/delete goes through local git. If the cron pull starts failing, content probably diverged from earlier /admin edits — check `git -C <content-dir> status`.

## Adding a page

1. Add a component in `src/pages/` (see `archive.rs`)
2. Add a `<Route>` in `src/app.rs`
3. Add a nav link in `src/components/layout.rs`

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
