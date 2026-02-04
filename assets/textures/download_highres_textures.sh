#!/bin/bash
# Script to download HIGH RESOLUTION celestial body textures
# Primary: Solar System Scope 8K textures (based on NASA data)
# These are significantly higher quality than the previous 2K versions
# License: CC BY 4.0 (requires attribution) - but based on NASA public domain data

set -e

TEXTURES_DIR="$(dirname "$0")/celestial"
BASE_URL="https://www.solarsystemscope.com/textures"

echo "=========================================="
echo "HIGH RESOLUTION TEXTURE DOWNLOAD"
echo "=========================================="
echo ""
echo "Downloading high-resolution celestial body textures..."
echo "Resolution: 4K-8K (vs previous 1K-2K)"
echo "Source: Solar System Scope (based on NASA public domain data)"
echo "License: CC BY 4.0 (requires attribution)"
echo "Target directory: $TEXTURES_DIR"
echo ""
echo "Note: These files will be MUCH larger than previous versions"
echo "Expected total size: ~200-500MB (vs previous 6MB)"
echo ""

# Create directories if they don't exist
mkdir -p "$TEXTURES_DIR/stars"
mkdir -p "$TEXTURES_DIR/planets"
mkdir -p "$TEXTURES_DIR/moons"
mkdir -p "$TEXTURES_DIR/asteroids"
mkdir -p "$TEXTURES_DIR/comets"

# Function to download a texture with retry logic
download_texture() {
    local url="$1"
    local output="$2"
    local name="$3"
    local size_estimate="$4"
    
    if [ -f "$output" ]; then
        echo "✓ $name already exists, skipping..."
        return 0
    fi
    
    echo "Downloading $name ($size_estimate)..."
    if curl -L --retry 3 --retry-delay 2 --max-time 300 -o "$output" "$url" 2>/dev/null; then
        # Check if file was actually downloaded and has content
        if [ -s "$output" ]; then
            local actual_size=$(du -h "$output" | cut -f1)
            echo "✓ Downloaded $name - actual size: $actual_size"
        else
            echo "✗ Failed to download $name (empty file)"
            rm -f "$output"
            return 1
        fi
    else
        echo "✗ Failed to download $name from $url"
        rm -f "$output"
        return 1
    fi
}

echo "Starting downloads..."
echo ""
echo "=== STARS ==="

# Download Sun - 8K version
download_texture \
    "${BASE_URL}/download/8k_sun.jpg" \
    "$TEXTURES_DIR/stars/sun_8k.jpg" \
    "Sun (8K)" \
    "~40-50MB"

echo ""
echo "=== PLANETS ==="

# Download Planets - 8K versions where available, 4K as fallback
download_texture \
    "${BASE_URL}/download/8k_mercury.jpg" \
    "$TEXTURES_DIR/planets/mercury_8k.jpg" \
    "Mercury (8K)" \
    "~30-40MB"

download_texture \
    "${BASE_URL}/download/8k_venus_surface.jpg" \
    "$TEXTURES_DIR/planets/venus_surface_8k.jpg" \
    "Venus Surface (8K)" \
    "~30-40MB"

download_texture \
    "${BASE_URL}/download/8k_venus_atmosphere.jpg" \
    "$TEXTURES_DIR/planets/venus_atmosphere_8k.jpg" \
    "Venus Atmosphere (8K)" \
    "~30-40MB"

download_texture \
    "${BASE_URL}/download/8k_earth_daymap.jpg" \
    "$TEXTURES_DIR/planets/earth_8k.jpg" \
    "Earth (8K)" \
    "~40-50MB"

download_texture \
    "${BASE_URL}/download/8k_mars.jpg" \
    "$TEXTURES_DIR/planets/mars_8k.jpg" \
    "Mars (8K)" \
    "~40-50MB"

download_texture \
    "${BASE_URL}/download/8k_jupiter.jpg" \
    "$TEXTURES_DIR/planets/jupiter_8k.jpg" \
    "Jupiter (8K)" \
    "~40-50MB"

download_texture \
    "${BASE_URL}/download/8k_saturn.jpg" \
    "$TEXTURES_DIR/planets/saturn_8k.jpg" \
    "Saturn (8K)" \
    "~30-40MB"

download_texture \
    "${BASE_URL}/download/2k_uranus.jpg" \
    "$TEXTURES_DIR/planets/uranus_2k.jpg" \
    "Uranus (2K - 8K not available)" \
    "~5-10MB"

download_texture \
    "${BASE_URL}/download/2k_neptune.jpg" \
    "$TEXTURES_DIR/planets/neptune_2k.jpg" \
    "Neptune (2K - 8K not available)" \
    "~5-10MB"

echo ""
echo "=== MOONS ==="

