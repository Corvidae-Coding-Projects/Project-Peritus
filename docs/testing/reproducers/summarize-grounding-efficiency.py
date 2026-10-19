#!/usr/bin/env python3
"""Summarize one run's content-free .efficiency.jsonl without summing cumulative snapshots."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('trace', type=Path, help='Run trace or .efficiency.jsonl path')
args = parser.parse_args()
path = args.trace if args.trace.name.endswith('.efficiency.jsonl') else args.trace.with_suffix('.efficiency.jsonl')
rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
inspections = [row for row in rows if row.get('event') == 'inspection']
views = [row for row in rows if row.get('event') == 'context-view']
accounting = [row for row in rows if row.get('event') == 'provider-accounting']
model_views = [row for row in rows if row.get('event') == 'model-view']
responses = {}
for row in accounting:
    if row.get('response_usage') is not None:
        responses[(row.get('invocation'), row['model_requests'])] = row['response_usage']
first = next((row for row in rows if row.get('event') == 'first-useful-action'), None)
print(json.dumps({
    'trace': str(args.trace),
    'rendered_read_bytes': sum(row['read_bytes'] for row in inspections),
    'identical_rendered_read_bytes': sum(row['repeated_bytes'] for row in inspections),
    'receipt_hash_validation_bytes': sum(row.get('file_hash_validation_bytes', 0) for row in rows),
    'reused_observations': sum(row.get('files_revalidated', 0) for row in rows if row.get('event') == 'grounding-reuse'),
    'design_reuses': sum(row.get('event') == 'design-reused' for row in rows),
    'first_useful_action': first,
    'last_provider_snapshot': accounting[-1] if accounting else None,
    'provider_reported_response_usage': list(responses.values()),
    'observed_response_input_tokens': sum(usage['input_tokens'] or 0 for usage in responses.values()),
    'observed_response_cached_input_tokens': sum(usage['cached_input_tokens'] or 0 for usage in responses.values()),
    'responses_with_known_input_usage': sum(usage['input_tokens'] is not None for usage in responses.values()),
    'responses_with_known_cached_input_usage': sum(usage['cached_input_tokens'] is not None for usage in responses.values()),
    'model_requests_observed': len(model_views),
    'system_policy_bytes_range': [min(row['system_policy_bytes'] for row in model_views), max(row['system_policy_bytes'] for row in model_views)] if model_views else None,
    'context_views': len(views),
    'per_view_input_token_savings_range': [min(row['input_tokens_saved'] for row in views), max(row['input_tokens_saved'] for row in views)] if views else None,
    'auxiliary_compaction_calls': sum(row.get('event') == 'local-semantic-compaction' for row in rows),
    'notes': 'Read bytes count rendered text, not all filesystem I/O. The hash byte counter covers receipt revalidation; read pre/post checks and archived-preview validation add I/O; repeated bytes count identical pages. Context savings are per view, not cumulative provider billing. Null response usage means unknown. Per-response updates are replaced by invocation/request identity. Provider totals are cumulative snapshots; compare their last values, never sum snapshots. Attempt timing is monotonic; wall timing includes pauses. No source content is stored in these metrics.'
}, indent=2))
