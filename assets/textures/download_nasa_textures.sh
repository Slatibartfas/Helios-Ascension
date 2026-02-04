#!/bin/bash

# Download textures from NASA/JPL planetary data sources
# These are public domain, no attribution required

set -e

TEXTURE_DIR="./celestial"
mkdir -p "$TEXTURE_DIR/moons"
mkdir -p "$TEXTURE_DIR/planets"
mkdir -p "$TEXTURE_DIR/asteroids"

echo "=========================================="
echo "NASA PUBLIC DOMAIN TEXTURE DOWNLOAD"
echo "=========================================="
echo ""
echo "Downloading from NASA/JPL sources"
echo "License: Public Domain"
echo ""

# Function to download with validation
download_texture() {
    local url="$1"
    local output="$2"
    local name="$3"
    
    if [ -f "$output" ]; then
        # Check if it's a real JPEG (not placeholder)
        local size=$(stat -f%z "$output" 2>/dev/null || stat -c%s "$output" 2>/dev/null)
        if [ "$size" -gt 500000 ]; then
            echo "✓ $name already exists ($(du -h "$output" | cut -f1)), skipping..."
            return 0
        else
            echo "  Replacing placeholder for $name..."
        fi
    fi
    
    echo "Downloading $name..."
    if curl -f -L -o "$output.tmp" "$url" 2>/dev/null; then
        # Verify it's actually a JPEG
        if file "$output.tmp" | grep -q "JPEG\|PNG"; then
            mv "$output.tmp" "$output"
            echo "✓ $name downloaded successfully ($(du -h "$output" | cut -f1))"
        else
            echo "✗ $name download failed (not a valid image)"
            rm -f "$output.tmp"
            return 1
        fi
    else
        echo "✗ $name download failed (HTTP error)"
        rm -f "$output.tmp"
        return 1
    fi
}

echo "=== JUPITER'S MOONS ==="
echo ""

# Io - from Galileo mission
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Io_GalileoSSI_Global_Mosaic_1km.jpg" \
    "$TEXTURE_DIR/moons/io_1k.jpg" \
    "Io (1K)"

# Europa - from Galileo mission
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Europa_Voyager_GalileoSSI_global_mosaic_500m.jpg" \
    "$TEXTURE_DIR/moons/europa_1k.jpg" \
    "Europa (1K)"

# Ganymede - from Voyager/Galileo
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Ganymede_Voyager_GalileoSSI_global_mosaic_1km.jpg" \
    "$TEXTURE_DIR/moons/ganymede_1k.jpg" \
    "Ganymede (1K)"

# Callisto - from Voyager/Galileo
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Callisto_Voyager_Galileo_global_mosaic_1km.jpg" \
    "$TEXTURE_DIR/moons/callisto_1k.jpg" \
    "Callisto (1K)"

echo ""
echo "=== SATURN'S MOONS ==="
echo ""

# Titan - from Cassini
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Titan_ISS_P19658_Mosaic_Global_1km.jpg" \
    "$TEXTURE_DIR/moons/titan_1k.jpg" \
    "Titan (1K)"

# Enceladus - from Cassini
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Enceladus_Cassini_mosaic_global_110m.jpg" \
    "$TEXTURE_DIR/moons/enceladus_1k.jpg" \
    "Enceladus (1K)"

# Mimas - from Cassini
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Mimas_Cassini_mosaic_global_405m.jpg" \
    "$TEXTURE_DIR/moons/mimas_2k.jpg" \
    "Mimas (2K)"

# Tethys - from Cassini
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Tethys_Cassini_mosaic_global_500m.jpg" \
    "$TEXTURE_DIR/moons/tethys_1k.jpg" \
    "Tethys (1K)"

# Dione - from Cassini
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Dione_Cassini_mosaic_global_417m.jpg" \
    "$TEXTURE_DIR/moons/dione_1k.jpg" \
    "Dione (1K)"

# Rhea - from Cassini
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Rhea_Cassini_mosaic_global_417m.jpg" \
    "$TEXTURE_DIR/moons/rhea_1k.jpg" \
    "Rhea (1K)"

# Iapetus - from Cassini
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Iapetus_Cassini_mosaic_global_926m.jpg" \
    "$TEXTURE_DIR/moons/iapetus_1k.jpg" \
    "Iapetus (1K)"

# Phoebe - from Cassini
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Phoebe_Cassini_mosaic_global_232m.jpg" \
    "$TEXTURE_DIR/moons/phoebe_2k.jpg" \
    "Phoebe (2K)"

echo ""
echo "=== MARS MOONS ==="
echo ""

# Phobos - from Mars Reconnaissance Orbiter
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Phobos_ME_HRSC_mosaic_global_2ppd.jpg" \
    "$TEXTURE_DIR/moons/phobos_2k.jpg" \
    "Phobos (2K)"

# Deimos - from Mars Reconnaissance Orbiter
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Deimos_Viking_mosaic_global_0.2ppd.jpg" \
    "$TEXTURE_DIR/moons/deimos_2k.jpg" \
    "Deimos (2K)"

echo ""
echo "=== URANUS MOONS ==="
echo ""

# Miranda - from Voyager 2
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Miranda_Voyager2_mosaic_global_500m.jpg" \
    "$TEXTURE_DIR/moons/miranda_2k.jpg" \
    "Miranda (2K)"

echo ""
echo "=== NEPTUNE MOONS ==="
echo ""

# Triton - from Voyager 2
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Triton_Voyager2_mosaic_global_1500m.jpg" \
    "$TEXTURE_DIR/moons/triton_2k.jpg" \
    "Triton (2K)"

echo ""
echo "=== DWARF PLANETS ==="
echo ""

# Pluto - from New Horizons
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Pluto_NewHorizons_Global_Mosaic_300m_Jul2017.jpg" \
    "$TEXTURE_DIR/planets/pluto_2k.jpg" \
    "Pluto (2K)"

echo ""
echo "=== ASTEROIDS ==="
echo ""

# Vesta - from Dawn mission
download_texture \
    "https://planetarymaps.usgs.gov/mosaic/Vesta_Dawn_FC_HAMO_global_100m.jpg" \
    "$TEXTURE_DIR/asteroids/vesta_2k.jpg" \
    "Vesta (2K)"

echo ""
echo "=========================================="
echo "Download Summary"
echo "=========================================="
echo ""
echo "NASA/JPL textures are PUBLIC DOMAIN"
echo "No attribution required!"
echo ""
echo "Total downloaded textures available"
ls -lh "$TEXTURE_DIR"/*/*.jpg 2>/dev/null | wc -l | xargs echo "Files:"
du -sh "$TEXTURE_DIR" | awk '{print "Total size:", $1}'
echo ""
echo "✓ Download complete!"
