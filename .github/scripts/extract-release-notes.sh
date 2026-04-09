#!/bin/bash
# Extracts release notes for a given version from CHANGELOG.md
# Usage: extract-release-notes.sh <version>
# Outputs:
#   release-title.txt  — "v0.1.8 | Subtitle..." or just "v0.1.8"
#   release-notes.md   — Full body (changelog section + installation footer)

set -euo pipefail

VERSION="$1"
CHANGELOG="CHANGELOG.md"

if [ ! -f "$CHANGELOG" ]; then
  echo "Error: $CHANGELOG not found" >&2
  exit 1
fi

# Extract subtitle from "> ..." line after version header
SUBTITLE=$(awk "/^## \[${VERSION}\]/{found=1; next} found && /^>/{gsub(/^> */, \"\"); print; exit}" "$CHANGELOG")

# Build title
if [ -n "$SUBTITLE" ]; then
  echo "v${VERSION} | ${SUBTITLE}" > release-title.txt
else
  echo "v${VERSION}" > release-title.txt
fi

# Extract body: everything between this version's ## and the next ##
awk "/^## \[${VERSION}\]/{flag=1; next} /^## \[/{flag=0} flag" "$CHANGELOG" \
  | sed '/^> /d' \
  | sed '1{/^$/d}' > changelog-section.md

if [ ! -s changelog-section.md ]; then
  echo "Warning: No changelog content found for version ${VERSION}" >&2
fi

# Build release body = changelog section + installation footer
cat changelog-section.md > release-notes.md
cat >> release-notes.md << 'FOOTER'

---

### Installation

**From crates.io**
```bash
cargo install fossil-mcp
```

**Pre-built Binaries**
Download the appropriate archive for your platform from the assets below, extract, and place `fossil-mcp` in your PATH.

### Platform Support
| Platform | Architecture | Archive |
|----------|-------------|---------|
| Linux | x86_64 (recommended) | `fossil-mcp-linux-x86_64-musl-*.tar.gz` |
| Linux | x86_64 (glibc) | `fossil-mcp-linux-x86_64-*.tar.gz` |
| Linux | ARM64 | `fossil-mcp-linux-aarch64-*.tar.gz` |
| macOS | x86_64 (Intel) | `fossil-mcp-macos-x86_64-*.tar.gz` |
| macOS | ARM64 (Apple Silicon) | `fossil-mcp-macos-aarch64-*.tar.gz` |
| Windows | x86_64 | `fossil-mcp-windows-x86_64-*.zip` |

### Changelog
See [CHANGELOG.md](https://github.com/yfedoseev/fossil-mcp/blob/main/CHANGELOG.md) for full details.
FOOTER

# Cleanup
rm -f changelog-section.md

echo "Generated release-title.txt and release-notes.md for v${VERSION}"
