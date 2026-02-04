#!/bin/bash

# Download missing textures from Steve Albers / Solar System Scope / NOAA
# These sources allow direct downloads and are generally reliable

set -e

TEXTURE_DIR="./celestial"
mkdir -p "$TEXTURE_DIR/moons"
mkdir -p "$TEXTURE_DIR/planets"
mkdir -p "$TEXTURE_DIR/asteroids"

echo "=========================================="
echo "DOWNLOADING CELESTIAL TEXTURES"
echo "=========================================="

download_texture() {
    local url="$1"
    local output="$2"
    local name="$3"
    
    # Check if a file with the same base name but different extension exists (e.g. png vs jpg)
    # We will just check the exact output path for now.
    
    if [ -f "$output" ]; then
        local size=$(ls -l "$output" | awk "{print $5}")
        # Simplistic size check, just skipping if exists for now to avoid re-download
        echo "✓ $name already exists, skipping..."
        return 0
    fi
    
    echo "Attempting to download $name..."
    echo "  URL: $url"
    
    # Try using curl
    if curl -f -L -o "$output.tmp" "$url" --connect-timeout 30 --retry 3; then
        # Check if it is actually an image (simple check)
        if grep -qE "JFIF|PNG|Exif|II\*" <(head -c 10 "$output.tmp"); then
            mv "$output.tmp" "$output"
            echo "✓ $name downloaded successfully"
        else
            # Try file command if available
            if command -v file &> /dev/null; then
                 if file "$output.tmp" | grep -qE "image|bitmap|JPEG|PNG|TIFF"; then
                    mv "$output.tmp" "$output"
                    echo "✓ $name downloaded successfully"
                 else
                    echo "✗ $name download failed (not a valid image or verification failed)"
                    rm -f "$output.tmp"
                 fi
            else 
                 # Assume it worked if curl worked and we cant check magic bytes easily in bash without tools
                 # But grep check above handles most cases.
                 # If grep failed, it might be TIF or other format.
                 mv "$output.tmp" "$output"
                 echo "✓ $name downloaded successfully (verification skipped)"
            fi
        fi
    else
        echo "✗ $name download failed (HTTP error)"
        rm -f "$output.tmp"
    fi
}

echo "=== MOONS - JUPITER ==="
download_texture "https://stevealbers.net/albers/sos/jupiter/io/io_rgb_cyl.jpg" "$TEXTURE_DIR/moons/io_4k.jpg" "Io"
download_texture "https://stevealbers.net/albers/sos/jupiter/europa/europa_rgb_cyl_juno.png" "$TEXTURE_DIR/moons/europa_4k.png" "Europa"
download_texture "http://stevealbers.net/albers/sos/jupiter/ganymede/ganymede_4k.jpg" "$TEXTURE_DIR/moons/ganymede_4k.jpg" "Ganymede"
download_texture "https://bjj.mmedia.is/data/callisto/callisto.jpg" "$TEXTURE_DIR/moons/callisto_4k.jpg" "Callisto"

echo "=== MOONS - SATURN ==="
# Titan - using radar map as visual is just haze
download_texture "https://stevealbers.net/albers/sos/saturn/titan/titan_cyl.jpg" "$TEXTURE_DIR/moons/titan_4k.jpg" "Titan (Radar)"
download_texture "https://stevealbers.net/albers/sos/saturn/enceladus/enceladus_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/enceladus_4k.jpg" "Enceladus"
download_texture "https://stevealbers.net/albers/sos/saturn/mimas/mimas_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/mimas_2k.jpg" "Mimas"
download_texture "https://stevealbers.net/albers/sos/saturn/tethys/tethys_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/tethys_4k.jpg" "Tethys"
download_texture "https://stevealbers.net/albers/sos/saturn/dione/dione_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/dione_4k.jpg" "Dione"
download_texture "https://stevealbers.net/albers/sos/saturn/rhea/rhea_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/rhea_4k.jpg" "Rhea"
download_texture "https://stevealbers.net/albers/sos/saturn/iapetus/iapetus_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/iapetus_4k.jpg" "Iapetus"

echo "=== MOONS - URANUS ==="
download_texture "https://stevealbers.net/albers/sos/uranus/ariel/ariel_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/ariel_2k.jpg" "Ariel"
download_texture "https://stevealbers.net/albers/sos/uranus/titania/titania_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/titania_2k.jpg" "Titania"
download_texture "https://stevealbers.net/albers/sos/uranus/oberon/oberon_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/oberon_2k.jpg" "Oberon"

echo "=== MOONS - NEPTUNE ==="
download_texture "https://stevealbers.net/albers/sos/neptune/triton/triton_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/triton_4k.jpg" "Triton"

echo "=== DWARF PLANETS & ASTEROIDS ==="
download_texture "http://stevealbers.net/albers/sos/asteroids/ceres_rgb_cyl.png" "$TEXTURE_DIR/asteroids/ceres_4k.png" "Ceres"
download_texture "http://stevealbers.net/albers/sos/asteroids/vesta.png" "$TEXTURE_DIR/asteroids/vesta_4k.png" "Vesta"
download_texture "https://stevealbers.net/albers/sos/pluto/pluto_rgb_cyl_8k.png" "$TEXTURE_DIR/planets/pluto_8k.png" "Pluto"
download_texture "https://stevealbers.net/albers/sos/pluto/charon/charon_rgb_cyl.jpg" "$TEXTURE_DIR/moons/charon_4k.jpg" "Charon"

echo "=========================================="
echo "Download process complete."
echo "Note: Some files may be PNGs. The game generally supports PNG/JPG/TIF."

