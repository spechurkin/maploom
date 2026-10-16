import {readFile, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

export const root = fileURLToPath(new URL('../', import.meta.url));

export async function appVersion(directory = root) {
    const {version} = JSON.parse(await readFile(path.join(directory, 'package.json'), 'utf8'));
    // SemVer, including prereleases and build metadata. Numeric identifiers cannot have leading zeros.
    const number = '(?:0|[1-9][0-9]*)';
    const identifier = `(?:${number}|[0-9]*[A-Za-z-][0-9A-Za-z-]*)`;
    const semver = new RegExp(`^${number}\\.${number}\\.${number}(?:-${identifier}(?:\\.${identifier})*)?(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?$`);
    if (typeof version !== 'string' || !semver.test(version)) throw new Error('Invalid version in package.json (expected SemVer)');
    return version;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    try {
        console.log(`App version: ${await syncVersion()}`);
    } catch (error) {
        console.error(error.message);
        process.exitCode = 1;
    }
}

// Cargo and npm require literal versions in their manifests/lockfiles. These are generated copies.
export async function syncVersion(directory = root) {
    const version = await appVersion(directory);
    const config = JSON.parse(await readFile(path.join(directory, 'tauri.conf.json'), 'utf8'));
    if (config.version !== 'package.json') throw new Error('tauri.conf.json version must reference package.json');
    const cargoPath = path.join(directory, 'Cargo.toml');
    const cargo = await readFile(cargoPath, 'utf8');
    const packageSection = /(^\[package\][^\S\r\n]*\r?\n)([\s\S]*?)(?=^\[|(?![\s\S]))/m;
    const versionLine = /^(version\s*=\s*")[^"]+("[^\r\n]*)/m;
    if (!versionLine.test(cargo.match(packageSection)?.[2] ?? '')) throw new Error('Missing Cargo package version');
    const nextCargo = cargo.replace(packageSection, (_, header, body) =>
        header + body.replace(versionLine, (_, prefix, suffix) => `${prefix}${version}${suffix}`));

    const lockPath = path.join(directory, 'Cargo.lock');
    const lock = await readFile(lockPath, 'utf8');
    const crate = /(^\[\[package\]\][^\S\r\n]*\r?\nname = "maploom"\r?\nversion = ")[^"]+("[^\r\n]*)/m;
    if (!crate.test(lock)) throw new Error('Missing Maploom package in Cargo.lock');
    const nextLock = lock.replace(crate, (_, prefix, suffix) => `${prefix}${version}${suffix}`);

    const npmPath = path.join(directory, 'package-lock.json');
    const npmText = await readFile(npmPath, 'utf8');
    const npmLock = JSON.parse(npmText);
    if (!npmLock.packages?.['']) throw new Error('Missing root package in package-lock.json');
    let nextNpm = npmText;
    if (npmLock.version !== version || npmLock.packages[''].version !== version) {
        npmLock.version = version;
        npmLock.packages[''].version = version;
        const newline = npmText.includes('\r\n') ? '\r\n' : '\n';
        nextNpm = `${JSON.stringify(npmLock, null, 2)}\n`.replace(/\n/g, newline);
    }

    // Validate all inputs before changing any generated copy. Avoid touching unchanged files.
    for (const [file, before, after] of [[cargoPath, cargo, nextCargo], [lockPath, lock, nextLock], [npmPath, npmText, nextNpm]]) {
        if (before !== after) await writeFile(file, after);
    }
    return version;
}
