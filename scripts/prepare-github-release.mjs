import {createHash} from 'node:crypto';
import {cp, mkdir, readdir, readFile, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {appVersion} from './app-version.mjs';
import {updaterManifest, updaterTargets, verifyUpdaterSignature} from './updater-release.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const expected = {
    'windows-x64': ['-setup.exe', '.msi', '-portable.zip'],
    'windows-x86': ['-setup.exe', '.msi', '-portable.zip'],
    'macos-x64': ['.dmg', '.app.tar.gz'],
    'macos-arm64': ['.dmg', '.app.tar.gz'],
    'linux-x64': ['.deb', '.rpm', '.AppImage'],
    'linux-arm64': ['.deb', '.rpm', '.AppImage'],
};

try {
    const version = await appVersion();
    const input = path.resolve(root, process.argv[2] || 'artifacts/github-downloads');
    const output = path.resolve(root, process.argv[3] || 'dist/github-release');
    const manifests = [];
    const artifacts = [];
    const signatures = new Map();
    const {plugins} = JSON.parse(await readFile(path.join(root, 'tauri.conf.json'), 'utf8'));
    for (const [platform, suffixes] of Object.entries(expected)) {
        const directory = path.join(input, `Maploom-${platform}`);
        const manifest = JSON.parse(await readFile(path.join(directory, 'manifest.json'), 'utf8'));
        if (manifest.platform !== platform || manifest.version !== version || manifest.withDefaults !== true) {
            throw new Error(`Invalid release manifest (default assets required): ${platform}`);
        }
        const wanted = suffixes.map(suffix => `Maploom-${version}-${platform}${suffix}`);
        wanted.push(...new Set(Object.values(updaterTargets[platform]).map(suffix => `Maploom-${version}-${platform}${suffix}.sig`)));
        if (manifest.artifacts.length !== wanted.length || new Set(manifest.artifacts.map(item => item.file)).size !== wanted.length) {
            throw new Error(`Incorrect artifact count: ${platform}`);
        }
        for (const item of manifest.artifacts) {
            if (!wanted.includes(item.file)) throw new Error(`Unexpected artifact: ${item.file}`);
            const source = path.join(directory, item.file);
            const bytes = await readFile(source);
            const sha256 = createHash('sha256').update(bytes).digest('hex');
            if (!bytes.length || bytes.length !== item.bytes || sha256 !== item.sha256) throw new Error(`Checksum mismatch: ${item.file}`);
            artifacts.push({...item, platform, source});
            if (item.file.endsWith('.sig')) {
                const signature = bytes.toString('utf8').trim();
                verifyUpdaterSignature(await readFile(source.slice(0, -4)), signature, plugins.updater.pubkey);
                signatures.set(item.file, signature);
            }
        }
        manifests.push(manifest);
    }
    const latest = updaterManifest(version, signatures, process.env.GITHUB_REPOSITORY || 'spechurkin/maploom');
    // Require a fresh output; do not publish stale files from a previous run.
    await mkdir(output, {recursive: true});
    if ((await readdir(output)).length) throw new Error(`Release output must be empty: ${output}`);
    for (const artifact of artifacts) await cp(artifact.source, path.join(output, artifact.file));
    await writeFile(path.join(output, 'latest.json'), `${JSON.stringify(latest, null, 2)}\n`);
    await writeFile(path.join(output, 'SHA256SUMS.txt'), artifacts.map(item => `${item.sha256}  ${item.file}\n`).sort().join(''));
    await writeFile(path.join(output, 'manifest.json'), `${JSON.stringify({version, builds: manifests}, null, 2)}\n`);
    console.log(`Verified ${manifests.length} platforms and ${artifacts.length} packages: ${output}`);
} catch (error) {
    console.error(error.message);
    process.exitCode = 1;
}
