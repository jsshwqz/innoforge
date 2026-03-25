import sharp from 'sharp';
import { readFileSync, mkdirSync, existsSync } from 'fs';
import { join } from 'path';

const svgBuffer = readFileSync('static/icon.svg');
const tauriIcons = join('src-tauri', 'icons');
const harmonyIcons = join('harmonyos', 'AppScope', 'resources', 'base', 'media');

// Ensure directories exist
[tauriIcons, harmonyIcons].forEach(dir => {
  mkdirSync(dir, { recursive: true });
});

const sizes = [
  // Tauri desktop
  { size: 32, path: join(tauriIcons, '32x32.png') },
  { size: 128, path: join(tauriIcons, '128x128.png') },
  { size: 256, path: join(tauriIcons, '128x128@2x.png') },
  // Android
  { size: 48, path: join(tauriIcons, 'android-mdpi.png') },
  { size: 72, path: join(tauriIcons, 'android-hdpi.png') },
  { size: 96, path: join(tauriIcons, 'android-xhdpi.png') },
  { size: 144, path: join(tauriIcons, 'android-xxhdpi.png') },
  { size: 192, path: join(tauriIcons, 'android-xxxhdpi.png') },
  // iOS
  { size: 60, path: join(tauriIcons, 'ios-60.png') },
  { size: 120, path: join(tauriIcons, 'ios-60@2x.png') },
  { size: 180, path: join(tauriIcons, 'ios-60@3x.png') },
  { size: 1024, path: join(tauriIcons, 'ios-appstore.png') },
  // HarmonyOS
  { size: 108, path: join(harmonyIcons, 'app_icon.png') },
  { size: 216, path: join(harmonyIcons, 'app_icon_foreground.png') },
];

for (const { size, path } of sizes) {
  await sharp(svgBuffer)
    .resize(size, size)
    .png()
    .toFile(path);
  console.log(`✅ ${size}x${size} → ${path}`);
}

// Generate ICO for Windows (multi-size)
await sharp(svgBuffer).resize(256, 256).png().toFile(join(tauriIcons, 'icon.ico'));
console.log('✅ icon.ico → ' + join(tauriIcons, 'icon.ico'));

// Generate ICNS placeholder (macOS) - just use 512 PNG
await sharp(svgBuffer).resize(512, 512).png().toFile(join(tauriIcons, 'icon.icns'));
console.log('✅ icon.icns → ' + join(tauriIcons, 'icon.icns'));

console.log('\nDone! Generated', sizes.length + 2, 'icons');
