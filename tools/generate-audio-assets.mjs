import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(import.meta.dirname, "..");
const outputDirectory = resolve(root, "apps/web/static/audio");
const manifestPath = resolve(root, "config/audio-assets.json");
const write = process.argv.includes("--write");

const loops = [
  {
    id: "music-command-loop",
    bus: "music",
    role: "music",
    durationSeconds: 18,
    inputs: [
      "sine=frequency=55:duration=18:sample_rate=48000",
      "sine=frequency=82.5:duration=18:sample_rate=48000",
      "sine=frequency=110:duration=18:sample_rate=48000",
      "anoisesrc=color=pink:amplitude=0.1:duration=18:sample_rate=48000",
    ],
    filter:
      "[0:a]volume=0.14,tremolo=f=0.125:d=0.16[a0];" +
      "[1:a]volume=0.07,tremolo=f=0.25:d=0.12[a1];" +
      "[2:a]volume=0.035,tremolo=f=0.5:d=0.1[a2];" +
      "[3:a]lowpass=f=720,highpass=f=90,volume=0.045[a3];" +
      "[a0][a1][a2][a3]amix=inputs=4:normalize=0,lowpass=f=1100,alimiter=limit=0.72,pan=stereo|c0=c0|c1=c0[out]",
  },
  {
    id: "ambience-ocean-loop",
    bus: "ambience",
    role: "ambience",
    durationSeconds: 18,
    inputs: [
      "anoisesrc=color=pink:amplitude=0.2:duration=18:sample_rate=48000",
      "anoisesrc=color=brown:amplitude=0.16:duration=18:sample_rate=48000",
      "sine=frequency=37:duration=18:sample_rate=48000",
    ],
    filter:
      "[0:a]highpass=f=130,lowpass=f=1400,volume=0.18,tremolo=f=0.1:d=0.18[a0];" +
      "[1:a]highpass=f=40,lowpass=f=380,volume=0.2,tremolo=f=0.13:d=0.14[a1];" +
      "[2:a]volume=0.055,tremolo=f=0.17:d=0.12[a2];" +
      "[a0][a1][a2]amix=inputs=3:normalize=0,alimiter=limit=0.62,pan=stereo|c0=c0|c1=c0[out]",
  },
];

