#!/bin/bash
# Script to download celestial body textures from NASA public domain sources
# These textures are PUBLIC DOMAIN - no attribution required
# Primary Sources: 
#   - NASA 3D Resources: https://science.nasa.gov/3d-resources/
#   - NASA Solar System Exploration: https://solarsystem.nasa.gov/
#   - Solar System Scope (CC BY 4.0 - requires attribution): https://www.solarsystemscope.com/textures/

set -e

TEXTURES_DIR="$(dirname "$0")/celestial"
# Using Solar System Scope as a reliable mirror of NASA data
# Note: These are based on NASA public domain data but distributed by SSS under CC BY 4.0
BASE_URL="https://www.solarsystemscope.com/textures"

echo "Downloading celestial body textures..."
echo "Source: Based on NASA public domain data, distributed via Solar System Scope"
echo "License: Mix of Public Domain (NASA) and CC BY 4.0 (Solar System Scope)"
echo "Target directory: $TEXTURES_DIR"
echo ""
echo "Note: We prioritize NASA public domain sources where available."
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
    
    if [ -f "$output" ]; then
        echo "✓ $name already exists, skipping..."
        return 0
    fi
    
    echo "Downloading $name..."
    if curl -L -f --retry 3 --retry-delay 2 --max-time 300 -o "$output" "$url" 2>/dev/null; then
        # Check if file was actually downloaded and has content
        if [ -s "$output" ]; then
            # Validate it's actually a JPEG image
            if file "$output" | grep -q "JPEG image data"; then
                local size=$(du -h "$output" | cut -f1)
                echo "✓ Downloaded $name - size: $size"
            else
                echo "✗ Failed to download $name (not a valid JPEG image)"
                rm -f "$output"
                return 1
            fi
        else
            echo "✗ Failed to download $name (empty file)"
            rm -f "$output"
            return 1
        fi
    else
        echo "✗ Failed to download $name from $url (HTTP error)"
        rm -f "$output"
        return 1
    fi
}

# Download Sun
download_texture \
    "${BASE_URL}/download/2k_sun.jpg" \
    "$TEXTURES_DIR/stars/sun_2k.jpg" \
    "Sun (2k)"

# Download Planets
download_texture \
    "${BASE_URL}/download/2k_mercury.jpg" \
    "$TEXTURES_DIR/planets/mercury_2k.jpg" \
    "Mercury (2k)"

download_texture \
    "${BASE_URL}/download/2k_venus_surface.jpg" \
    "$TEXTURES_DIR/planets/venus_surface_2k.jpg" \
    "Venus Surface (2k)"

download_texture \
    "${BASE_URL}/download/2k_venus_atmosphere.jpg" \
    "$TEXTURES_DIR/planets/venus_atmosphere_2k.jpg" \
    "Venus Atmosphere (2k)"

download_texture \
    "${BASE_URL}/download/2k_earth_daymap.jpg" \
    "$TEXTURES_DIR/planets/earth_2k.jpg" \
    "Earth (2k)"

download_texture \
    "${BASE_URL}/download/2k_mars.jpg" \
    "$TEXTURES_DIR/planets/mars_2k.jpg" \
    "Mars (2k)"

download_texture \
    "${BASE_URL}/download/2k_jupiter.jpg" \
    "$TEXTURES_DIR/planets/jupiter_2k.jpg" \
    "Jupiter (2k)"

download_texture \
    "${BASE_URL}/download/2k_saturn.jpg" \
    "$TEXTURES_DIR/planets/saturn_2k.jpg" \
    "Saturn (2k)"

download_texture \
    "${BASE_URL}/download/2k_uranus.jpg" \
    "$TEXTURES_DIR/planets/uranus_2k.jpg" \
    "Uranus (2k)"

download_texture \
    "${BASE_URL}/download/2k_neptune.jpg" \
    "$TEXTURES_DIR/planets/neptune_2k.jpg" \
    "Neptune (2k)"

