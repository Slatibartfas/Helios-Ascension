#!/usr/bin/env python3
"""
Generate placeholder textures for missing moon/asteroid textures.
Creates simple procedural rocky surfaces using PIL/Pillow.
"""

from PIL import Image, ImageDraw, ImageFilter
import random
import os

def create_rocky_texture(width, height, base_color, seed):
    """Create a simple rocky texture with noise"""
    random.seed(seed)
    
    # Create base image
    img = Image.new('RGB', (width, height), base_color)
    pixels = img.load()
    
    # Add noise
    for y in range(height):
        for x in range(width):
            r, g, b = pixels[x, y]
            noise = random.randint(-30, 30)
            pixels[x, y] = (
                max(0, min(255, r + noise)),
                max(0, min(255, g + noise)),
                max(0, min(255, b + noise))
            )
    
    # Apply slight blur for smoother appearance
    img = img.filter(ImageFilter.GaussianBlur(radius=1))
    
    return img

def create_moon_texture(output_path, width, height, color_tuple, name):
    """Create a placeholder moon texture"""
    print(f"Creating placeholder texture for {name}...")
    
    # Create directory if needed
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    
    # Generate texture
    img = create_rocky_texture(width, height, color_tuple, hash(name))
    
    # Save as JPEG
    img.save(output_path, 'JPEG', quality=85)
    print(f"✓ Created {output_path} ({width}x{height})")

# Create directory structure
os.makedirs("celestial/moons", exist_ok=True)
os.makedirs("celestial/asteroids", exist_ok=True)
os.makedirs("celestial/planets", exist_ok=True)

# Moon textures (1K = 1024x512)
moons_1k = [
    ("celestial/moons/io_1k.jpg", "Io", (180, 150, 100)),  # Yellowish
    ("celestial/moons/europa_1k.jpg", "Europa", (200, 190, 180)),  # Light tan
    ("celestial/moons/ganymede_1k.jpg", "Ganymede", (140, 130, 120)),  # Gray-brown
    ("celestial/moons/callisto_1k.jpg", "Callisto", (100, 95, 90)),  # Dark gray
    ("celestial/moons/titan_1k.jpg", "Titan", (180, 140, 100)),  # Orange-tan
    ("celestial/moons/enceladus_1k.jpg", "Enceladus", (240, 240, 240)),  # White
    ("celestial/moons/rhea_1k.jpg", "Rhea", (200, 195, 190)),  # Light gray
    ("celestial/moons/iapetus_1k.jpg", "Iapetus", (150, 145, 140)),  # Gray
    ("celestial/moons/dione_1k.jpg", "Dione", (210, 205, 200)),  # Light gray
    ("celestial/moons/tethys_1k.jpg", "Tethys", (220, 215, 210)),  # Very light gray
]

# Moon textures (2K = 2048x1024)
moons_2k = [
    ("celestial/moons/phobos_2k.jpg", "Phobos", (120, 110, 100)),  # Dark gray
    ("celestial/moons/deimos_2k.jpg", "Deimos", (140, 130, 120)),  # Light gray
    ("celestial/moons/triton_2k.jpg", "Triton", (210, 200, 190)),  # Light pink-gray
    ("celestial/moons/miranda_2k.jpg", "Miranda", (180, 175, 170)),  # Gray
    ("celestial/moons/mimas_2k.jpg", "Mimas", (210, 205, 200)),  # Light gray
    ("celestial/moons/phoebe_2k.jpg", "Phoebe", (80, 75, 70)),  # Very dark gray
]

# Asteroid textures (2K)
asteroids_2k = [
    ("celestial/asteroids/vesta_2k.jpg", "Vesta", (160, 150, 140)),  # Gray-tan
]

# Dwarf planets (2K)
dwarf_planets_2k = [
    ("celestial/planets/pluto_2k.jpg", "Pluto", (190, 170, 150)),  # Tan-gray
]

print("=" * 50)
print("Generating Placeholder Textures")
print("=" * 50)
print()

# Generate all textures
for path, name, color in moons_1k:
    create_moon_texture(path, 1024, 512, color, name)

for path, name, color in moons_2k:
    create_moon_texture(path, 2048, 1024, color, name)

for path, name, color in asteroids_2k:
    create_moon_texture(path, 2048, 1024, color, name)

for path, name, color in dwarf_planets_2k:
    create_moon_texture(path, 2048, 1024, color, name)

print()
print("=" * 50)
print("All placeholder textures generated successfully!")
print("=" * 50)
print()
print("Note: These are placeholder textures.")
print("The procedural variation system will apply additional")
print("color and material variations to make them look unique.")
