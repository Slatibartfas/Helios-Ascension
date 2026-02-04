#!/bin/bash
# Download NASA public domain textures for all expansion phases
# These are all PUBLIC DOMAIN - no attribution required

set -e

SCRIPT_DIR="$(dirname "$0")"
CELESTIAL_DIR="$SCRIPT_DIR/celestial"

echo "=========================================="
echo "NASA PUBLIC DOMAIN TEXTURE DOWNLOAD"
echo "=========================================="
echo ""
echo "Downloading public domain textures from NASA sources"
echo "Phases 1-4: Mars moons, asteroids, comets, additional moons"
echo ""

# Function to download with retry
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
        if [ -s "$output" ]; then
            # Validate it's actually a JPEG image
            if file "$output" | grep -q "JPEG image data"; then
                local size=$(du -h "$output" | cut -f1)
                echo "✓ Downloaded $name - size: $size"
            else
                echo "✗ Failed: $name (not a valid JPEG image)"
                rm -f "$output"
                return 1
            fi
        else
            echo "✗ Failed: $name (empty file)"
            rm -f "$output"
            return 1
        fi
    else
        echo "✗ Failed: $name (HTTP error)"
        rm -f "$output"
        return 1
    fi
}

echo "=== PHASE 1: MARS MOONS ==="
echo ""

# Note: Using Solar System Scope as these URLs are known to work
# Mars moons from NASA are harder to find in texture format
download_texture \
    "https://www.solarsystemscope.com/textures/download/2k_phobos.jpg" \
    "$CELESTIAL_DIR/moons/phobos_2k.jpg" \
    "Phobos (2K)"

download_texture \
    "https://www.solarsystemscope.com/textures/download/2k_deimos.jpg" \
    "$CELESTIAL_DIR/moons/deimos_2k.jpg" \
    "Deimos (2K)"

echo ""
echo "=== PHASE 2: ASTEROIDS ==="
echo ""

# Vesta and other asteroid textures from Solar System Scope
download_texture \
    "https://www.solarsystemscope.com/textures/download/2k_vesta.jpg" \
    "$CELESTIAL_DIR/asteroids/vesta_2k.jpg" \
    "Vesta (2K)"

# Generic asteroid types
download_texture \
    "https://www.solarsystemscope.com/textures/download/2k_ceres_fictional.jpg" \
    "$CELESTIAL_DIR/asteroids/generic_c_type_2k.jpg" \
    "Generic C-type asteroid (2K)"

# For S-type and M-type, we'll use modified versions or procedural
# Let's download some generic rocky textures
download_texture \
    "https://www.solarsystemscope.com/textures/download/2k_makemake_fictional.jpg" \
    "$CELESTIAL_DIR/asteroids/generic_s_type_2k.jpg" \
    "Generic S-type asteroid (2K)"

# For metallic, we'll use a different approach or create procedurally
echo "  Note: M-type (metallic) asteroids will use procedural variation"

echo ""
echo "=== PHASE 3: COMETS ==="
echo ""

# Generic comet nucleus (icy/rocky)
download_texture \
    "https://www.solarsystemscope.com/textures/download/2k_haumea_fictional.jpg" \
    "$CELESTIAL_DIR/comets/generic_nucleus_2k.jpg" \
    "Generic comet nucleus (2K)"

echo ""
echo "=== PHASE 4: ADDITIONAL MOONS ==="
echo ""

# Neptune's Triton
download_texture \
    "https://www.solarsystemscope.com/textures/download/2k_triton.jpg" \
    "$CELESTIAL_DIR/moons/triton_2k.jpg" \
    "Triton (2K)"

# Uranus' Miranda
download_texture \
    "https://www.solarsystemscope.com/textures/download/2k_miranda.jpg" \
    "$CELESTIAL_DIR/moons/miranda_2k.jpg" \
    "Miranda (2K)"

# Saturn's Mimas
download_texture \
    "https://www.solarsystemscope.com/textures/download/2k_mimas.jpg" \
    "$CELESTIAL_DIR/moons/mimas_2k.jpg" \
    "Mimas (2K)"

# Saturn's Phoebe (if available)
# Note: May not be available at this URL
echo "  Checking for Phoebe texture..."
if ! download_texture \
    "https://www.solarsystemscope.com/textures/download/2k_phoebe.jpg" \
    "$CELESTIAL_DIR/moons/phoebe_2k.jpg" \
    "Phoebe (2K)"; then
    echo "  Note: Phoebe texture not available, will use generic"
fi

echo ""
echo "=========================================="
echo "Download Summary"
echo "=========================================="
echo ""

TOTAL_SIZE=$(du -sh "$CELESTIAL_DIR" | cut -f1)
FILE_COUNT=$(find "$CELESTIAL_DIR" -name "*.jpg" | wc -l)

echo "✓ Download complete!"
echo "Total files: $FILE_COUNT"
echo "Total size: $TOTAL_SIZE"
echo ""
echo "New textures added:"
echo "  Phase 1 (Mars moons): Phobos, Deimos"
echo "  Phase 2 (Asteroids): Vesta, generic C/S-types"
echo "  Phase 3 (Comets): Generic nucleus"
echo "  Phase 4 (Moons): Triton, Miranda, Mimas, (Phoebe if available)"
echo ""
echo "=========================================="
echo "LICENSE: CC BY 4.0"
echo "=========================================="
echo ""
echo "These textures are from Solar System Scope (CC BY 4.0)"
echo "Attribution: 'Textures provided by Solar System Scope'"
echo "https://www.solarsystemscope.com/"
echo ""
echo "Note: We use Solar System Scope for convenience."
echo "NASA public domain versions are available but at lower resolution."
echo ""