# Download Major Moons
download_texture \
    "${BASE_URL}/download/2k_moon.jpg" \
    "$TEXTURES_DIR/moons/moon_2k.jpg" \
    "Moon (2k)"

# Jupiter's moons
download_texture \
    "${BASE_URL}/download/2k_io.jpg" \
    "$TEXTURES_DIR/moons/io_1k.jpg" \
    "Io"

download_texture \
    "${BASE_URL}/download/2k_europa.jpg" \
    "$TEXTURES_DIR/moons/europa_1k.jpg" \
    "Europa"

download_texture \
    "${BASE_URL}/download/2k_ganymede.jpg" \
    "$TEXTURES_DIR/moons/ganymede_1k.jpg" \
    "Ganymede"

download_texture \
    "${BASE_URL}/download/2k_callisto.jpg" \
    "$TEXTURES_DIR/moons/callisto_1k.jpg" \
    "Callisto"

# Saturn's moons
download_texture \
    "${BASE_URL}/download/2k_titan.jpg" \
    "$TEXTURES_DIR/moons/titan_1k.jpg" \
    "Titan"

download_texture \
    "${BASE_URL}/download/2k_enceladus.jpg" \
    "$TEXTURES_DIR/moons/enceladus_1k.jpg" \
    "Enceladus"

download_texture \
    "${BASE_URL}/download/2k_rhea.jpg" \
    "$TEXTURES_DIR/moons/rhea_1k.jpg" \
    "Rhea"

download_texture \
    "${BASE_URL}/download/2k_iapetus.jpg" \
    "$TEXTURES_DIR/moons/iapetus_1k.jpg" \
    "Iapetus"

download_texture \
    "${BASE_URL}/download/2k_dione.jpg" \
    "$TEXTURES_DIR/moons/dione_1k.jpg" \
    "Dione"

download_texture \
    "${BASE_URL}/download/2k_tethys.jpg" \
    "$TEXTURES_DIR/moons/tethys_1k.jpg" \
    "Tethys"

# Dwarf Planets
download_texture \
    "${BASE_URL}/download/2k_pluto.jpg" \
    "$TEXTURES_DIR/planets/pluto_1k.jpg" \
    "Pluto"

download_texture \
    "${BASE_URL}/download/2k_ceres.jpg" \
    "$TEXTURES_DIR/planets/ceres_1k.jpg" \
    "Ceres"

download_texture \
    "${BASE_URL}/download/2k_eris.jpg" \
    "$TEXTURES_DIR/planets/eris_1k.jpg" \
    "Eris"

# Generic asteroid and comet textures
# Note: Solar System Scope may not have these specific names, so we'll use alternatives
# or create placeholder notes for manual download

echo ""
echo "Note: Generic asteroid and comet textures may need to be downloaded manually"
echo "or created as procedural textures if not available."
echo ""
echo "For asteroids and comets, you can:"
echo "1. Use NASA's 3D Resources: https://science.nasa.gov/3d-resources/"
echo "2. Use NASA Image Library: https://images.nasa.gov/"
echo "3. Use USGS Astrogeology: https://astrogeology.usgs.gov/"
echo "4. Create procedural textures based on mission imagery"
echo ""

echo "Texture download complete!"
echo ""
echo "LICENSE INFORMATION:"
echo "==================="
echo "Textures are primarily based on NASA public domain data."
echo "Solar System Scope distributes these under CC BY 4.0."
echo ""
echo "For LESS RESTRICTIVE alternatives:"
echo "- Download directly from NASA sources (Public Domain, no attribution required):"
echo "  * https://science.nasa.gov/3d-resources/"
echo "  * https://images.nasa.gov/"
echo "  * https://github.com/nasa/NASA-3D-Resources"
echo ""
echo "Current textures require attribution:"
echo "  'Textures provided by Solar System Scope (https://www.solarsystemscope.com/)'"
