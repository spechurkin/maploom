// Native package lifecycle checks. Linux installation runs ONLY on an ephemeral
// GitHub runner. macOS operates only in this repository's temporary artifact tree.
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {cp, mkdir, mkdtemp, readdir, readFile, rename, rm, stat, writeFile} from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import {root} from './app-version.mjs';

const input = path.resolve(process.argv[2] ?? 'dist');

async function files(dir) {
    const result = [];
    for (const entry of await readdir(dir, {withFileTypes: true})) {
        const file = path.join(dir, entry.name);
        if (entry.isDirectory()) result.push(...await files(file)); else result.push(file);
    }
    return result;
}

const packages = await files(input);

function run(command, args, allowed = [0]) {
    const result = spawnSync(command, args, {encoding: 'utf8', windowsHide: true, timeout: 180000});
    if (result.error) throw result.error;
    if (!allowed.includes(result.status)) throw Error(`${command} failed (${result.status})\n${result.stdout}\n${result.stderr}`);
    return result.stdout.trim();
}

async function marker(directory) {
    const folder = path.join(directory, 'Моя тема [夜] 100%');
    await mkdir(path.join(folder, 'fonts'), {recursive: true});
    const data = {
        'theme.json': '{"format":"maploom/theme","api_version":1,"name":"Keep me","stylesheet":"style.css"}',
        'style.css': '/* user changes survive */', 'theme.tsx': 'export default api => api.setTokens({});',
        'fonts/user.woff2': Buffer.from([0, 1, 2, 123, 255])
    };
    const hashes = new Map();
    for (const [name, content] of Object.entries(data)) {
        await writeFile(path.join(folder, name), content);
        hashes.set(name, createHash('sha256').update(content).digest('hex'));
    }
    return async () => {
        for (const [name, hash] of hashes) assert.equal(createHash('sha256').update(await readFile(path.join(folder, name))).digest('hex'), hash, `${name} was changed or deleted`);
    };
}

const stageBase = path.join(root, 'artifacts/theme-package-check');
await mkdir(stageBase, {recursive: true});
const stage = await mkdtemp(path.join(stageBase, process.platform + '-'));

function withinStage(target) {
    const relative = path.relative(stage, path.resolve(target));
    if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) throw Error('Unsafe lifecycle target');
    return target;
}

if (process.platform === 'linux') {
    if (process.env.GITHUB_ACTIONS !== 'true' || process.getuid?.() !== 0) throw Error('Linux package installation is allowed only as root on an ephemeral GitHub Actions runner');
    for (const kind of ['deb', 'rpm']) {
        const file = packages.find(p => p.endsWith('.' + kind));
        assert.ok(file, `Missing ${kind}`);
        const name = kind === 'deb' ? run('dpkg-deb', ['-f', file, 'Package']) : run('rpm', ['-qp', '--queryformat', '%{NAME}', file]);
        const listed = kind === 'deb' ? run('dpkg-deb', ['-c', file]) : run('rpm', ['-qlp', file]);
        assert.ok(!listed.includes('/usr/lib/Maploom/themes'), 'User theme paths must not be package-owned');
        const query = spawnSync(kind === 'deb' ? 'dpkg-query' : 'rpm', kind === 'deb' ? ['-W', '-f=${Status}', name] : ['-q', name], {encoding: 'utf8'});
        assert.ok(query.status !== 0 || (kind === 'deb' && !query.stdout.includes('installed')), 'Refusing to replace an installed package');
        const install = () => kind === 'deb' ? run('dpkg', ['--force-depends', '-i', file]) : run('rpm', ['-U', '--replacepkgs', '--nodeps', file]);
        install();
        assert.ok((await stat('/usr/lib/Maploom/themes')).isDirectory());
        const verify = await marker('/usr/lib/Maploom/themes');
        install();
        await verify();
        if (kind === 'deb') {
            run('dpkg', ['--force-depends', '--remove', name]);
            await verify();
            install();
            await verify();
            run('dpkg', ['--force-depends', '--purge', name]);
            await verify();
        } else {
            run('rpm', ['-e', '--nodeps', name]);
            await verify();
        }
        console.log(`${kind}: native install, same-version reinstall, remove${kind === 'deb' ? ', purge' : ''}; user SHA-256 hashes preserved`);
    }
    const image = packages.find(p => p.endsWith('.AppImage'));
    assert.ok(image, 'Missing AppImage');
    const target = withinStage(path.join(stage, 'Maploom.AppImage'));
    await cp(image, target);
    const verify = await marker(path.join(stage, 'themes'));
    // AppImage updater replaces the executable, never its containing directory.
    await rename(target, withinStage(path.join(stage, 'previous.AppImage')));
    await cp(image, target);
    await verify();
    await rm(target);
    await verify();
    console.log('AppImage: executable replacement/removal preserves sibling themes (filesystem lifecycle; no GUI updater execution)');
} else if (process.platform === 'darwin') {
    const dmg = packages.find(p => p.endsWith('.dmg'));
    assert.ok(dmg, 'Missing DMG');
    const mount = withinStage(path.join(stage, 'mounted'));
    await mkdir(mount);
    run('hdiutil', ['attach', '-readonly', '-nobrowse', '-mountpoint', mount, dmg]);
    try {
        const app = (await readdir(mount)).find(p => p.endsWith('.app'));
        assert.ok(app, 'Missing app in DMG');
        const target = withinStage(path.join(stage, app));
        run('ditto', [path.join(mount, app), target]);
        const verify = await marker(path.join(stage, 'themes'));
        run('ditto', [path.join(mount, app), target]);
        await verify();
        await rename(target, withinStage(path.join(stage, 'previous.app')));
        run('ditto', [path.join(mount, app), target]);
        await verify();
        run('codesign', ['--verify', '--deep', '--strict', target]);
        await rm(withinStage(target), {recursive: true});
        await verify();
        console.log('macOS: real DMG mount/copy, app replacement/removal, signature and theme SHA-256 hashes passed (no GUI updater execution)');
    } finally {
        run('hdiutil', ['detach', mount]);
    }
} else {
    throw Error('Use the Windows NSIS/MSI lifecycle script on Windows');
}
