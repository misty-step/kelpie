"""Exercise deployed public bytes, canonical homepage and an unknown-path refusal."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

base = sys.argv[1].rstrip('/')
config = json.loads(Path('cloudflare/site.json').read_text())
readiness_deadline = None

def request(path, digest=None):
    url = base + '/' + urllib.parse.quote(path, safe='/')
    try:
        response = urllib.request.urlopen(urllib.request.Request(url, headers={
            'Accept-Encoding': 'identity',
            'Cache-Control': 'no-cache',
            'User-Agent': 'MistyStep-static-site-parity/1.0 (+https://foundations.mistystep.io/)',
        }), timeout=min(30, max(0.01, readiness_deadline - time.monotonic())) if readiness_deadline is not None else 30)
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
        if readiness_deadline is not None and time.monotonic() >= readiness_deadline:
            raise TimeoutError('readiness budget exhausted before request')
        return dict(request(path, digest), passed=True, base=base)
    except Exception as error:
        return {'path': path, 'base': base, 'passed': False,
                'error_type': type(error).__name__, 'error': str(error)[:500]}


files = [line.split('  ', 1) for line in Path('cloudflare/files.sha256').read_text().splitlines()]
checks = [(str(Path(path).relative_to('_site')), digest) for digest, path in files]
index = next(digest for digest, path in files if path == '_site/' + (config['project_prefix'] + '/' if config['project_prefix'] else '') + 'index.html')
homepage = config['project_prefix'] + '/' if config['project_prefix'] else ''
checks.append((homepage, index))
checks.append(('__migration_missing_t_c60b5646__', None))
if config['repo'].endswith('/the-farm-house-demo'):
    checks.append(('the-farm-house-demo/_astro/index.2xZUTRFX.css', None))


def sweep():
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for result in pool.map(checked, checks):
            results.append(result)
            Path('cloudflare-http-proof.json').write_text(json.dumps(results, indent=2) + '\n')
    return results


if '--wait' in sys.argv[2:]:
    # Edge routing and individual assets converge separately. A ready homepage
    # alone is not evidence that the full deployed artifact is ready.
    readiness_deadline = time.monotonic() + 120
    observations = []
    consecutive = 0
    stable_since = None
    while time.monotonic() < readiness_deadline:
        results = sweep()
        observations.append({'elapsed_seconds': round(time.monotonic() - (readiness_deadline - 120), 3), 'checks': results})
        Path('cloudflare-readiness-proof.json').write_text(json.dumps(observations, indent=2) + '\n')
        if all(result['passed'] for result in results):
            consecutive += 1
            stable_since = time.monotonic() if stable_since is None else stable_since
        else:
            consecutive = 0
            stable_since = None
        # Actual fresh-Worker probes regressed to per-object404 after two
        # passing sweeps. Require a measured all-object quiet window, not a
        # blind delay or a discarded failed observation.
        if consecutive >= 2 and time.monotonic() - stable_since >= 30:
            break
        time.sleep(5)
    else:
        raise RuntimeError('edge readiness deadline exceeded; observations retained')
    readiness_deadline = None
results = sweep()
if not all(result['passed'] for result in results):
    raise RuntimeError('HTTP parity failed; per-object failure evidence retained')
print(f'PASS {len(results)} real HTTP/hash/refusal checks at {base}')
