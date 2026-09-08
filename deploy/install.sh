#!/usr/bin/env bash
# Leafpress interactive install / reinstall (run on the server):
#   curl -fsSL https://raw.githubusercontent.com/YXalix/leafpress/main/deploy/install.sh | sudo bash
# (For a private fork/mirror: export GITHUB_TOKEN=<fine-grained PAT, Contents: Read> and the
# script authenticates its release download with it.)
#
# Idempotent: re-running only updates the binary; existing config.toml / content dir / database are always preserved.
# Non-interactive: override all prompts via environment variables, e.g.
#   INSTALL_DIR=/opt/leafpress CONTENT_URL=https://<token>@github.com/you/content.git \
#     ADMIN_PASSWORD=secret sudo -E bash install.sh
set -euo pipefail

REPO="${LEAFPRESS_REPO:-YXalix/leafpress}"
SERVICE="leafpress"

[ "$(id -u)" -eq 0 ] || { echo "Root privileges required: sudo bash install.sh"; exit 1; }

# Dependencies
need=()
command -v curl >/dev/null || need+=(curl)
command -v git  >/dev/null || need+=(git)
if [ "${#need[@]}" -gt 0 ]; then
  if command -v apt-get >/dev/null; then
    apt-get update -qq && apt-get install -y -qq "${need[@]}"
  else
    echo "Missing commands: ${need[*]}, please install them first"; exit 1
  fi
fi

# Prompts (with curl | bash, stdin is the pipe; read from /dev/tty to allow interaction)
ask() { # ask VAR "prompt" "default" [secret]
  local __var="$1" __prompt="$2" __default="${3:-}" __secret="${4:-}" __val=""
  [ -n "${!__var:-}" ] && return 0
  if [ -e /dev/tty ]; then
    # Write prompts to the tty explicitly: read -p prompts go to stderr and become invisible after redirection (the script looks like it "hangs")
    if [ -n "$__secret" ]; then
      printf '%s: ' "$__prompt" > /dev/tty
      read -rs __val < /dev/tty || true
      printf '\n' > /dev/tty
    else
      printf '%s [%s]: ' "$__prompt" "$__default" > /dev/tty
      read -r __val < /dev/tty || true
    fi
  fi
  printf -v "$__var" '%s' "${__val:-$__default}"
}

echo "=== Leafpress install ==="

# ---------- Download the binary first (the download is slow; finish it before user input) ----------
case "$(uname -m)" in
  x86_64)  target=x86_64-unknown-linux-gnu ;;
  aarch64) target=aarch64-unknown-linux-gnu ;;
  *) echo "Unsupported architecture: $(uname -m)"; exit 1 ;;
esac
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
auth=()
[ -n "${GITHUB_TOKEN:-}" ] && auth=(-H "Authorization: Bearer $GITHUB_TOKEN")
echo ">> Downloading latest release ($target)"
curl -fSL --progress-bar "${auth[@]}" \
  "https://github.com/$REPO/releases/latest/download/leafpress-$target" -o "$tmp/leafpress"

# ---------- User input ----------
ask INSTALL_DIR "Install dir" "/opt/leafpress"
ask SITE_ADDR   "Listen address" "0.0.0.0:3000"

# ---------- Install the binary ----------
mkdir -p "$INSTALL_DIR/data"   # database dir (explicit location, see database in config.toml)
install -m 755 "$tmp/leafpress" "$INSTALL_DIR/leafpress"

# CLI on PATH: a wrapper that pins the config location, so `leafpress doctor/passwd/update`
# works from any cwd. /usr/local/bin is in the default PATH (also sudo's secure_path) on
# mainstream distros — no user PATH edits needed.
mkdir -p /usr/local/bin
printf '#!/bin/sh\nexec env LEAFPRESS_CONFIG="%s/config.toml" "%s/leafpress" "$@"\n' \
  "$INSTALL_DIR" "$INSTALL_DIR" > /usr/local/bin/leafpress
chmod 755 /usr/local/bin/leafpress

