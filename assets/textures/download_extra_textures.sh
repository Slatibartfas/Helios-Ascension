#!/bin/bash

# Download additional textures and rings
# Sources: Steve Albers, Solar System Scope, Planet Pixel Emporium, USGS

set -e

# Ensure we are in the right directory or paths are correct
# We assume this script is run from assets/textures/ or we cd to it
cd "$(dirname "$0")"

TEXTURE_DIR="./celestial"
mkdir -p "$TEXTURE_DIR/moons"
mkdir -p "$TEXTURE_DIR/planets"
mkdir -p "$TEXTURE_DIR/asteroids"
mkdir -p "$TEXTURE_DIR/rings"

echo "=========================================="
echo "DOWNLOADING ADDITIONAL TEXTURES"
echo "=========================================="

download_texture() {
    local url="$1"
    local output="$2"
    local name="$3"
    
    if [ -f "$output" ]; then
        echo "✓ $name already exists, skipping..."
        return 0
    fi
    
    echo "Attempting to download $name..."
    # -L follows redirects, -f fails on HTTP error
    if curl -f -L -o "$output.tmp" "$url" --connect-timeout 30 --retry 3; then
        mv "$output.tmp" "$output"
        echo "✓ $name downloaded successfully"
    else
        echo "✗ $name download failed"
        rm -f "$output.tmp"
    fi
}

echo "=== MARS MOONS ==="
# Phobos - USGS
download_texture "https://astrogeology.usgs.gov/ckan/dataset/ca781f2d-0e29-4560-a14e-1b41269c74a9/resource/6c47165c-4094-4ff5-8a21-065deb4319d6/download/phobos_me_src_mosaic_global_1024.jpg" "$TEXTURE_DIR/moons/phobos_2k.jpg" "Phobos"

# Deimos - Fallback to Celestia Motherlode or similar if USGS is complex
# Trying a known Celestia mirror or Steve Albers alternative path
download_texture "https://stevealbers.net/albers/sos/mars/deimos/deimos_rgb_cyl.jpg" "$TEXTURE_DIR/moons/deimos_2k.jpg" "Deimos"
# If that fails, we can try this one from a random university mirror or texture site
# download_texture "http://www.solarviews.com/raw/mars/deimos.jpg"

echo "=== SATURN MOONS ==="
# Titan - Surface
download_texture "https://stevealbers.net/albers/sos/saturn/titan/titan_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/titan_4k.jpg" "Titan"

# Mimas
download_texture "https://stevealbers.net/albers/sos/saturn/mimas/mimas_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/mimas_4k.jpg" "Mimas"

# Phoebe
download_texture "https://stevealbers.net/albers/sos/saturn/phoebe/phoebe_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/phoebe_4k.jpg" "Phoebe"

echo "=== URANUS MOONS ==="
# Miranda
download_texture "https://stevealbers.net/albers/sos/uranus/miranda/miranda_rgb_cyl_www.jpg" "$TEXTURE_DIR/moons/miranda_4k.jpg" "Miranda"

echo "=== RINGS ==="
download_texture "https://www.solarsystemscope.com/textures/download/8k_saturn_ring_alpha.png" "$TEXTURE_DIR/rings/saturn_rings_8k.png" "Saturn Rings"

echo "=========================================="
echo "Done."

