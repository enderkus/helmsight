#!/bin/sh
# Builds the project website (landing page and documentation) into site/_site.
# Requires mdbook (https://rust-lang.github.io/mdBook/).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
out="$here/_site"
rm -rf "$out"
mkdir -p "$out/screenshots" "$here/book/src/images"
cp "$root"/docs/screenshots/*.png "$here/book/src/images/"
cp "$root"/docs/screenshots/*.png "$out/screenshots/"
cp -R "$here/landing/." "$out/"
mdbook build "$here/book"
touch "$out/.nojekyll"
echo "site built in $out"