const shortAssets = [
  ["ui-hover", "effects", "ui", 0.12, "0.1*sin(2*PI*(310+260*t)*t)*exp(-32*t)"],
  [
    "ui-select",
    "effects",
    "ui",
    0.2,
    "0.14*sin(2*PI*(430+320*t)*t)*exp(-20*t)+0.05*sin(2*PI*860*t)*exp(-26*t)",
  ],
  [
    "ui-confirm",
    "effects",
    "ui",
    0.38,
    "0.13*sin(2*PI*520*t)*exp(-8*t)+0.1*sin(2*PI*780*t)*exp(-7*t)",
  ],
  [
    "ui-cancel",
    "effects",
    "ui",
    0.3,
    "0.14*sin(2*PI*(260-150*t)*t)*exp(-10*t)",
  ],
  [
    "ui-connected",
    "effects",
    "ui",
    0.36,
    "0.1*sin(2*PI*390*t)*exp(-7*t)+0.08*sin(2*PI*585*t)*exp(-6*t)",
  ],
  [
    "ui-chat",
    "effects",
    "ui",
    0.22,
    "0.09*sin(2*PI*330*t)*exp(-14*t)+0.07*sin(2*PI*495*t)*exp(-12*t)",
  ],
  [
    "ui-target-lock",
    "effects",
    "ui",
    0.32,
    "0.12*sin(2*PI*(620+760*t)*t)*exp(-8*t)+0.05*sin(2*PI*980*t)*exp(-16*t)",
  ],
  ["ui-radar", "effects", "ui", 0.45, "0.09*sin(2*PI*(220+260*t)*t)*exp(-6*t)"],
  [
    "ui-sonar",
    "effects",
    "ui",
    0.8,
    "0.18*sin(2*PI*(145-62*t)*t)*exp(-3.8*t)+0.04*sin(2*PI*435*t)*exp(-6*t)",
  ],
  [
    "ui-place",
    "effects",
    "ui",
    0.34,
    "0.15*sin(2*PI*(300+180*t)*t)*exp(-9*t)+0.05*sin(2*PI*120*t)*exp(-7*t)",
  ],
  [
    "ui-rotate",
    "effects",
    "ui",
    0.28,
    "0.12*sin(2*PI*(510-260*t)*t)*exp(-10*t)",
  ],
  [
    "weapon-fire",
    "effects",
    "weapon",
    0.62,
    "0.28*sin(2*PI*(95-48*t)*t)*exp(-6*t)+0.13*sin(2*PI*240*t)*exp(-15*t)+0.07*sin(2*PI*620*t)*exp(-28*t)",
  ],
  [
    "impact-miss",
    "effects",
    "impact",
    0.72,
    "0.12*sin(2*PI*(190-90*t)*t)*exp(-4*t)+0.05*sin(2*PI*760*t)*exp(-7*t)",
  ],
  [
    "impact-hit",
    "effects",
    "impact",
    0.72,
    "0.26*sin(2*PI*(118-50*t)*t)*exp(-5*t)+0.12*sin(2*PI*360*t)*exp(-12*t)",
  ],
  [
    "vessel-sinking",
    "effects",
    "sinking",
    1.4,
    "0.22*sin(2*PI*(150-48*t)*t)*exp(-2.5*t)+0.13*sin(2*PI*(72-18*t)*t)*exp(-1.8*t)",
  ],
  [
    "cue-turn",
    "voice",
    "accessibility",
    0.34,
    "0.11*sin(2*PI*560*t)*exp(-7*t)+0.09*sin(2*PI*840*t)*exp(-6*t)",
  ],
  [
    "cue-ready",
    "voice",
    "accessibility",
    0.42,
    "0.1*sin(2*PI*610*t)*exp(-6*t)+0.08*sin(2*PI*915*t)*exp(-5*t)",
  ],
  [
    "cue-start",
    "voice",
    "accessibility",
    0.65,
    "0.1*sin(2*PI*220*t)*exp(-3.7*t)+0.1*sin(2*PI*330*t)*exp(-3.3*t)+0.09*sin(2*PI*550*t)*exp(-3*t)",
  ],
  [
    "cue-countdown",
    "voice",
    "accessibility",
    0.22,
    "0.13*sin(2*PI*700*t)*exp(-15*t)+0.05*sin(2*PI*350*t)*exp(-13*t)",
  ],
  [
    "cue-hit",
    "voice",
    "accessibility",
    0.34,
    "0.12*sin(2*PI*880*t)*exp(-8*t)+0.09*sin(2*PI*1320*t)*exp(-9*t)",
  ],
  [
    "cue-miss",
    "voice",
    "accessibility",
    0.38,
    "0.11*sin(2*PI*(410-180*t)*t)*exp(-7*t)",
  ],
  [
    "cue-sunk",
    "voice",
    "accessibility",
    0.7,
    "0.12*sin(2*PI*300*t)*exp(-4*t)+0.1*sin(2*PI*200*t)*exp(-3.8*t)",
  ],
  [
    "victory",
    "effects",
    "victory",
    1.6,
    "0.12*sin(2*PI*440*t)*exp(-1.9*t)+0.11*sin(2*PI*554.37*t)*exp(-1.8*t)+0.1*sin(2*PI*659.25*t)*exp(-1.7*t)",
  ],
  [
    "defeat",
    "effects",
    "defeat",
    1.6,
    "0.18*sin(2*PI*(165-38*t)*t)*exp(-2*t)+0.1*sin(2*PI*(82-16*t)*t)*exp(-1.7*t)",
  ],
  [
    "cue-victory",
    "voice",
    "accessibility",
    0.9,
    "0.1*sin(2*PI*660*t)*exp(-3*t)+0.09*sin(2*PI*990*t)*exp(-2.8*t)",
  ],
  [
    "cue-defeat",
    "voice",
    "accessibility",
    0.9,
    "0.12*sin(2*PI*(360-100*t)*t)*exp(-2.8*t)",
  ],
].map(([id, bus, role, durationSeconds, expression]) => ({
  id,
  bus,
  role,
  durationSeconds,
  expression,
}));

