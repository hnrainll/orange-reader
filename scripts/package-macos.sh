#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"
cargo build --release --locked

bundle_dir="$(mktemp -d "${TMPDIR:-/tmp}/verso-bundle.XXXXXX")"
trap 'rm -rf "$bundle_dir"' EXIT
app_dir="$bundle_dir/Verso.app"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources" "$repo_dir/dist"
cp "$repo_dir/target/release/verso" "$app_dir/Contents/MacOS/"
cp "$repo_dir/packaging/macos/Info.plist" "$app_dir/Contents/"
cp "$repo_dir/build/icon.icns" "$app_dir/Contents/Resources/"
cp "$repo_dir/foliate-js/LICENSE" "$app_dir/Contents/Resources/foliate-js-LICENSE"

plutil -lint "$app_dir/Contents/Info.plist"
codesign --force --deep --sign - "$app_dir"
codesign --verify --deep --strict "$app_dir"
archive="$repo_dir/dist/Verso-2.0.0-macos.zip"
ditto -c -k --sequesterRsrc --keepParent "$app_dir" "$archive"
echo "$archive"
