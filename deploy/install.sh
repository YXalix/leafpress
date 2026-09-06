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

# ---------- Config & content (if config.toml already exists, keep everything; pure upgrade) ----------
if [ -f "$INSTALL_DIR/config.toml" ]; then
  echo ">> config.toml already exists; keeping config and content untouched"
  CONTENT_DIR_FINAL=$(sed -nE 's/^content_dir *= *"(.*)".*/\1/p' "$INSTALL_DIR/config.toml" | head -1)
  case "$CONTENT_DIR_FINAL" in /*) ;; *) CONTENT_DIR_FINAL="$INSTALL_DIR/${CONTENT_DIR_FINAL:-content}" ;; esac
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

  case "$CONTENT_CHOICE" in
    1)
      ask CONTENT_URL "git repo URL (for private repos use the https://<token>@github.com/... form)" ""
      [ -n "$CONTENT_URL" ] || { echo "URL cannot be empty"; exit 1; }
      if [ -d "$CONTENT_DIR/.git" ]; then
        echo ">> Already exists, running git pull"
        git -C "$CONTENT_DIR" pull --ff-only || true
      else
        git clone "$CONTENT_URL" "$CONTENT_DIR"
      fi
      git config --global --add safe.directory "$CONTENT_DIR" || true
      ;;
    2)
      [ -d "$CONTENT_DIR" ] || { echo "Directory does not exist: $CONTENT_DIR"; exit 1; }
      ;;
    *)
      if [ -d "$CONTENT_DIR" ] && [ -n "$(ls -A "$CONTENT_DIR" 2>/dev/null)" ]; then
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
或访问 /admin 在线写作。这篇示例可以放心删除。
MD
      fi
      ;;
  esac
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

# ---------- systemd (generated from the actual install dir) ----------
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

echo
echo "=== Done ==="
echo "Status:   $(systemctl is-active "$SERVICE") (logs: systemctl status $SERVICE)"
echo "Visit:    http://<server-IP>:${SITE_ADDR##*:}  (admin at /admin)"
echo "Config:   $INSTALL_DIR/config.toml"
echo "Database: $INSTALL_DIR/data/site.db"
echo "Content:  ${CONTENT_DIR_FINAL:-$INSTALL_DIR/content} (changes hot-reload, no restart needed)"
echo "Update later: sudo $INSTALL_DIR/leafpress update"
