#!/usr/bin/env bash
# ----------------------------------------------------------------------------
#  Sudostake AI agent build script
#
#  Creates a new version folder in ~/.nearai/registry/…/sudo/<ver>
#  based on the highest version already present there.
#
#  Usage:
#     ./agent/build.sh [patch|minor|major]
#  (default bump = patch)
#  ----------------------------------------------------------------------------

set -e  # stop on first error

# ---------- 1. CONFIG --------------------------------------------------------
ACCOUNT="sudostake.near"      # the NEAR account that owns the agent
NAME="sudo"                   # agent name in the registry
MODE=${1:-patch}              # bump type from first CLI arg (default patch)

# Where the local registry lives  (override with env var if needed)
DATA_ROOT="${NEARAI_DATA_FOLDER:-$HOME/.nearai/registry}"

# Paths inside the repo
REPO_ROOT=$(git rev-parse --show-toplevel)
SRC_DIR="$REPO_ROOT/agent/src"           # editable source
META_SRC="$REPO_ROOT/agent/metadata.json"
README_SRC="$REPO_ROOT/agent/README.md"
# -----------------------------------------------------------------------------

# ---------- 2. DISCOVER LATEST VERSION IN REGISTRY ---------------------------
REGISTRY_DIR="$DATA_ROOT/$ACCOUNT/$NAME"
mkdir -p "$REGISTRY_DIR"     # ensure parent dirs exist

# Collect folder names that look like x.y.z
existing=$(find "$REGISTRY_DIR" -maxdepth 1 -mindepth 1 -type d \
            | sed -E 's#.*/##' \
            | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' || true)

if [[ -z "$existing" ]]; then
  latest="0.0.0"             # no prior versions
else
  # sort numerically on major.minor.patch and take the highest
  latest=$(echo "$existing" | sort -t. -k1,1n -k2,2n -k3,3n | tail -n1)
fi

echo "Highest local version so far: $latest"
# -----------------------------------------------------------------------------

# ---------- 3. COMPUTE NEW VERSION -------------------------------------------
# We rely on python-semver for bumping logic
command -v python3 >/dev/null || { echo "python3 required"; exit 1; }

case $MODE in
  patch) new_version=$(python3 -c "import semver; print(semver.bump_patch('$latest'))") ;;
  minor) new_version=$(python3 -c "import semver; print(semver.bump_minor('$latest'))") ;;
  major) new_version=$(python3 -c "import semver; print(semver.bump_major('$latest'))") ;;
  *)     echo "unknown bump type: $MODE" && exit 1 ;;
esac

echo "→ New version: $new_version"
# -----------------------------------------------------------------------------

# ---------- 4. COPY SOURCE ---------------------------------------------------
DEST="$REGISTRY_DIR/$new_version"
rm -rf "$DEST"
mkdir -p "$DEST"
cp -R "$SRC_DIR/"* "$DEST"
# -----------------------------------------------------------------------------

# Include the agent-level README.md so the agent can ingest it.
if [[ -f "$README_SRC" ]]; then
  cp "$README_SRC" "$DEST/README.md"
fi
# -----------------------------------------------------------------------------

# ---------- 5. STAMP metadata.json WITH NEW VERSION --------------------------
command -v jq >/dev/null || { echo "jq required"; exit 1; }
jq --arg v "$new_version" '.version=$v' "$META_SRC" > "$DEST/metadata.json"
# -----------------------------------------------------------------------------

# ---------- 6. REPORT --------------------------------------------------------
echo
echo "✅  Folder ready:"
echo "   $DEST"
echo
echo "Next commands:"
echo "   nearai agent interactive \"$DEST\" --local"
echo "   nearai registry upload      \"$DEST\""
echo "-------------------------------------------------------------------------"