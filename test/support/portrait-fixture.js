export function createFixturePalette(name) {
  const palettes = {
    left: {
      background: 0xf4d6c8ff,
      accent: 0x804f3eff,
      highlight: 0xfff1e6ff
    },
    right: {
      background: 0x1c2e4aff,
      accent: 0xe36f47ff,
      highlight: 0xffd79fff
    },
    vivid: {
      background: 0x12305eff,
      accent: 0x62e3ccff,
      highlight: 0xff5c5cff
    }
  };

  return palettes[name];
}

export async function createPortraitDataUrl(JimpCtor, palette) {
  const image = new JimpCtor({ width: 128, height: 160, color: palette.background });

  for (let y = 0; y < image.bitmap.height; y += 1) {
    for (let x = 0; x < image.bitmap.width; x += 1) {
      const inFace = x > 30 && x < 98 && y > 22 && y < 138;
      const inHair = x > 18 && x < 110 && y > 8 && y < 70;
      const inShoulders = y > 106 && x > 10 && x < 118;
      const inEyeBand = y > 56 && y < 72 && x > 38 && x < 90;
      const inAura = Math.abs(x - 64) + Math.abs(y - 72) < 54;

      if (inAura) {
        image.setPixelColor(palette.accent, x, y);
      }

      if (inHair) {
        image.setPixelColor(palette.accent, x, y);
      }

      if (inFace) {
        image.setPixelColor(palette.highlight, x, y);
      }

      if (inEyeBand && (x < 52 || x > 76)) {
        image.setPixelColor(palette.background, x, y);
      }

      if (inShoulders) {
        image.setPixelColor(palette.accent, x, y);
      }
    }
  }

  return image.getBase64("image/png");
}
