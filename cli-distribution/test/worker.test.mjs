import assert from 'node:assert/strict';
import test from 'node:test';
import { fetch as handleRequest, keyForPath } from '../src/index.js';

function fixtureEnv(objects = {}) {
  return {
    CLI_RELEASES: {
      async get(key) {
        const value = objects[key];
        return value ? { body: new Blob([value.body]).stream(), httpMetadata: { contentType: value.contentType } } : null;
      },
    },
  };
}

const objects = {
  'install': { body: '#!/bin/sh\necho install\n', contentType: 'text/plain; charset=utf-8' },
  'install.ps1': { body: 'Write-Output install\n', contentType: 'text/plain; charset=utf-8' },
  'latest': { body: 'v0.1.0\n', contentType: 'text/plain; charset=utf-8' },
  'releases/v0.1.0/manifest.json': { body: '{"version":"v0.1.0"}\n', contentType: 'application/json' },
  'releases/v0.1.0/manifest.json.sig': { body: 'signature', contentType: 'application/octet-stream' },
  'releases/v0.1.0/mpaka-x86_64-unknown-linux-musl.tar.gz': { body: 'archive', contentType: 'application/gzip' },
};

test('serves allowed CLI distribution paths from R2', async () => {
  for (const path of ['/install', '/install.ps1', '/latest', '/releases/v0.1.0/manifest.json', '/releases/v0.1.0/manifest.json.sig', '/releases/v0.1.0/mpaka-x86_64-unknown-linux-musl.tar.gz']) {
    const response = await handleRequest(new Request(`https://cli.mpaka.dev${path}`), fixtureEnv(objects));
    assert.equal(response.status, 200, path);
  }
});

test('maps only approved public paths to R2 keys', () => {
  assert.equal(keyForPath('/latest'), 'latest');
  assert.equal(keyForPath('/releases/v0.1.0/manifest.json'), 'releases/v0.1.0/manifest.json');
  assert.equal(keyForPath('/releases/v0.1.0/unknown'), null);
  assert.equal(keyForPath('/../secrets'), null);
});

test('uses short cache for latest and immutable cache for releases', async () => {
  const latest = await handleRequest(new Request('https://cli.mpaka.dev/latest'), fixtureEnv(objects));
  const release = await handleRequest(new Request('https://cli.mpaka.dev/releases/v0.1.0/manifest.json'), fixtureEnv(objects));
  assert.match(latest.headers.get('cache-control'), /no-cache/);
  assert.match(release.headers.get('cache-control'), /immutable/);
});

test('rejects traversal, unknown paths, and non-GET methods', async () => {
  const env = fixtureEnv(objects);
  assert.equal((await handleRequest(new Request('https://cli.mpaka.dev/releases/v0.1.0/../../secret'), env)).status, 404);
  assert.equal((await handleRequest(new Request('https://cli.mpaka.dev/unknown'), env)).status, 404);
  assert.equal((await handleRequest(new Request('https://cli.mpaka.dev/latest', { method: 'POST' }), env)).status, 405);
});

test('returns 404 for an allowed but missing R2 object', async () => {
  const response = await handleRequest(new Request('https://cli.mpaka.dev/releases/v0.1.0/manifest.json'), fixtureEnv());
  assert.equal(response.status, 404);
});
