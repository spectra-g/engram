#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <version>"
  echo "Example: $0 0.2.0"
  exit 1
fi

VERSION="$1"
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

echo "Setting version to $VERSION across all packages..."

# 1. core/Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"$VERSION\"/" "$ROOT_DIR/core/Cargo.toml"
rm -f "$ROOT_DIR/core/Cargo.toml.bak"
echo "  Updated core/Cargo.toml"

# 2. adapter/package.json — version + optionalDependencies
cd "$ROOT_DIR/adapter"
npm version "$VERSION" --no-git-tag-version
node -e "
  const fs = require('fs');
  const pkg = JSON.parse(fs.readFileSync('package.json', 'utf-8'));
  for (const dep of Object.keys(pkg.optionalDependencies || {})) {
    pkg.optionalDependencies[dep] = '$VERSION';
  }
  fs.writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');
"
echo "  Updated adapter/package.json"

# 2b. adapter/package-lock.json — keep in sync with package.json
node -e "
  const fs = require('fs');
  const lock = JSON.parse(fs.readFileSync('package-lock.json', 'utf-8'));
  lock.version = '$VERSION';
  if (lock.packages && lock.packages['']) {
    lock.packages[''].version = '$VERSION';
    const optDeps = lock.packages[''].optionalDependencies;
    if (optDeps) {
      for (const dep of Object.keys(optDeps)) {
        optDeps[dep] = '$VERSION';
      }
    }
  }
  for (const [key, entry] of Object.entries(lock.packages)) {
    if (key.includes('@spectra-g/engram-core-')) {
      entry.version = '$VERSION';
      const shortName = key.split('/').pop();
      const scopedName = key.replace('node_modules/', '');
      entry.resolved = 'https://registry.npmjs.org/' + scopedName + '/-/' + shortName + '-$VERSION.tgz';
      delete entry.integrity;
    }
  }
  fs.writeFileSync('package-lock.json', JSON.stringify(lock, null, 2) + '\n');
"
echo "  Updated adapter/package-lock.json"

# 3. Platform packages
for platform in darwin-arm64 darwin-x64 linux-x64 linux-arm64 win32-x64; do
  pkg_dir="$ROOT_DIR/npm/@spectra-g/engram-core-$platform"
  if [ -f "$pkg_dir/package.json" ]; then
    cd "$pkg_dir"
    npm version "$VERSION" --no-git-tag-version
    echo "  Updated npm/@spectra-g/engram-core-$platform/package.json"
  fi
done

# 4. adapter/src/mcp-server.ts — version string
sed -i.bak "s/version: \".*\"/version: \"$VERSION\"/" "$ROOT_DIR/adapter/src/mcp-server.ts"
rm -f "$ROOT_DIR/adapter/src/mcp-server.ts.bak"
echo "  Updated adapter/src/mcp-server.ts"

echo ""
echo "All packages set to version $VERSION"
