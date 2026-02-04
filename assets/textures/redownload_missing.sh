#!/bin/bash

# Script to re-download corrupted texture files with proper validation
# Uses Planet Pixel Emporium textures (free for non-commercial use)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Base URL for Planet Pixel Emporium textures
PPE_URL="https://planetpixelemporium.com"

# Function to download and validate texture
download_texture() {
    local url="$1"
    local output="$2"
    local name="$3"
    
    echo "Downloading $name..."
    
    # Download with proper error handling
    if curl -L -f --retry 3 --retry-delay 2 --max-time 300 -o "$output" "$url" 2>&1; then
        # Validate it's actually a JPEG image
        if [ -s "$output" ] && file "$output" | grep -q "JPEG image data"; then
            local size=$(du -h "$output" | cut -f1)
            echo "✓ Downloaded $name - size: $size"
            return 0
        else
            echo "✗ Failed: $name (not a valid JPEG image)"
            rm -f "$output"
            return 1
        fi
    else
        echo "✗ Failed: $name (download error)"
        rm -f "$output"
        return 1
    fi
}

echo "========================================="
echo "Re-downloading Missing/Corrupted Textures"
echo "========================================="
echo ""
echo "Source: Planet Pixel Emporium (Free for non-commercial use)"
echo "Based on NASA mission data"
echo ""

# Create directories if they don't exist
mkdir -p celestial/moons
mkdir -p celestial/asteroids
mkdir -p celestial/planets

# Download Galilean moons (1K textures from Planet Pixel Emporium)
download_texture "${PPE_URL}/io/iomap.jpg" "celestial/moons/io_1k.jpg" "Io"
download_texture "${PPE_URL}/europa/europamap.jpg" "celestial/moons/europa_1k.jpg" "Europa"
download_texture "${PPE_URL}/ganymede/ganymedemap.jpg" "celestial/moons/ganymede_1k.jpg" "Ganymede"
download_texture "${PPE_URL}/callisto/callistomap.jpg" "celestial/moons/callisto_1k.jpg" "Callisto"

# Download Saturn moons  
download_texture "${PPE_URL}/titan/titanmap.jpg" "celestial/moons/titan_1k.jpg" "Titan"
download_texture "${PPE_URL}/enceladus/enceladusmap.jpg" "celestial/moons/enceladus_1k.jpg" "Enceladus"
download_texture "${PPE_URL}/rhea/rheamap.jpg" "celestial/moons/rhea_1k.jpg" "Rhea"
download_texture "${PPE_URL}/iapetus/iapetusmap.jpg" "celestial/moons/iapetus_1k.jpg" "Iapetus"
download_texture "${PPE_URL}/dione/dionemap.jpg" "celestial/moons/dione_1k.jpg" "Dione"
download_texture "${PPE_URL}/tethys/tethysmap.jpg" "celestial/moons/tethys_1k.jpg" "Tethys"

# Download Mars moons
download_texture "${PPE_URL}/phobos/phobosmap.jpg" "celestial/moons/phobos_2k.jpg" "Phobos"
download_texture "${PPE_URL}/deimos/deimosmap.jpg" "celestial/moons/deimos_2k.jpg" "Deimos"

# Download additional moons
download_texture "${PPE_URL}/triton/tritonmap.jpg" "celestial/moons/triton_2k.jpg" "Triton"
download_texture "${PPE_URL}/miranda/mirandamap.jpg" "celestial/moons/miranda_2k.jpg" "Miranda"
download_texture "${PPE_URL}/mimas/mimasmap.jpg" "celestial/moons/mimas_2k.jpg" "Mimas"
download_texture "${PPE_URL}/phoebe/phoebemap.jpg" "celestial/moons/phoebe_2k.jpg" "Phoebe"

# Download asteroids - Vesta
download_texture "${PPE_URL}/vesta/vestamap.jpg" "celestial/asteroids/vesta_2k.jpg" "Vesta"

# Download dwarf planets - Pluto
download_texture "${PPE_URL}/pluto/plutomap1k.jpg" "celestial/planets/pluto_2k.jpg" "Pluto"

echo ""
echo "========================================="
echo "Download Complete!"
echo "========================================="
echo ""
echo "Verifying all files..."
file celestial/moons/*.jpg celestial/asteroids/*.jpg celestial/planets/pluto_2k.jpg | grep -v "JPEG image data" || echo "All textures validated successfully!"
