import { AXIS_DEFINITIONS } from "./contracts.js";
import { average, clamp, decodePixelInt, hashSignal, percentile, round, stddev } from "./util.js";

function computeLuminance(r, g, b) {
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
}

function computeSaturation(r, g, b) {
  const max = Math.max(r, g, b) / 255;
  const min = Math.min(r, g, b) / 255;
  return max === 0 ? 0 : (max - min) / max;
}

function sampleGrid(image, gridWidth = 48, gridHeight = 60) {
  const samples = [];
  const width = image.bitmap.width;
  const height = image.bitmap.height;

  for (let row = 0; row < gridHeight; row += 1) {
    const y = Math.round((row / Math.max(gridHeight - 1, 1)) * (height - 1));
    const rowSamples = [];

    for (let column = 0; column < gridWidth; column += 1) {
      const x = Math.round((column / Math.max(gridWidth - 1, 1)) * (width - 1));
      const pixel = decodePixelInt(image.getPixelColor(x, y));
      const luminance = computeLuminance(pixel.r, pixel.g, pixel.b);
      const saturation = computeSaturation(pixel.r, pixel.g, pixel.b);
      const nx = column / Math.max(gridWidth - 1, 1);
      const ny = row / Math.max(gridHeight - 1, 1);
      const dx = nx - 0.5;
      const dy = ny - 0.45;
      const distance = Math.sqrt(dx ** 2 + dy ** 2) / 0.72;
      const centerWeight = 1 - clamp(distance, 0, 1);

      rowSamples.push({
        ...pixel,
        luminance,
        saturation,
        nx,
        ny,
        centerWeight
      });
    }

    samples.push(rowSamples);
  }

  return samples;
}

function flattenGrid(grid) {
  return grid.flatMap(row => row);
}

function computeMirrorDifference(grid) {
  const differences = [];
  for (const row of grid) {
    const half = Math.floor(row.length / 2);
    for (let index = 0; index < half; index += 1) {
      const left = row[index];
      const right = row[row.length - 1 - index];
      differences.push(Math.abs(left.luminance - right.luminance));
    }
  }
  return average(differences);
}

function computeEdgeStrength(grid) {
  const strengths = [];

  for (let row = 0; row < grid.length; row += 1) {
    for (let column = 0; column < grid[row].length; column += 1) {
      const current = grid[row][column];
      const right = grid[row][column + 1];
      const down = grid[row + 1]?.[column];

      if (right) {
        strengths.push(Math.abs(current.luminance - right.luminance));
      }

      if (down) {
        strengths.push(Math.abs(current.luminance - down.luminance));
      }
    }
  }

  return average(strengths);
}

function computeCenterPresence(flatSamples) {
  const center = flatSamples.filter(sample => sample.centerWeight >= 0.55);
  const outer = flatSamples.filter(sample => sample.centerWeight < 0.55);
  const centerSignal = average(center.map(sample => sample.saturation * 0.45 + sample.luminance * 0.2 + sample.centerWeight * 0.35));
  const outerSignal = average(outer.map(sample => sample.saturation * 0.4 + sample.luminance * 0.2));
  return clamp(centerSignal - outerSignal + 0.55, 0, 1);
}

function computePaletteMood(flatSamples) {
  const warmth = average(flatSamples.map(sample => (sample.r - sample.b) / 255));
  const vibrance = average(flatSamples.map(sample => sample.saturation));
  return clamp((warmth + 1) * 0.25 + vibrance * 0.65, 0, 1);
}

export function scorePortrait(portrait) {
  const grid = sampleGrid(portrait.image);
  const flat = flattenGrid(grid);
  const luminances = flat.map(sample => sample.luminance);
  const saturations = flat.map(sample => sample.saturation);
  const colorSpread = average(flat.map(sample => stddev([sample.r / 255, sample.g / 255, sample.b / 255])));
  const mirrorDifference = computeMirrorDifference(grid);
  const edgeStrength = computeEdgeStrength(grid);
  const centerPresence = computeCenterPresence(flat);
  const dynamicRange = percentile(luminances, 0.9) - percentile(luminances, 0.1);
  const luminanceDeviation = stddev(luminances);
  const saturationDeviation = stddev(saturations);
  const paletteMood = computePaletteMood(flat);

  const axisScores = {
    symmetry_harmony: round(clamp(100 - mirrorDifference * 145 + hashSignal(portrait.hash, 0), 28, 99)),
    lighting_contrast: round(clamp(dynamicRange * 62 + luminanceDeviation * 85 + hashSignal(portrait.hash, 1), 24, 99)),
    sharpness_detail: round(clamp(edgeStrength * 190 + luminanceDeviation * 18 + hashSignal(portrait.hash, 2), 22, 99)),
    color_vitality: round(clamp(average(saturations) * 76 + saturationDeviation * 70 + colorSpread * 32 + hashSignal(portrait.hash, 3), 18, 99)),
    composition_presence: round(clamp(centerPresence * 100 + edgeStrength * 22 + hashSignal(portrait.hash, 4), 20, 99)),
    style_aura: round(clamp(paletteMood * 48 + centerPresence * 28 + average(saturations) * 22 + dynamicRange * 12 + hashSignal(portrait.hash, 5), 20, 99))
  };

  const weightedTotal = AXIS_DEFINITIONS.reduce((sum, axis) => sum + axisScores[axis.key] * axis.weight, 0);
  const weightSum = AXIS_DEFINITIONS.reduce((sum, axis) => sum + axis.weight, 0);

  return {
    axes: axisScores,
    total: round(weightedTotal / weightSum),
    telemetry: {
      mirrorDifference: round(mirrorDifference, 4),
      edgeStrength: round(edgeStrength, 4),
      centerPresence: round(centerPresence, 4),
      dynamicRange: round(dynamicRange, 4)
    }
  };
}
