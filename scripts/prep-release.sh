#!/usr/bin/env bash
set -euo pipefail

# Prepare the repository for a new release.
#
# Usage:
#   ./scripts/prep-release.sh <version>
#
# Example:
#   ./scripts/prep-release.sh 0.4.1
#
# The script performs these steps:
#   1. Validate the requested version and working tree state.
#   2. Bump the crate version in Cargo.toml and update Cargo.lock.
#   3. Update AGENTS.md and README.md version references.
#   4. Move the current [Unreleased] CHANGELOG entries into a new dated section.
#   5. Regenerate docs/pacto-bot-admin-llms.txt via cargo xtask docs.
#   6. Run fmt-check, clippy, and the full test suite.

usage() {
  echo "Usage: $0 <version>"
  echo "Example: $0 0.4.1"
  exit 1
}

if [[ $# -ne 1 ]]; then
  usage
fi

new_version="$1"

if [[ ! "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.-]+)?$ ]]; then
  echo "error: '$new_version' does not look like a valid SemVer version" >&2
  exit 1
fi

# Ensure we are in the repository root.
cd "$(dirname "$0")/.."

if git status --short | grep -v '^?? scripts/prep-release.sh$' | grep -q .; then
  echo "error: working tree is not clean. Commit or stash changes first." >&2
  git status --short
  exit 1
fi

current_version="$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)"
if [[ "$new_version" == "$current_version" ]]; then
  echo "error: requested version '$new_version' is already the current version" >&2
  exit 1
fi

repo_url="$(grep '^repository' Cargo.toml | head -1 | cut -d'"' -f2)"
# Strip a trailing .git from the URL for the compare links.
repo_url="${repo_url%.git}"
release_date="$(date +%Y-%m-%d)"

echo "==> Preparing release $new_version (from $current_version)"

# 1. Bump Cargo.toml version.
echo "==> Updating Cargo.toml version"
perl -pi -e "s/^version = \"$current_version\"/version = \"$new_version\"/" Cargo.toml

# 2. Update Cargo.lock.
echo "==> Updating Cargo.lock"
cargo update -p pacto-bot-api

# 3. Update AGENTS.md version references.
echo "==> Updating AGENTS.md version references"
perl -pi -e "s/\b$current_version\b/$new_version/g" AGENTS.md

# 4. Update README.md install example.
echo "==> Updating README.md version reference"
perl -pi -e "s/PACTO_VERSION=$current_version/PACTO_VERSION=$new_version/g" README.md

# 5. Update CHANGELOG.md.
echo "==> Updating CHANGELOG.md"
python3 - "$new_version" "$current_version" "$release_date" "$repo_url" <<'PY'
import sys

new_version, prev_version, release_date, repo_url = sys.argv[1:5]

with open("CHANGELOG.md", "r") as f:
    content = f.read()

lines = content.splitlines()

# Find the [Unreleased] header and the next version header.
unreleased_idx = None
next_version_idx = None
for i, line in enumerate(lines):
    if line.strip() == "## [Unreleased]":
        unreleased_idx = i
    elif unreleased_idx is not None and line.startswith("## ["):
        next_version_idx = i
        break

if unreleased_idx is None:
    print("error: could not find ## [Unreleased] section in CHANGELOG.md", file=sys.stderr)
    sys.exit(1)

if next_version_idx is None:
    print("error: could not find next version section after [Unreleased] in CHANGELOG.md", file=sys.stderr)
    sys.exit(1)

# Extract the body between the Unreleased header and the next version header.
unreleased_body = lines[unreleased_idx + 1:next_version_idx]

# Build the new section. Keep the body intact (may be empty).
new_section_lines = [
    "## [Unreleased]",
    "",
    f"## [{new_version}] - {release_date}",
] + unreleased_body

# Replace the old Unreleased section with the new one.
new_lines = lines[:unreleased_idx] + new_section_lines + lines[next_version_idx:]

# Update the footer compare links.
footer_lines = []
unreleased_link_found = False
new_link_added = False
for line in new_lines:
    if line.startswith("[Unreleased]:"):
        footer_lines.append(f"[Unreleased]: {repo_url}/compare/v{new_version}...HEAD")
        unreleased_link_found = True
    elif line.startswith(f"[{prev_version}]:"):
        footer_lines.append(f"[{new_version}]: {repo_url}/compare/v{prev_version}...v{new_version}")
        footer_lines.append(line)
        new_link_added = True
    else:
        footer_lines.append(line)

if not unreleased_link_found:
    print("error: could not find [Unreleased] compare link in CHANGELOG.md", file=sys.stderr)
    sys.exit(1)

if not new_link_added:
    print("error: could not find [prev_version] compare link in CHANGELOG.md", file=sys.stderr)
    sys.exit(1)

with open("CHANGELOG.md", "w") as f:
    f.write("\n".join(footer_lines) + "\n")

print(f"Updated CHANGELOG.md with [{new_version}] - {release_date}")
PY

# 6. Regenerate operator guide.
echo "==> Regenerating docs/pacto-bot-admin-llms.txt"
cargo xtask docs

# 7. Run validation gates.
echo "==> Running make validate"
make validate

echo "==> Running cargo test --all-targets --all-features"
cargo test --all-targets --all-features

echo ""
echo "Release prep complete for $new_version."
echo "Review the changes, then commit and tag:"
echo "  git add -u"
echo "  git commit -m \"chore: release $new_version\""
echo "  git tag v$new_version"
echo "  git push && git push origin v$new_version"
