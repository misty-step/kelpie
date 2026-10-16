"""Bound only this workflow's previews before allocating another Worker.

The accepted migration review is pinned; ordinary reviews expire after two newer
completed previews. No DNS, Access, canonical deployment or credential mutation.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request


def plan(names, runs, prefix, pinned, current_id, current_attempt, workflow_id):
    receipts = {r['id']: r for r in runs}
    pattern = re.compile(re.escape(prefix) + r'-([1-9][0-9]*)-([1-9][0-9]*)')
    current = f'{prefix}-{current_id}-{current_attempt}'
    completed = []
    for name in names:
        match = pattern.fullmatch(name)
        if not match:
            continue
        run_id, attempt = map(int, match.groups())
        run = receipts.get(run_id)
        if not run or run['workflow_id'] != workflow_id or run['path'] != '.github/workflows/cloudflare-site.yml' or attempt > run['run_attempt']:
            raise RuntimeError('Unattributed preview: ' + name)
        if name == current:
            continue
        if run['status'] != 'completed' and not (run_id == current_id and attempt < current_attempt):
            raise RuntimeError('Another preview is still running: ' + name)
        completed.append((run_id, attempt, name))
    newest = {row[2] for row in sorted(completed)[-2:]}
    keep = newest | {pinned, current}
    return keep, sorted(row[2] for row in completed if row[2] not in keep)


def main():
    if '--apply' in sys.argv and os.environ.get('GITHUB_ACTIONS') != 'true':
        raise RuntimeError('Deletion is CI-only; local inspection is read-only')
    config = json.loads(Path('cloudflare/wrangler.json').read_text())
    repo = os.environ['GITHUB_REPOSITORY']
    prefix = repo.split('/')[1] + '-site-preview'
    if config['name'] != prefix:
        raise RuntimeError('Cleanup must run before run-name allocation')
    pinned = os.environ['RETAINED_REVIEW_PREVIEW']
    if not re.fullmatch(re.escape(prefix) + r'-[1-9][0-9]*-[1-9][0-9]*', pinned):
        raise RuntimeError('Invalid retained review identity')
    token = os.environ['CLOUDFLARE_API_TOKEN']
    api = 'https://api.cloudflare.com/client/v4/accounts/' + config['account_id'] + '/workers/scripts'

    def cf(suffix='', method='GET'):
        request = urllib.request.Request(api + suffix, method=method, headers={'Authorization': 'Bearer ' + token})
        try:
            response = urllib.request.urlopen(request, timeout=30)
        except urllib.error.HTTPError as exc:
            response = exc
        with response:
            body = json.load(response)
            if response.status == 404 and any(e.get('code') == 10007 for e in body.get('errors', [])):
                return None
            if response.status != 200 or not body.get('success'):
                raise RuntimeError(f'Cloudflare {method} failed: HTTP {response.status}')
            return body['result']

    command = ['gh', 'api', '--paginate', '--slurp', f'repos/{repo}/actions/workflows/cloudflare-site.yml/runs?per_page=100']
    pages = json.loads(subprocess.check_output(command, text=True))
    runs = [r for page in pages for r in page['workflow_runs']]
    if len(runs) != pages[0]['total_count'] or len({r['id'] for r in runs}) != len(runs):
        raise RuntimeError('Incomplete workflow run inventory')
    current_id, current_attempt = int(os.environ['GITHUB_RUN_ID']), int(os.environ['GITHUB_RUN_ATTEMPT'])
    current = next(r for r in runs if r['id'] == current_id)
    names = {row['id'] for row in cf()}
    keep, delete = plan(names, runs, prefix, pinned, current_id, current_attempt, current['workflow_id'])
    # This one unsuffixed migration predecessor was attributed to real Actions
    # provider output before publication. Never infer ownership from prefix alone.
    if prefix in names:
        legacy = next(r for r in runs if r['id'] == int(os.environ['LEGACY_PREVIEW_RUN']))
        if legacy['workflow_id'] != current['workflow_id'] or legacy['status'] != 'completed':
            raise RuntimeError('Unattributed legacy preview')
        delete.append(prefix)
    protected = [repo.split('/')[1] + '-site', pinned]
    before = {name: cf('/' + name + '/deployments') for name in protected}
    if not all(before.values()):
        raise RuntimeError('Missing canonical or accepted review identity')
    proof = {'repo': repo, 'keep': sorted(keep), 'candidates': delete, 'deleted': [], 'apply': '--apply' in sys.argv}
    path = Path(os.environ.get('RUNNER_TEMP', '.')) / 'preview-retention-proof.json'
    path.write_text(json.dumps(proof, indent=2) + '\n')
    print(json.dumps(proof), flush=True)
    for name in delete if proof['apply'] else []:
        cf('/' + name, 'DELETE')
        if cf('/' + name + '/deployments') is not None:
            raise RuntimeError('Provider deletion readback failed: ' + name)
        deadline = time.monotonic() + 90
        while True:
            url = 'https://' + name + '.misty-step.workers.dev/?deleted=' + str(time.time_ns())
            try:
                response = urllib.request.urlopen(urllib.request.Request(url, headers={'User-Agent': 'MistyStep-static-site-parity/1.0 (+https://foundations.mistystep.io/)', 'Cache-Control': 'no-cache'}), timeout=15)
            except urllib.error.HTTPError as exc:
                response = exc
            with response:
                status = response.status
                body = response.read(16384)
            proof.setdefault('http_observations', {}).setdefault(name, []).append({'status': status, 'missing_worker_1042': b'1042' in body})
            path.write_text(json.dumps(proof, indent=2) + '\n')
            if status == 404 and b'1042' in body:
                break
            if time.monotonic() >= deadline:
                raise RuntimeError('HTTP deletion readback failed: ' + name)
            time.sleep(5)
        proof['deleted'].append({'name': name, 'provider': 404, 'http': status})
        path.write_text(json.dumps(proof, indent=2) + '\n')
    if {name: cf('/' + name + '/deployments') for name in protected} != before:
        raise RuntimeError('Canonical or retained review deployment changed')
    proof['protected_deployments'] = before
    proof['protected_unchanged'] = True
    proof['remaining'] = sorted(row['id'] for row in cf() if row['id'] == prefix or re.fullmatch(re.escape(prefix) + r'-[1-9][0-9]*-[1-9][0-9]*', row['id']))
    if proof['apply'] and not set(proof['remaining']) <= keep:
        raise RuntimeError('Preview bound readback failed')
    path.write_text(json.dumps(proof, indent=2) + '\n')
    print('Preview bound verified; canonical and retained review unchanged', flush=True)


if __name__ == '__main__':
    main()
