"""Exercise deployed public bytes, canonical homepage and an unknown-path refusal."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import sys
import urllib.error
import urllib.parse
import urllib.request

base = sys.argv[1].rstrip('/')
config = json.loads(Path('cloudflare/site.json').read_text())

def request(path, digest=None):
    url = base + '/' + urllib.parse.quote(path, safe='/')
    try:
        response = urllib.request.urlopen(urllib.request.Request(url, headers={
            'Accept-Encoding': 'identity',
            'User-Agent': 'MistyStep-static-site-parity/1.0 (+https://foundations.mistystep.io/)',
        }), timeout=30)
    except urllib.error.HTTPError as exc:
        response = exc
    with response:
        if urllib.parse.urlparse(response.url).netloc != urllib.parse.urlparse(base).netloc:
            raise RuntimeError('cross-host redirect')
        body = response.read(26_000_001)
        if len(body) > 26_000_000:
            raise RuntimeError('oversized response')
        result = {'path': path, 'status': response.status, 'sha256': hashlib.sha256(body).hexdigest()}
        if digest:
            if response.status != 200 or result['sha256'] != digest:
                raise RuntimeError(result)
        else:
            if response.status != 404:
                raise RuntimeError(result)
        return result

def checked(pair):
    path, digest = pair
    try:
        return dict(request(path, digest), passed=True, base=base)
    except Exception as error:
        return {'path': path, 'base': base, 'passed': False,
                'error_type': type(error).__name__, 'error': str(error)[:500]}


files = [line.split('  ', 1) for line in Path('cloudflare/files.sha256').read_text().splitlines()]
checks = [(str(Path(path).relative_to('_site')), digest) for digest, path in files]
index = next(digest for digest, path in files if path == '_site/' + (config['project_prefix'] + '/' if config['project_prefix'] else '') + 'index.html')
checks.append((config['project_prefix'] + '/' if config['project_prefix'] else '', index))
checks.append(('__migration_missing_t_c60b5646__', None))
if config['repo'].endswith('/the-farm-house-demo'):
    checks.append(('the-farm-house-demo/_astro/index.2xZUTRFX.css', None))
results = []
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    for result in pool.map(checked, checks):
        results.append(result)
        Path('cloudflare-http-proof.json').write_text(json.dumps(results, indent=2) + '\n')
if not all(result['passed'] for result in results):
    raise RuntimeError('HTTP parity failed; per-object failure evidence retained')
print(f'PASS {len(results)} real HTTP/hash/refusal checks at {base}')
