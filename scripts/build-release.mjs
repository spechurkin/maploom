import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {cp, mkdir, mkdtemp, readdir, readFile, rm, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {syncVersion} from './app-version.mjs';
import {updaterTargets, verifyUpdaterSignature} from './updater-release.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const targets = {
    'windows-x64': {host: 'win32', triple: 'x86_64-pc-windows-msvc', bundles: ['nsis', 'msi']},
    'windows-x86': {host: 'win32', triple: 'i686-pc-windows-msvc', bundles: ['nsis', 'msi']},
    'macos-x64': {host: 'darwin', triple: 'x86_64-apple-darwin', bundles: ['app', 'dmg']},
    'macos-arm64': {host: 'darwin', triple: 'aarch64-apple-darwin', bundles: ['app', 'dmg']},
    'linux-x64': {host: 'linux', triple: 'x86_64-unknown-linux-gnu', bundles: ['deb', 'rpm', 'appimage']},
    'linux-arm64': {host: 'linux', triple: 'aarch64-unknown-linux-gnu', bundles: ['deb', 'rpm', 'appimage']},
};

function run(command, args, env = {}) {
    console.log(`> ${command} ${args.join(' ')}`);
    const result = spawnSync(command, args, {cwd: root, stdio: 'inherit', env: {...process.env, ...env}});
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(`${command} failed (${result.status ?? result.signal})`);
}

function within(directory, candidate) {
    const relative = path.relative(directory, candidate);
    if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) throw new Error(`Unsafe output path: ${candidate}`);
    return candidate;
}

async function files(directory) {
    const result = [];
    for (const item of await readdir(directory, {withFileTypes: true})) {
        const entry = path.join(directory, item.name);
        if (item.isDirectory()) result.push(...await files(entry));
        else if (item.isFile()) result.push(entry);
    }
    return result;
}

async function verifyExecutable(binary, target) {
    const bytes = await readFile(binary);
    if (target.host === 'win32') {
        if (bytes.toString('ascii', 0, 2) !== 'MZ') throw new Error('Missing PE executable');
        const offset = bytes.readUInt32LE(0x3c);
        if (bytes.toString('ascii', offset, offset + 4) !== 'PE\0\0') throw new Error('Invalid PE header');
        const expected = target.triple.startsWith('i686') ? 0x14c : 0x8664;
        if (bytes.readUInt16LE(offset + 4) !== expected) throw new Error('Wrong Windows architecture');
    } else if (target.host === 'linux') {
        if (bytes.toString('hex', 0, 4) !== '7f454c46') throw new Error('Missing ELF executable');
        const expected = target.triple.startsWith('aarch64') ? 183 : 62;
        if (bytes[4] !== 2 || bytes.readUInt16LE(18) !== expected) throw new Error('Wrong Linux architecture');
    } else {
        if (bytes.readUInt32LE(0) !== 0xfeedfacf) throw new Error('Missing Mach-O executable');
        const expected = target.triple.startsWith('aarch64') ? 0x100000c : 0x1000007;
        if (bytes.readUInt32LE(4) !== expected) throw new Error('Wrong macOS architecture');
    }
}

