"""Record the native immutable URL; the workflow proves its bytes and isolation."""
import json
import os
from pathlib import Path
import re


config = json.loads(Path('cloudflare/wrangler.json').read_text())
versions = re.findall(r'Current Version ID: ([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})(?=\s|$)', os.environ['WRANGLER_OUTPUT'])
assert len(versions) == 1, 'Expected exactly one native version identity'
version = versions[0]
url = 'https://' + version[:8] + '-' + config['name'] + '.misty-step.workers.dev'
proof = {'version': version, 'url': url, 'run': os.environ['GITHUB_RUN_ID'],
         'attempt': os.environ['GITHUB_RUN_ATTEMPT'], 'workflow_revision': os.environ['GITHUB_SHA']}
Path('preview-identity.json').write_text(json.dumps(proof, indent=2) + '\n')
with open(os.environ['GITHUB_ENV'], 'a') as env:
    env.write('REVIEW_URL=' + url + '\n')
with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as summary:
    summary.write('Immutable review: ' + url + '\n\nVersion: ' + version + '\n')
print(json.dumps(proof))
