"""Prove isolated static A→B→native A with recorded bounded edge readiness."""
import hashlib
import json
from pathlib import Path
import sys
import time
import urllib.error
import urllib.request

NAME = '__migration_rollback_probe_t_c60b5646.txt'
BODY = b'synthetic static-preview rollback probe t_c60b5646\n'
mode = sys.argv[1]
marker = Path('_site') / NAME
if mode == 'stage':
    if marker.exists() or not marker.parent.is_dir():
        raise RuntimeError('unexpected staging state')
    marker.write_bytes(BODY)
else:
    if mode not in ('absent', 'visible', 'restored'):
        raise ValueError('unknown mode')
    config = json.loads(Path('cloudflare/wrangler.json').read_text())
    url = 'https://' + config['name'] + '.misty-step.workers.dev/' + NAME
    observations = []; consecutive = 0
    # Workers upload completion is not global-edge readiness. Retain every
    # sample (including initial failures), require2 consecutive matching reads,
    # and fail loudly at90s. This is a readiness window, not an uptime claim.
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        sample = {'elapsed_seconds': round(90 - (deadline-time.monotonic()), 3)}
        try:
            try:
                response = urllib.request.urlopen(urllib.request.Request(url, headers={
                    'User-Agent': 'MistyStep-static-site-parity/1.0 (+https://foundations.mistystep.io/)',
                    'Cache-Control': 'no-cache',
                }), timeout=15)
            except urllib.error.HTTPError as exc:
                response = exc
            with response:
                body = response.read(1024)
                sample.update(status=response.status, sha256=hashlib.sha256(body).hexdigest())
                okay = (response.status == 200 and body == BODY) if mode == 'visible' else response.status == 404
        except (OSError, urllib.error.URLError) as exc:
            sample['error_type'] = type(exc).__name__; okay = False
        observations.append(sample)
        print(mode, json.dumps(sample), flush=True)
        Path('rollback-' + mode + '.json').write_text(json.dumps(observations, indent=2) + '\n')
        consecutive = consecutive+1 if okay else 0
        if consecutive == 2:
            break
        time.sleep(5)
    else:
        raise RuntimeError('edge readiness failed after90s; observations retained')
    if mode == 'restored':
        if marker.read_bytes() != BODY:
            raise RuntimeError('local promotion artifact marker changed unexpectedly')
        marker.unlink()
