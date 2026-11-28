#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
upstream_url="${FOLIATE_JS_REPO:-https://github.com/johnfactotum/foliate-js.git}"
upstream_ref="${1:-main}"
meta_file="$repo_dir/scripts/foliate-js-upstream.txt"
dest_dir="$repo_dir/foliate-js"

files=(
  LICENSE
  view.js
  epub.js
  epubcfi.js
  paginator.js
  fixed-layout.js
  overlayer.js
  progress.js
  text-walker.js
  comic-book.js
  fb2.js
  footnotes.js
  mobi.js
  search.js
  tts.js
  vendor/zip.js
  vendor/fflate.js
)

work_dir="$(mktemp -d "${TMPDIR:-/tmp}/foliate-js-update.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT

clone_dir="$work_dir/upstream"
stage_dir="$work_dir/stage"
mkdir -p "$stage_dir/vendor"

git init -q "$clone_dir"
git -C "$clone_dir" remote add origin "$upstream_url"
git -C "$clone_dir" fetch --depth 1 origin "$upstream_ref" \
  || git -C "$clone_dir" -c http.version=HTTP/1.1 fetch --depth 1 origin "$upstream_ref"
git -C "$clone_dir" checkout -q FETCH_HEAD

commit="$(git -C "$clone_dir" rev-parse HEAD)"
commit_date="$(git -C "$clone_dir" log -1 --format=%cs HEAD)"
commit_subject="$(git -C "$clone_dir" log -1 --format=%s HEAD)"

for rel in "${files[@]}"; do
  src="$clone_dir/$rel"
  if [[ ! -f "$src" ]]; then
    echo "error: expected upstream file missing: $rel" >&2
    exit 1
  fi
  mkdir -p "$stage_dir/$(dirname "$rel")"
  cp "$src" "$stage_dir/$rel"
done

# Verso does not bundle vendor/pdfjs; keep a lightweight stub for view.js dynamic import.
cat > "$stage_dir/pdf.js" <<'EOF'
export const makePDF = () => { throw new Error('PDF not supported') }
EOF

rm -rf "$dest_dir"
mv "$stage_dir" "$dest_dir"

cat > "$meta_file" <<EOF
repo=https://github.com/johnfactotum/foliate-js
commit=$commit
date=$commit_date
subject=$commit_subject
EOF

echo "Synced foliate-js to $commit ($commit_date): $commit_subject"
echo "Next steps:"
echo "  git diff --stat foliate-js/"
echo "  cargo test"
echo "  VERSO_DATA_DIR=/private/tmp/verso-test cargo run -- /path/to/book.epub --self-test"
