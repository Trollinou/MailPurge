#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(__dirname, '..');

const cargoTomlPath = path.join(rootDir, 'Cargo.toml');
const tauriConfPath = path.join(rootDir, 'src-tauri', 'tauri.conf.json');
const packageJsonPath = path.join(rootDir, 'package.json');
const packageLockPath = path.join(rootDir, 'package-lock.json');

const targetVersion = process.argv[2]?.trim();

let cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');
let newVersion = targetVersion;

if (targetVersion) {
  // L'utilisateur a passé une version en argument : on met à jour Cargo.toml en premier
  const workspacePackageRegex = /(\[workspace\.package\][\s\S]*?version\s*=\s*)"[^"]+"/;
  if (!workspacePackageRegex.test(cargoToml)) {
    console.error('❌ Impossible de trouver [workspace.package] version dans Cargo.toml');
    process.exit(1);
  }
  cargoToml = cargoToml.replace(workspacePackageRegex, `$1"${targetVersion}"`);
  fs.writeFileSync(cargoTomlPath, cargoToml, 'utf8');
  console.log(`✅ Cargo.toml mis à jour : -> ${targetVersion}`);
} else {
  // Sans argument : on extrait la version de [workspace.package] dans Cargo.toml
  const match = cargoToml.match(/\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"/);
  if (!match) {
    console.error("❌ Impossible d'extraire la version de [workspace.package] dans Cargo.toml");
    process.exit(1);
  }
  newVersion = match[1];
  console.log(`ℹ️  Version source détectée dans Cargo.toml : ${newVersion}`);
}

// 1. Mise à jour de src-tauri/tauri.conf.json
if (fs.existsSync(tauriConfPath)) {
  const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
  const oldVersion = tauriConf.version;
  tauriConf.version = newVersion;
  fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n', 'utf8');
  console.log(`✅ src-tauri/tauri.conf.json : ${oldVersion} -> ${newVersion}`);
}

// 2. Mise à jour de package.json
if (fs.existsSync(packageJsonPath)) {
  const pkg = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
  const oldVersion = pkg.version;
  pkg.version = newVersion;
  fs.writeFileSync(packageJsonPath, JSON.stringify(pkg, null, 2) + '\n', 'utf8');
  console.log(`✅ package.json : ${oldVersion} -> ${newVersion}`);
}

// 3. Mise à jour de package-lock.json
if (fs.existsSync(packageLockPath)) {
  const lock = JSON.parse(fs.readFileSync(packageLockPath, 'utf8'));
  lock.version = newVersion;
  if (lock.packages && lock.packages['']) {
    lock.packages[''].version = newVersion;
  }
  fs.writeFileSync(packageLockPath, JSON.stringify(lock, null, 2) + '\n', 'utf8');
  console.log(`✅ package-lock.json : -> ${newVersion}`);
}

console.log(`\n🎉 Synchronisation terminée ! Tous les fichiers sont sur la version ${newVersion}`);
