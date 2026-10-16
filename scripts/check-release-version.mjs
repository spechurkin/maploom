import {appendFile} from 'node:fs/promises';
import {appVersion, syncVersion} from './app-version.mjs';

try {
    const version = await appVersion();
    const dispatch = process.env.GITHUB_EVENT_NAME === 'workflow_dispatch';
    const tag = dispatch ? version : process.env.GITHUB_REF_NAME || process.argv[2];
    if (tag !== version) throw new Error(`Release tag must be exactly ${version} (without v), got ${tag ?? 'no tag'}`);
    if (!dispatch && process.env.GITHUB_REF_TYPE && process.env.GITHUB_REF_TYPE !== 'tag') throw new Error('Releases require a tag push');
    if (process.env.GITHUB_EVENT_NAME && !['push', 'workflow_dispatch'].includes(process.env.GITHUB_EVENT_NAME)) throw new Error('Releases require a push or manual release event');
    await syncVersion();
    if (process.env.GITHUB_OUTPUT) await appendFile(process.env.GITHUB_OUTPUT, `version=${version}\nprerelease=${version.split('+')[0].includes('-')}\n`);
    console.log(`Release tag verified: ${tag}`);
} catch (error) {
    console.error(error.message);
    process.exitCode = 1;
}
