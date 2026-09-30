#!/usr/bin/env bash
# Pobiera model U²-Net-p (Apache-2.0, ~4.6 MB) do usuwania tła do ~/.local/share/dziwak/models/.
set -euo pipefail
dir="${XDG_DATA_HOME:-$HOME/.local/share}/dziwak/models"
mkdir -p "$dir"
url="https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx"
sum="309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8"
if [ -f "$dir/u2netp.onnx" ] && echo "$sum  $dir/u2netp.onnx" | sha256sum -c --quiet 2>/dev/null; then
  echo "Model już jest: $dir/u2netp.onnx"; exit 0
fi
curl -fL --progress-bar -o "$dir/u2netp.onnx.tmp" "$url"
echo "$sum  $dir/u2netp.onnx.tmp" | sha256sum -c --quiet
mv "$dir/u2netp.onnx.tmp" "$dir/u2netp.onnx"
echo "Pobrano model: $dir/u2netp.onnx"
