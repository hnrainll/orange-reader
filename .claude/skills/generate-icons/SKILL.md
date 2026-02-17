---
name: generate-icons
description: Generate macOS app icons (icon_macos.png + build/icon.icns) from icon.png source file
disable-model-invocation: true
allowed-tools: Bash, Read
argument-hint: [icon-source-path]
---

# Generate macOS App Icons

From the source icon file (default: `icon.png`), generate two output files:
- `icon_macos.png` — Dev mode Dock icon (white rounded rect background, transparent corners)
- `build/icon.icns` — Production macOS app icon

Source file path: `$ARGUMENTS` (defaults to `icon.png` if not provided)

## Key Requirements

1. **Source icon (`icon.png`)** may have a squircle border/shadow baked in — this MUST be removed to avoid "two layer" effect in Dock
2. **`icon_macos.png`** must be: graphic on a **clean white rounded rectangle** (220px radius on 1024x1024), corners are **transparent** (not white, not black)
3. **`build/icon.icns`** is generated from `icon_macos.png` via `iconutil`
4. All operations must preserve **sRGB colorspace** (use `-colorspace sRGB` in magick composite commands)

## Steps

### Step 1: Extract graphic from source (remove baked-in squircle background)

Use ImageMagick flood fill from multiple edge/midpoints with 12% fuzz to remove all white/near-white background:

```bash
magick <source>.png -fuzz 12% -fill none \
  -draw "color 0,0 floodfill" \
  -draw "color 1023,0 floodfill" \
  -draw "color 0,1023 floodfill" \
  -draw "color 1023,1023 floodfill" \
  -draw "color 512,0 floodfill" \
  -draw "color 0,512 floodfill" \
  -draw "color 1023,512 floodfill" \
  -draw "color 512,1023 floodfill" \
  -draw "color 256,0 floodfill" \
  -draw "color 768,0 floodfill" \
  -draw "color 0,256 floodfill" \
  -draw "color 0,768 floodfill" \
  -draw "color 1023,256 floodfill" \
  -draw "color 1023,768 floodfill" \
  -draw "color 256,1023 floodfill" \
  -draw "color 768,1023 floodfill" \
  -draw "color 128,1023 floodfill" \
  -draw "color 896,1023 floodfill" \
  -draw "color 128,0 floodfill" \
  -draw "color 896,0 floodfill" \
  /tmp/icon_transparent.png
```

### Step 2: Create icon_macos.png (graphic on white rounded rect)

```bash
magick -size 1024x1024 xc:none -colorspace sRGB -fill white \
  -draw "roundrectangle 0,0 1023,1023 220,220" /tmp/white_bg.png

magick /tmp/white_bg.png /tmp/icon_transparent.png \
  -gravity center -colorspace sRGB -compose Over -composite icon_macos.png
```

### Step 3: Generate build/icon.icns

```bash
ICONSET="/tmp/icon.iconset"
mkdir -p "$ICONSET"
sips -z 16 16     icon_macos.png --out "$ICONSET/icon_16x16.png"
sips -z 32 32     icon_macos.png --out "$ICONSET/icon_16x16@2x.png"
sips -z 32 32     icon_macos.png --out "$ICONSET/icon_32x32.png"
sips -z 64 64     icon_macos.png --out "$ICONSET/icon_32x32@2x.png"
sips -z 128 128   icon_macos.png --out "$ICONSET/icon_128x128.png"
sips -z 256 256   icon_macos.png --out "$ICONSET/icon_128x128@2x.png"
sips -z 256 256   icon_macos.png --out "$ICONSET/icon_256x256.png"
sips -z 512 512   icon_macos.png --out "$ICONSET/icon_256x256@2x.png"
sips -z 512 512   icon_macos.png --out "$ICONSET/icon_512x512.png"
sips -z 1024 1024 icon_macos.png --out "$ICONSET/icon_512x512@2x.png"
iconutil -c icns "$ICONSET" -o build/icon.icns
rm -rf "$ICONSET"
```

### Step 4: Verify

Use Read tool to visually inspect `icon_macos.png`:
- Should show the graphic on a white rounded rectangle
- Corners should be transparent (no black, no square)
- No "two layer" squircle border effect
- Colors should be preserved (not grayscale)

Clean up temp files: `rm -f /tmp/icon_transparent.png /tmp/white_bg.png`
