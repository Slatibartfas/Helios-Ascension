#!/usr/bin/env python3
"""
Script to add texture paths to the solar_system.ron file.
This script updates the RON file to include texture paths for celestial bodies.
"""

import re
import sys

# Mapping of celestial body names to their texture paths
TEXTURE_MAPPING = {
    # Stars
    "Sol": "textures/celestial/stars/sun_2k.jpg",
    
    # Planets
    "Mercury": "textures/celestial/planets/mercury_2k.jpg",
    "Venus": "textures/celestial/planets/venus_atmosphere_2k.jpg",
    "Earth": "textures/celestial/planets/earth_2k.jpg",
    "Mars": "textures/celestial/planets/mars_2k.jpg",
    "Jupiter": "textures/celestial/planets/jupiter_2k.jpg",
    "Saturn": "textures/celestial/planets/saturn_2k.jpg",
    "Uranus": "textures/celestial/planets/uranus_2k.jpg",
    "Neptune": "textures/celestial/planets/neptune_2k.jpg",
    
    # Dwarf Planets
    "Pluto": "textures/celestial/planets/pluto_1k.jpg",
    "Ceres": "textures/celestial/planets/ceres_1k.jpg",
    "Eris": "textures/celestial/planets/eris_1k.jpg",
    
    # Earth's Moon
    "Moon": "textures/celestial/moons/moon_2k.jpg",
    
    # Jupiter's Galilean Moons
    "Io": "textures/celestial/moons/io_1k.jpg",
    "Europa": "textures/celestial/moons/europa_1k.jpg",
    "Ganymede": "textures/celestial/moons/ganymede_1k.jpg",
    "Callisto": "textures/celestial/moons/callisto_1k.jpg",
    
    # Saturn's Major Moons
    "Titan": "textures/celestial/moons/titan_1k.jpg",
    "Rhea": "textures/celestial/moons/rhea_1k.jpg",
    "Iapetus": "textures/celestial/moons/iapetus_1k.jpg",
    "Dione": "textures/celestial/moons/dione_1k.jpg",
    "Tethys": "textures/celestial/moons/tethys_1k.jpg",
    "Enceladus": "textures/celestial/moons/enceladus_1k.jpg",
}

def process_ron_file(input_file, output_file):
    """
    Process the RON file and add texture paths to celestial bodies.
    
    Args:
        input_file: Path to input RON file
        output_file: Path to output RON file
    """
    with open(input_file, 'r') as f:
        lines = f.readlines()
    
    output_lines = []
    current_body_name = None
    texture_added = False
    bodies_updated = 0
    
    for i, line in enumerate(lines):
        # Check if this is a name line
        name_match = re.match(r'\s*name:\s*"([^"]+)"', line)
        if name_match:
            current_body_name = name_match.group(1)
            texture_added = False
        
        # Check if this is a rotation_period line and we haven't added texture yet
        if current_body_name and not texture_added and re.match(r'\s*rotation_period:', line):
            # This is the rotation_period line
            output_lines.append(line)
            
            # Check if this body has a texture
            if current_body_name in TEXTURE_MAPPING:
                # Get the indentation from the rotation_period line
                indent = len(line) - len(line.lstrip())
                texture_path = TEXTURE_MAPPING[current_body_name]
                texture_line = ' ' * indent + f'texture: Some("{texture_path}"),\n'
                output_lines.append(texture_line)
                texture_added = True
                bodies_updated += 1
                print(f"Added texture for: {current_body_name}")
            
            current_body_name = None  # Reset for next body
            continue
        
        output_lines.append(line)
    
    # Write output
    with open(output_file, 'w') as f:
        f.writelines(output_lines)
    
    print(f"\nUpdated {bodies_updated} celestial bodies with texture paths")
    print(f"Output written to: {output_file}")

if __name__ == "__main__":
    import os
    input_file = "assets/data/solar_system.ron"
    output_file = "assets/data/solar_system.ron"
    
    # Restore from backup first
    import shutil
    backup_file = "assets/data/solar_system.ron.backup"
    if os.path.exists(backup_file):
        shutil.copy2(backup_file, input_file)
        print(f"Restored from backup: {backup_file}")
    
    process_ron_file(input_file, output_file)
    print("Done!")