# Lets every local user (root CLI + leafpress service) run git in the content repo:
# git refuses cross-user repo access ("dubious ownership") unless the path is whitelisted.
# --system covers all users; the server binary additionally passes -c safe.directory itself.
mark_safe() {
  [ -d "$1/.git" ] || return 0
  git config --system --get-all safe.directory 2>/dev/null | grep -Fxq "$1" && return 0
  git config --system --add safe.directory "$1" || true
}

# ---------- Config & content (if config.toml already exists, keep everything; pure upgrade) ----------
if [ -f "$INSTALL_DIR/config.toml" ]; then
  echo ">> config.toml already exists; keeping config and content untouched"
  CONTENT_DIR_FINAL=$(sed -nE 's/^content_dir *= *"(.*)".*/\1/p' "$INSTALL_DIR/config.toml" | head -1)
  case "$CONTENT_DIR_FINAL" in /*) ;; *) CONTENT_DIR_FINAL="$INSTALL_DIR/${CONTENT_DIR_FINAL:-content}" ;; esac
  mark_safe "$CONTENT_DIR_FINAL"
else
  ask SITE_NAME "Site name" "My Blog"

  echo
  echo "Content (markdown posts) source:"
  echo "  1) clone a git repo (e.g. your private content repo)"
  echo "  2) an existing directory on this server"
  echo "  3) generate a sample post (ideal to get up and running)"
  [ -n "${CONTENT_URL:-}" ] && CONTENT_CHOICE=1
  [ -n "${CONTENT_DIR:-}" ] && [ -z "${CONTENT_URL:-}" ] && CONTENT_CHOICE=2
  ask CONTENT_CHOICE "Choice" "3"
  ask CONTENT_DIR "Content dir path" "/srv/leafpress-content"
  CONTENT_DIR_FINAL="$CONTENT_DIR"

  # Content setup is deliberately non-fatal: a bad URL / network failure must not abort the
  # install before config.toml and the systemd unit are written (that leaves a broken
  # half-installed server). On failure the site simply starts with an empty content dir.
  setup_content() {
    case "$CONTENT_CHOICE" in
      1)
        ask CONTENT_URL "git repo URL (for private repos use the https://<token>@github.com/... form)" ""
        if [ -z "$CONTENT_URL" ]; then
          echo "!! git repo URL is empty"
          return 1
        fi
        mkdir -p "$(dirname "$CONTENT_DIR")"
        if [ -d "$CONTENT_DIR/.git" ]; then
          echo ">> Already exists, running git pull"
          git -C "$CONTENT_DIR" pull --ff-only || true
        elif [ -d "$CONTENT_DIR" ] && [ -n "$(ls -A "$CONTENT_DIR" 2>/dev/null)" ]; then
          echo "!! $CONTENT_DIR is not empty and not a git repo — empty it or pick another path"
          return 1
        else
          git clone "$CONTENT_URL" "$CONTENT_DIR" || return 1
        fi
        ;;
      2)
        [ -d "$CONTENT_DIR" ] || { echo "!! Directory does not exist: $CONTENT_DIR"; return 1; }
        ;;
      *)
        mkdir -p "$CONTENT_DIR"
        if [ -n "$(ls -A "$CONTENT_DIR" 2>/dev/null)" ]; then
          echo ">> $CONTENT_DIR is not empty, leaving it as-is"
        else
          mkdir -p "$CONTENT_DIR/posts"
          cat > "$CONTENT_DIR/posts/hello.md" <<'MD'
---
title: "Hello, Leafpress"
date: "2026-01-01 12:00"
status: published
---

Leafpress 已经跑起来了。在内容目录里增删 `.md` 文件即可更新（热加载，无需重启），
也可以在 /admin 的 git 控制台提交并推送改动。这篇示例可以放心删除。
MD
        fi
        ;;
    esac
  }
  if setup_content; then
    mark_safe "$CONTENT_DIR"
  else
    echo "!! Content setup failed — continuing anyway; the site starts with an empty content dir."
    echo "   Fix later: git clone <repo> $CONTENT_DIR && chown -R leafpress:leafpress $CONTENT_DIR && systemctl restart leafpress"
    mkdir -p "$CONTENT_DIR"
  fi
  mkdir -p "$CONTENT_DIR/images"

  ask ADMIN_PASSWORD "Admin password (for /admin login, must not contain double quotes)" "" secret
  [ -n "$ADMIN_PASSWORD" ] || ADMIN_PASSWORD="changeme"

  cat > "$INSTALL_DIR/config.toml" <<EOF
site_name = "$SITE_NAME"
admin_password = "$ADMIN_PASSWORD"
content_dir = "$CONTENT_DIR_FINAL"
database = "$INSTALL_DIR/data/site.db"
EOF
  echo ">> Generated $INSTALL_DIR/config.toml (database: $INSTALL_DIR/data/site.db)"
fi

# ---------- User & permissions ----------
id -u leafpress >/dev/null 2>&1 || useradd --system --home "$INSTALL_DIR" --shell /usr/sbin/nologin leafpress
chown -R leafpress:leafpress "$INSTALL_DIR"
[ -n "${CONTENT_DIR_FINAL:-}" ] && [ -d "$CONTENT_DIR_FINAL" ] && chown -R leafpress:leafpress "$CONTENT_DIR_FINAL"

# ---------- SSH for the service user (only when the content remote is git-over-SSH) ----------
# The service runs as leafpress with HOME=$INSTALL_DIR; without $INSTALL_DIR/.ssh every
# git fetch/pull fails with "Host key verification failed". Best-effort: never aborts install.
seed_ssh() {
  [ -n "${CONTENT_DIR_FINAL:-}" ] && [ -d "$CONTENT_DIR_FINAL/.git" ] || return 0
  local url
  url=$(git -C "$CONTENT_DIR_FINAL" remote get-url origin 2>/dev/null) || return 0
  [ -n "$url" ] || return 0
  local host="" port=""
  case "$url" in
    git@*:*)
      host="${url#git@}"; host="${host%%:*}" ;;
    ssh://*)
      host="${url#ssh://}"; host="${host#*@}"; host="${host%%/*}"
      case "$host" in *:*) port="${host##*:}"; host="${host%%:*}" ;; esac ;;
    *) return 0 ;; # https/local remote: no SSH setup needed
  esac
  [ -n "$host" ] || return 0
  echo ">> Content remote is SSH ($host) — seeding $INSTALL_DIR/.ssh for the leafpress user"
  install -d -m 700 -o leafpress -g leafpress "$INSTALL_DIR/.ssh"

  # known_hosts: prefer keys root already trusts; only if that file lacks the host (idempotent)
  local kh="$INSTALL_DIR/.ssh/known_hosts" kh_host="$host"
  [ -n "$port" ] && kh_host="[$host]:$port"
  local root_home
  root_home=$(getent passwd root | cut -d: -f6); root_home="${root_home:-/root}"
  if ssh-keygen -F "$kh_host" -f "$kh" >/dev/null 2>&1; then
    echo ">> $kh_host already in $kh"
  else
    local src copied=""
    for src in "$root_home/.ssh/known_hosts" /etc/ssh/ssh_known_hosts; do
      if [ -f "$src" ] && ssh-keygen -F "$kh_host" -f "$src" >/dev/null 2>&1; then
        ssh-keygen -F "$kh_host" -f "$src" | grep -v '^#' >> "$kh"
        echo ">> Copied $kh_host host key from $src"
        copied=1
        break
      fi
    done
    if [ -z "$copied" ]; then
      echo "!! $kh_host is not in root's known_hosts — scanning it now (trust-on-first-use)"
      local scan=() scan_out=""
      [ -n "$port" ] && scan=(-p "$port")
      if scan_out=$(ssh-keyscan "${scan[@]}" "$host" 2>/dev/null) && [ -n "$scan_out" ]; then
        printf '%s\n' "$scan_out" >> "$kh"
        echo "!! Verify this fingerprint against your git host's published fingerprint:"
        printf '%s\n' "$scan_out" | ssh-keygen -lf - | sed 's/^/!!   /'
      else
        echo "!! ssh-keyscan $host failed — add the host key manually: ssh-keyscan $host >> $kh"
      fi
    fi
  fi

  # Auth key: keep an existing one, else reuse root's key pair, else generate a deploy key
  if [ -f "$INSTALL_DIR/.ssh/id_ed25519" ] || [ -f "$INSTALL_DIR/.ssh/id_rsa" ]; then
    echo ">> $INSTALL_DIR/.ssh already has a private key, leaving it as-is"
  elif [ -f "$root_home/.ssh/id_ed25519" ] && [ -f "$root_home/.ssh/id_ed25519.pub" ]; then
    cp "$root_home/.ssh/id_ed25519" "$root_home/.ssh/id_ed25519.pub" "$INSTALL_DIR/.ssh/"
    echo ">> Copied root's id_ed25519 key pair (same git access as root)"
  elif [ -f "$root_home/.ssh/id_rsa" ] && [ -f "$root_home/.ssh/id_rsa.pub" ]; then
    cp "$root_home/.ssh/id_rsa" "$root_home/.ssh/id_rsa.pub" "$INSTALL_DIR/.ssh/"
    echo ">> Copied root's id_rsa key pair (same git access as root)"
  else
    ssh-keygen -t ed25519 -N '' -q -f "$INSTALL_DIR/.ssh/id_ed25519"
    echo "!! Generated a new SSH key for the leafpress user. Add this public key as a deploy key"
    echo "!! (WITH WRITE ACCESS) on your git host, otherwise pull/push will fail:"
    sed 's/^/!!   /' "$INSTALL_DIR/.ssh/id_ed25519.pub"
  fi
  chown -R leafpress:leafpress "$INSTALL_DIR/.ssh"
  chmod 700 "$INSTALL_DIR/.ssh"
  local f
  for f in "$INSTALL_DIR/.ssh/id_ed25519" "$INSTALL_DIR/.ssh/id_rsa"; do
    [ ! -f "$f" ] || chmod 600 "$f"
  done
  for f in "$INSTALL_DIR/.ssh/id_ed25519.pub" "$INSTALL_DIR/.ssh/id_rsa.pub" "$kh"; do
    [ ! -f "$f" ] || chmod 644 "$f"
  done
}
seed_ssh || echo "!! SSH setup for the leafpress user failed (non-fatal) — git sync over SSH may not work"

# ---------- systemd (generated from the actual install dir) ----------
if command -v systemctl >/dev/null && [ -d /run/systemd/system ]; then
  cat > "/etc/systemd/system/$SERVICE.service" <<EOF
[Unit]
Description=Leafpress blog engine
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=leafpress
WorkingDirectory=$INSTALL_DIR
ExecStart=$INSTALL_DIR/leafpress
Restart=on-failure
RestartSec=2
Environment=LEPTOS_SITE_ADDR=$SITE_ADDR

[Install]
WantedBy=multi-user.target
EOF

  systemctl daemon-reload
  systemctl enable "$SERVICE" >/dev/null 2>&1
  systemctl restart "$SERVICE"
  SVC_STATUS="$(systemctl is-active "$SERVICE") (logs: systemctl status $SERVICE)"
else
  SVC_STATUS="not installed (systemd not detected)"
  echo "!! systemd not detected — service unit skipped. Run manually:"
  echo "   LEAFPRESS_CONFIG=$INSTALL_DIR/config.toml LEPTOS_SITE_ADDR=$SITE_ADDR $INSTALL_DIR/leafpress"
fi

echo
echo "=== Done ==="
echo "Status:   $SVC_STATUS"
echo "Visit:    http://<server-IP>:${SITE_ADDR##*:}  (admin at /admin)"
echo "Config:   $INSTALL_DIR/config.toml"
echo "Database: $INSTALL_DIR/data/site.db"
echo "Content:  ${CONTENT_DIR_FINAL:-$INSTALL_DIR/content} (changes hot-reload, no restart needed)"
echo "CLI:      leafpress (wrapper at /usr/local/bin/leafpress, config pinned via LEAFPRESS_CONFIG)"
echo "Update later: sudo leafpress update"
