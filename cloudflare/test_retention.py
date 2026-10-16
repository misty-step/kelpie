"""Exercise the preview bound and rejection cases; live CI remains required."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('prune', Path(__file__).with_name('prune_previews.py'))
prune = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prune)

class RetentionTests(unittest.TestCase):
    def run_receipt(self, run_id, status='completed', attempt=1, workflow=9):
        return {'id': run_id, 'run_attempt': attempt, 'status': status, 'workflow_id': workflow,
                'path': '.github/workflows/cloudflare-site.yml'}

    def test_bound_preserves_current_review_and_newest_two(self):
        runs = [self.run_receipt(i) for i in range(1, 8)] + [self.run_receipt(8, 'in_progress')]
        names = ['demo-site-preview-' + str(i) + '-1' for i in range(1, 9)]
        names += ['demo-site', 'someone-else', 'demo-site-preview-123-not-an-attempt']
        keep, delete = prune.plan(names, runs, 'demo-site-preview', 'demo-site-preview-1-1', 8, 1, 9)
        self.assertEqual(keep, {'demo-site-preview-1-1', 'demo-site-preview-6-1', 'demo-site-preview-7-1', 'demo-site-preview-8-1'})
        self.assertEqual(delete, ['demo-site-preview-' + str(i) + '-1' for i in range(2, 6)])

    def test_unknown_receipt_is_rejected_before_any_delete(self):
        with self.assertRaises(RuntimeError):
            prune.plan(['demo-site-preview-99-1'], [self.run_receipt(1)], 'demo-site-preview', 'demo-site-preview-1-1', 2, 1, 9)

    def test_future_attempt_is_rejected(self):
        with self.assertRaises(RuntimeError):
            prune.plan(['demo-site-preview-1-2'], [self.run_receipt(1)], 'demo-site-preview', 'demo-site-preview-1-1', 2, 1, 9)

    def test_other_workflow_is_rejected(self):
        with self.assertRaises(RuntimeError):
            prune.plan(['demo-site-preview-1-1'], [self.run_receipt(1, workflow=88)], 'demo-site-preview', 'demo-site-preview-1-1', 2, 1, 9)

    def test_noncurrent_running_preview_stops_allocation(self):
        with self.assertRaises(RuntimeError):
            prune.plan(['demo-site-preview-1-1'], [self.run_receipt(1, 'in_progress')], 'demo-site-preview', 'demo-site-preview-2-1', 2, 1, 9)

    def test_recovery_after_partial_deletion_is_idempotent(self):
        runs = [self.run_receipt(i) for i in range(1, 8)]
        names = ['demo-site-preview-' + str(i) + '-1' for i in range(1, 8)]
        args = (runs, 'demo-site-preview', 'demo-site-preview-1-1', 8, 1, 9)
        keep, delete = prune.plan(names, *args)
        after_partial = [n for n in names if n != delete[0]]
        keep2, delete2 = prune.plan(after_partial, *args)
        self.assertEqual(keep2, keep)
        self.assertEqual(delete2, delete[1:])

    def test_rerun_attempt_does_not_replace_pinned_identity(self):
        runs = [self.run_receipt(1, attempt=5)]
        names = ['demo-site-preview-1-' + str(i) for i in range(1, 6)]
        keep, delete = prune.plan(names, runs, 'demo-site-preview', 'demo-site-preview-1-1', 1, 5, 9)
        self.assertIn('demo-site-preview-1-1', keep)
        self.assertIn('demo-site-preview-1-5', keep)
        self.assertEqual(delete, ['demo-site-preview-1-2'])

if __name__ == '__main__':
    unittest.main()
