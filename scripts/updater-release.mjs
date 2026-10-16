import {createHash, createPublicKey, verify} from 'node:crypto';

// Preserve the installed package type on Windows (MSI and NSIS have different upgrade paths).
export const updaterTargets = {
    'windows-x64': {'windows-x86_64': '-setup.exe', 'windows-x86_64-nsis': '-setup.exe', 'windows-x86_64-msi': '.msi'},
    'windows-x86': {'windows-i686': '-setup.exe', 'windows-i686-nsis': '-setup.exe', 'windows-i686-msi': '.msi'},
    'macos-x64': {'darwin-x86_64': '.app.tar.gz'},
    'macos-arm64': {'darwin-aarch64': '.app.tar.gz'},
    'linux-x64': {'linux-x86_64': '.AppImage', 'linux-x86_64-deb': '.deb', 'linux-x86_64-rpm': '.rpm'},
    'linux-arm64': {'linux-aarch64': '.AppImage', 'linux-aarch64-deb': '.deb', 'linux-aarch64-rpm': '.rpm'},
};

// Tauri wraps Minisign text in base64. Verify both the payload and trusted comment
// before publishing, so a wrong CI signing key cannot ship unusable updates.
export function verifyUpdaterSignature(bytes, signature, pubkey) {
    const keyLines = Buffer.from(pubkey.trim(), 'base64').toString('utf8').trim().split(/\r?\n/);
    const lines = Buffer.from(signature.trim(), 'base64').toString('utf8').trim().split(/\r?\n/);
    const key = Buffer.from(keyLines[1] || '', 'base64');
    const sig = Buffer.from(lines[1] || '', 'base64');
    const global = Buffer.from(lines[3] || '', 'base64');
    if (key.length !== 42 || sig.length !== 74 || global.length !== 64 || !lines[2]?.startsWith('trusted comment: ')
        || !key.subarray(2, 10).equals(sig.subarray(2, 10)) || sig.toString('ascii', 0, 2) !== 'ED') {
        throw new Error('Invalid updater signature or signing key');
    }
    const publicKey = createPublicKey({
        format: 'der', type: 'spki',
        key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), key.subarray(10)])
    });
    const payloadSignature = sig.subarray(10);
    if (!verify(null, createHash('blake2b512').update(bytes).digest(), publicKey, payloadSignature)
        || !verify(null, Buffer.concat([payloadSignature, Buffer.from(lines[2].slice(17))]), publicKey, global)) {
        throw new Error('Updater signature verification failed');
    }
}

export function updaterManifest(version, entries, repository = 'spechurkin/maploom') {
    if (repository !== 'spechurkin/maploom') throw new Error('Unexpected release repository');
    const platforms = {};
    for (const [platform, targets] of Object.entries(updaterTargets)) {
        for (const [target, suffix] of Object.entries(targets)) {
            const file = `Maploom-${version}-${platform}${suffix}`;
            const signature = entries.get(`${file}.sig`);
            if (!signature?.trim()) throw new Error(`Missing updater signature: ${file}`);
            platforms[target] = {
                signature: signature.trim(),
                url: `https://github.com/${repository}/releases/download/${encodeURIComponent(version)}/${encodeURIComponent(file)}`
            };
        }
    }
    return {version, pub_date: new Date().toISOString(), notes: `Maploom ${version}`, platforms};
}