async function build(name, options, version) {
    const target = targets[name];
    if (!target) throw new Error(`Unknown target ${name}. Choose: ${Object.keys(targets).join(', ')}`);
    if (target.host !== process.platform) throw new Error(`${name} must be built on ${target.host}; current host: ${process.platform}`);
    const targetDir = path.resolve(root, process.env.CARGO_TARGET_DIR || 'artifacts/build');
    const release = path.join(targetDir, target.triple, 'release');
    const bundle = within(targetDir, path.join(release, 'bundle'));
    const output = within(path.join(root, 'dist'), path.join(root, 'dist', name));
    const binary = path.join(release, target.host === 'win32' ? 'maploom.exe' : 'maploom');
    const resourceRecord = path.join(release, 'maploom-bundle-resources.json');
    const defaults = path.join(root, 'resources/defaults.mloom_fabric');
    const defaultsSha256 = createHash('sha256').update(await readFile(defaults)).digest('hex');
    if (!options.collectOnly) {
        // Official release builds always carry signatures; ordinary tauri builds need no key.
        const signingKey = process.env.TAURI_SIGNING_PRIVATE_KEY || path.join(root, '.secrets/maploom-updater.key');
        if (!process.env.TAURI_SIGNING_PRIVATE_KEY) await readFile(signingKey);
        // Remove only this target's bundles so stale installers cannot pass validation.
        await rm(bundle, {recursive: true, force: true});
        await rm(resourceRecord, {force: true});
        run('rustup', ['target', 'add', target.triple]);
        const args = ['node_modules/@tauri-apps/cli/tauri.js', 'build', '--ci', '--target', target.triple,
            '--bundles', target.bundles.join(','), '--config', JSON.stringify({bundle: {createUpdaterArtifacts: true}})];
        args.push('--', '--locked');
        run(process.execPath, args, {
            CARGO_TARGET_DIR: targetDir,
            APPIMAGE_EXTRACT_AND_RUN: '1',
            TAURI_SIGNING_PRIVATE_KEY: signingKey,
            TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD || ''
        });
        await writeFile(resourceRecord, JSON.stringify({defaultsSha256, version}));
    } else {
        const record = JSON.parse(await readFile(resourceRecord, 'utf8'));
        if (record.defaultsSha256 !== defaultsSha256 || record.version !== version) {
            throw new Error('Bundle resources or version differ from the requested release. Rebuild without --collect-only.');
        }
    }
    await verifyExecutable(binary, target);
    const bundled = await files(bundle);
    const prefix = `Maploom-${version}-${name}`;
    const artifacts = [];
    // Validate every expected format before replacing the last successful output.
    for (const format of target.bundles.filter(format => format !== 'app')) {
        const suffix = {
            nsis: '.exe',
            msi: '.msi',
            dmg: '.dmg',
            deb: '.deb',
            rpm: '.rpm',
            appimage: '.AppImage'
        }[format];
        const matches = bundled.filter(file => path.extname(file) === suffix && path.basename(path.dirname(file)) === format);
        if (matches.length !== 1) throw new Error(`Expected one ${format} bundle, found ${matches.length}`);
        artifacts.push({source: matches[0], name: `${prefix}${format === 'nsis' ? '-setup' : ''}${suffix}`});
    }
    if (target.host === 'darwin') {
        const appBinary = path.join(bundle, 'macos/Maploom.app/Contents/MacOS/maploom');
        await verifyExecutable(appBinary, target);
        run('codesign', ['--verify', '--deep', '--strict', path.join(bundle, 'macos/Maploom.app')]);
        const archive = bundled.filter(file => file.endsWith('.app.tar.gz'));
        if (archive.length !== 1) throw new Error('Expected one signed macOS updater archive');
        artifacts.push({source: archive[0], name: `${prefix}.app.tar.gz`});
    }
    const {plugins} = JSON.parse(await readFile(path.join(root, 'tauri.conf.json'), 'utf8'));
    for (const suffix of new Set(Object.values(updaterTargets[name]))) {
        const artifact = artifacts.find(item => item.name === `${prefix}${suffix}`);
        if (!artifact) throw new Error(`Missing updater artifact: ${suffix}`);
        if (suffix === '.deb' || suffix === '.rpm') {
            const privateKey = process.env.TAURI_SIGNING_PRIVATE_KEY || path.join(root, '.secrets/maploom-updater.key');
            let keyContents;
            try {
                keyContents = await readFile(privateKey, 'utf8');
            } catch (error) {
                if (!process.env.TAURI_SIGNING_PRIVATE_KEY) throw error;
                keyContents = process.env.TAURI_SIGNING_PRIVATE_KEY;
            }
            run(process.execPath, ['node_modules/@tauri-apps/cli/tauri.js', 'signer', 'sign', '--app-version', version, artifact.source], {
                TAURI_SIGNING_PRIVATE_KEY: keyContents,
                TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD || '',
            });
        }
        const signature = await readFile(`${artifact.source}.sig`, 'utf8');
        verifyUpdaterSignature(await readFile(artifact.source), signature, plugins.updater.pubkey);
        artifacts.push({source: `${artifact.source}.sig`, name: `${artifact.name}.sig`});
    }
    const stagingRoot = path.join(root, 'artifacts/release-staging');
    await mkdir(stagingRoot, {recursive: true});
    const stage = await mkdtemp(path.join(stagingRoot, `${name}-`));
    for (const artifact of artifacts) await cp(artifact.source, path.join(stage, artifact.name));
    if (target.host === 'win32') {
        const portable = path.join(stage, 'Maploom');
        await mkdir(portable);
        // Empty user-owned directory: never ship example files into themes,
        // so extracting a new portable release cannot replace a user's theme.
        await mkdir(path.join(portable, 'themes'));
        await cp(binary, path.join(portable, 'Maploom.exe'));
        for (const item of ['locales', 'docs', 'README.md', 'LICENSE']) {
            await cp(path.join(root, item), path.join(portable, item), {recursive: true});
        }
        await mkdir(path.join(portable, 'resources'));
        await cp(defaults, path.join(portable, 'resources/defaults.mloom_fabric'));
        // Paths travel through environment variables; PowerShell never parses them as code.
        run('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
                "$ErrorActionPreference='Stop'; Compress-Archive -Path (Join-Path $env:MAPLOOM_PORTABLE_DIR '*') -DestinationPath $env:MAPLOOM_PORTABLE_ZIP -CompressionLevel Optimal"],
            {MAPLOOM_PORTABLE_DIR: portable, MAPLOOM_PORTABLE_ZIP: path.join(stage, `${prefix}-portable.zip`)});
        await rm(within(stage, portable), {recursive: true});
    }
    const manifest = {version, platform: name, target: target.triple, withDefaults: true, artifacts: []};
    for (const file of (await readdir(stage)).sort()) {
        const bytes = await readFile(path.join(stage, file));
        if (!bytes.length) throw new Error(`Empty artifact: ${file}`);
        manifest.artifacts.push({file, bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex')});
    }
    await writeFile(path.join(stage, 'SHA256SUMS.txt'), manifest.artifacts.map(item => `${item.sha256}  ${item.file}\n`).join(''));
    await writeFile(path.join(stage, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
    // Users may unpack and run a portable app here. Replace only release files,
    // preserving extracted directories and anything else beside the packages.
    await mkdir(output, {recursive: true});
    for (const item of await readdir(output, {withFileTypes: true})) {
        const packageName = item.name.startsWith('Maploom-') && item.name.includes(`-${name}`)
            && ['-setup.exe', '.msi', '-portable.zip', '.dmg', '.app.tar.gz', '.deb', '.rpm', '.AppImage'].some(suffix => item.name.endsWith(suffix) || item.name.endsWith(`${suffix}.sig`));
        if (item.isFile() && (packageName || ['manifest.json', 'SHA256SUMS.txt'].includes(item.name))) {
            await rm(within(output, path.join(output, item.name)));
        }
    }
    await cp(stage, output, {recursive: true});
    await rm(within(stagingRoot, stage), {recursive: true});
    console.log(`Release ready: ${output}`);
}

try {
    const args = process.argv.slice(2);
    if (args.includes('--help')) {
        console.log(`node scripts/build-release.mjs [${Object.keys(targets).join('|')}|host] [--collect-only]\nDefault: current host and architecture. All packages include resources/defaults.mloom_fabric. host builds all architectures for this OS. --collect-only packages bundles built with the same assets. --with-defaults is accepted for compatibility.`);
    } else {
        const requested = args.find(arg => !arg.startsWith('--')) || `${{
            win32: 'windows',
            darwin: 'macos',
            linux: 'linux'
        }[process.platform]}-${process.arch === 'ia32' ? 'x86' : process.arch}`;
        for (const arg of args.filter(arg => arg.startsWith('--'))) {
            if (!['--with-defaults', '--collect-only'].includes(arg)) throw new Error(`Unknown option ${arg}`);
        }
        const version = await syncVersion();
        const selected = requested === 'host' ? Object.keys(targets).filter(name => targets[name].host === process.platform) : [requested];
        if (!selected.length) throw new Error(`Unsupported host: ${process.platform}`);
        for (const name of selected) await build(name, {collectOnly: args.includes('--collect-only')}, version);
    }
} catch (error) {
    console.error(error.message);
    process.exitCode = 1;
}