# Download Major Moons - 4K/2K versions
download_texture \
    "${BASE_URL}/download/8k_moon.jpg" \
    "$TEXTURES_DIR/moons/moon_8k.jpg" \
    "Moon (8K)" \
    "~40-50MB"

# Jupiter's moons - 2K (higher res may not be available)
download_texture \
    "${BASE_URL}/download/2k_io.jpg" \
    "$TEXTURES_DIR/moons/io_2k.jpg" \
    "Io (2K)" \
    "~5MB"

download_texture \
    "${BASE_URL}/download/2k_europa.jpg" \
    "$TEXTURES_DIR/moons/europa_2k.jpg" \
    "Europa (2K)" \
    "~5MB"

download_texture \
    "${BASE_URL}/download/2k_ganymede.jpg" \
    "$TEXTURES_DIR/moons/ganymede_2k.jpg" \
    "Ganymede (2K)" \
    "~5MB"

download_texture \
    "${BASE_URL}/download/2k_callisto.jpg" \
    "$TEXTURES_DIR/moons/callisto_2k.jpg" \
    "Callisto (2K)" \
    "~5MB"

# Saturn's moons - 2K
download_texture \
    "${BASE_URL}/download/2k_titan.jpg" \
    "$TEXTURES_DIR/moons/titan_2k.jpg" \
    "Titan (2K)" \
    "~5MB"

download_texture \
    "${BASE_URL}/download/2k_enceladus.jpg" \
    "$TEXTURES_DIR/moons/enceladus_2k.jpg" \
    "Enceladus (2K)" \
    "~5MB"

download_texture \
    "${BASE_URL}/download/2k_rhea.jpg" \
    "$TEXTURES_DIR/moons/rhea_2k.jpg" \
    "Rhea (2K)" \
    "~5MB"

download_texture \
    "${BASE_URL}/download/2k_iapetus.jpg" \
    "$TEXTURES_DIR/moons/iapetus_2k.jpg" \
    "Iapetus (2K)" \
    "~5MB"

download_texture \
    "${BASE_URL}/download/2k_dione.jpg" \
    "$TEXTURES_DIR/moons/dione_2k.jpg" \
    "Dione (2K)" \
    "~5MB"

download_texture \
    "${BASE_URL}/download/2k_tethys.jpg" \
    "$TEXTURES_DIR/moons/tethys_2k.jpg" \
    "Tethys (2K)" \
    "~5MB"

echo ""
echo "=== DWARF PLANETS ==="

# Dwarf Planets - 2K (8K not available for these)
download_texture \
    "${BASE_URL}/download/2k_pluto.jpg" \
    "$TEXTURES_DIR/planets/pluto_2k.jpg" \
    "Pluto (2K)" \
    "~5MB"

download_texture \
    "${BASE_URL}/download/2k_ceres_fictional.jpg" \
    "$TEXTURES_DIR/planets/ceres_2k.jpg" \
    "Ceres (2K)" \
    "~5MB"

download_texture \
    "${BASE_URL}/download/2k_eris_fictional.jpg" \
    "$TEXTURES_DIR/planets/eris_2k.jpg" \
    "Eris (2K - fictional)" \
    "~5MB"

echo ""
echo "=========================================="
echo "Download Summary"
echo "=========================================="
echo ""

# Calculate total size
TOTAL_SIZE=$(du -sh "$TEXTURES_DIR" | cut -f1)
FILE_COUNT=$(find "$TEXTURES_DIR" -name "*.jpg" | wc -l)

echo "✓ Download complete!"
echo "Total files: $FILE_COUNT"
echo "Total size: $TOTAL_SIZE"
echo ""
echo "Resolution improvements:"
echo "  - Sun: 2K → 8K (4x increase)"
echo "  - Major planets: 2K → 8K (4x increase)"  
echo "  - Earth's Moon: 2K → 8K (4x increase)"
echo "  - Other moons: 1K → 2K (2x increase)"
echo "  - Uranus/Neptune: 2K (8K not yet available)"
echo ""
echo "=========================================="
echo "LICENSE INFORMATION"
echo "=========================================="
echo ""
echo "Current textures: Solar System Scope (CC BY 4.0)"
echo "Requires attribution: 'Textures provided by Solar System Scope (https://www.solarsystemscope.com/)'"
echo ""
echo "These textures are BASED ON NASA public domain data, but"
echo "distributed by Solar System Scope under CC BY 4.0."
echo ""
echo "For TRUE public domain (no attribution):"
echo "  Download directly from NASA sources:"
echo "  - https://science.nasa.gov/3d-resources/"
echo "  - https://images.nasa.gov/"
echo "  - https://github.com/nasa/NASA-3D-Resources"
echo ""
echo "Note: Direct NASA sources may not offer 8K resolution for all bodies."
echo "=========================================="
