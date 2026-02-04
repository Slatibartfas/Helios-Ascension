#!/bin/bash

# Download from Solar System Scope - they provide free textures
# License: CC BY 4.0 (requires attribution)

set -e

TEXTURE_DIR="./celestial"
BASE_URL="https://www.solarsystemscope.com/textures"

echo "=========================================="
echo "SOLAR SYSTEM SCOPE TEXTURE DOWNLOAD"
echo "=========================================="
echo ""
echo "Downloading from Solar System Scope"
echo "License: CC BY 4.0 (attribution required)"
echo "Based on NASA mission data"
echo ""

download_texture() {
    local filename="$1"
    local output="$2"
    local name="$3"
    local url="${BASE_URL}/${filename}"
    
    if [ -f "$output" ]; then
        local size=$(stat -c%s "$output" 2>/dev/null || stat -f%z "$output" 2>/dev/null)
        if [ "$size" -gt 500000 ]; then
            echo "✓ $name already exists ($(du -h "$output" | cut -f1)), skipping..."
            return 0
        fi
    fi
    
    echo "Downloading $name from $url..."
    if curl -f -L -o "$output.tmp" "$url" 2>/dev/null; then
        if file "$output.tmp" | grep -q "JPEG\|PNG"; then
            mv "$output.tmp" "$output"
            echo "✓ $name downloaded ($(du -h "$output" | cut -f1))"
        else
            echo "✗ $name failed (not valid image)"
            rm -f "$output.tmp"
            return 1
        fi
    else
        echo "✗ $name failed (HTTP error)"
        rm -f "$output.tmp"
        return 1
    fi
}

echo "=== MOONS (1K Resolution) ==="
echo ""

download_texture "2k_io.jpg" "$TEXTURE_DIR/moons/io_1k.jpg" "Io"
download_texture "2k_europa.jpg" "$TEXTURE_DIR/moons/europa_1k.jpg" "Europa"
download_texture "2k_ganymede.jpg" "$TEXTURE_DIR/moons/ganymede_1k.jpg" "Ganymede"
download_texture "2k_callisto.jpg" "$TEXTURE_DIR/moons/callisto_1k.jpg" "Callisto"
download_texture "2k_titan.jpg" "$TEXTURE_DIR/moons/titan_1k.jpg" "Titan"
download_texture "2k_enceladus.jpg" "$TEXTURE_DIR/moons/enceladus_1k.jpg" "Enceladus"
download_texture "2k_tethys.jpg" "$TEXTURE_DIR/moons/tethys_1k.jpg" "Tethys"
download_texture "2k_dione.jpg" "$TEXTURE_DIR/moons/dione_1k.jpg" "Dione"
download_texture "2k_rhea.jpg" "$TEXTURE_DIR/moons/rhea_1k.jpg" "Rhea"
download_texture "2k_iapetus.jpg" "$TEXTURE_DIR/moons/iapetus_1k.jpg" "Iapetus"

echo ""
echo "Download complete!"
echo ""
echo "License: CC BY 4.0"
echo "Attribution: 'Textures provided by Solar System Scope (https://www.solarsystemscope.com/)'"