function runFfmpeg(args, id) {
  const result = spawnSync(
    "ffmpeg",
    ["-y", "-hide_banner", "-loglevel", "error", ...args],
    {
      encoding: "utf8",
    },
  );
  if (result.status !== 0) {
    throw new Error(
      `ffmpeg failed for ${id}: ${result.stderr || result.stdout}`,
    );
  }
}

function renderLoop(asset) {
  const inputArgs = asset.inputs.flatMap((input) => [
    "-f",
    "lavfi",
    "-i",
    input,
  ]);
  runFfmpeg(
    [
      ...inputArgs,
      "-filter_complex",
      asset.filter,
      "-map",
      "[out]",
      "-codec:a",
      "libmp3lame",
      "-b:a",
      "64k",
      "-ar",
      "48000",
      resolve(outputDirectory, `${asset.id}.mp3`),
    ],
    asset.id,
  );
}

function renderShort(asset) {
  runFfmpeg(
    [
      "-f",
      "lavfi",
      "-i",
      `aevalsrc=exprs=${asset.expression}:s=48000:d=${asset.durationSeconds}`,
      "-af",
      "highpass=f=35,lowpass=f=6800,alimiter=limit=0.78,pan=stereo|c0=c0|c1=c0",
      "-codec:a",
      "libmp3lame",
      "-b:a",
      "64k",
      "-ar",
      "48000",
      resolve(outputDirectory, `${asset.id}.mp3`),
    ],
    asset.id,
  );
}

function digest(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function buildManifest() {
  const assets = [...loops, ...shortAssets].map(
    ({ id, bus, role, durationSeconds }) => {
      const relativePath = `apps/web/static/audio/${id}.mp3`;
      const path = resolve(root, relativePath);
      return {
        id,
        path: relativePath,
        bus,
        role,
        loop: loops.some((asset) => asset.id === id),
        durationSeconds,
        bytes: statSync(path).size,
        sha256: digest(path),
      };
    },
  );
  return {
    schemaVersion: 1,
    generatedAt: "2026-08-19",
    provenance:
      "Original procedural masters rendered offline for MK.01; no third-party samples.",
    assets,
  };
}

function verify() {
  if (!existsSync(manifestPath))
    throw new Error("audio manifest is missing; run with --write");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const requiredRoles = new Set([
    "music",
    "ambience",
    "ui",
    "weapon",
    "impact",
    "sinking",
    "victory",
    "defeat",
    "accessibility",
  ]);
  let totalBytes = 0;
  for (const asset of manifest.assets ?? []) {
    const path = resolve(root, asset.path);
    if (!existsSync(path))
      throw new Error(`audio asset is missing: ${asset.path}`);
    const bytes = statSync(path).size;
    if (bytes !== asset.bytes || digest(path) !== asset.sha256) {
      throw new Error(`audio asset does not match its manifest: ${asset.path}`);
    }
    if (bytes > 800_000)
      throw new Error(`audio asset exceeds 800 KB: ${asset.path}`);
    totalBytes += bytes;
    requiredRoles.delete(asset.role);
  }
  if (requiredRoles.size)
    throw new Error(
      `audio roles are missing: ${[...requiredRoles].join(", ")}`,
    );
  if (totalBytes > 4_000_000)
    throw new Error(`audio payload exceeds 4 MB: ${totalBytes}`);

  const source = readFileSync(
    resolve(root, "apps/web/src/lib/sound.ts"),
    "utf8",
  );
  if (source.includes("createOscillator"))
    throw new Error("runtime oscillator synthesis is forbidden");
  for (const token of [
    "visibilitychange",
    "devicechange",
    "pagehide",
    "audioMix",
    "audioCues",
    "navigator.vibrate",
  ]) {
    if (!source.includes(token))
      throw new Error(`audio director is missing ${token}`);
  }
  console.log(
    `Audio assets verified: ${manifest.assets.length} files, ${totalBytes} bytes`,
  );
}

if (write) {
  mkdirSync(outputDirectory, { recursive: true });
  for (const asset of loops) renderLoop(asset);
  for (const asset of shortAssets) renderShort(asset);
  const manifest = buildManifest();
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(
    `Rendered ${manifest.assets.length} audio masters to ${outputDirectory}`,
  );
} else {
  verify();
}
