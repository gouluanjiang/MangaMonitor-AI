#!/usr/bin/env python3
"""Check the actual workflow headers without PyYAML or GitHub/source access.

Only the YAML mapping/list forms and GitHub expression operators used by these
headers are supported. Unsupported syntax fails rather than silently guessing.
This checks routing contracts, not GitHub's hosted runner/queue implementation.
"""

from __future__ import annotations

import itertools
import json
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]


def field(lines: list[str], key: str, indent: int) -> tuple[str, list[str]]:
    pattern = re.compile(rf"^{' ' * indent}{re.escape(key)}:\s*(.*?)\s*$")
    found = [(i, pattern.fullmatch(line)) for i, line in enumerate(lines)]
    found = [(i, match) for i, match in found if match]
    if len(found) != 1:
        raise ValueError(f"expected exactly one field {key!r} at indent {indent}")
    start, match = found[0]
    end = start + 1
    while end < len(lines):
        line = lines[end]
        if line.strip() and not line.lstrip().startswith('#'):
            if len(line) - len(line.lstrip()) <= indent:
                break
        end += 1
    return match.group(1), lines[start + 1:end]


def scalar(value: str) -> str:
    if value[:1] in ("'", '"'):
        if len(value) < 2 or value[-1] != value[0]:
            raise ValueError(f"unsupported YAML scalar: {value!r}")
        return value[1:-1]
    return value


def sequence(value: str, lines: list[str], indent: int) -> list[str]:
    if value:
        if not (value.startswith('[') and value.endswith(']')):
            raise ValueError(f"unsupported YAML sequence: {value!r}")
        return [scalar(item.strip()) for item in value[1:-1].split(',')]
    result = []
    for line in lines:
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        prefix = ' ' * indent + '- '
        if not line.startswith(prefix):
            raise ValueError(f"unsupported YAML sequence line: {line!r}")
        result.append(scalar(line[len(prefix):].strip()))
    return result


def keys(lines: list[str], indent: int) -> set[str]:
    pattern = re.compile(rf"^{' ' * indent}([a-z_-]+):")
    return {match.group(1) for line in lines if (match := pattern.match(line))}


def glob_matches(value: str, pattern: str) -> bool:
    # GitHub branch/path globs: '*' does not cross '/', '**' does.
    parts = re.split(r'(\*\*|\*|\?)', pattern)
    expression = ''.join({ '**': '.*', '*': '[^/]*', '?': '[^/]' }.get(part, re.escape(part))
                         for part in parts)
    return re.fullmatch(expression, value) is not None


class Expression:
    """A fail-closed interpreter for this header's documented expression subset."""

    TOKEN = re.compile(r"\s*(==|!=|&&|\|\||[(),]|'(?:[^']|'')*'|[a-zA-Z_][a-zA-Z0-9_.]*)")

    def __init__(self, raw: str, context: dict[str, str]):
        if not (raw.startswith('${{') and raw.endswith('}}')):
            raise ValueError('concurrency.group must be one explicit GitHub expression')
        source = raw[3:-2].strip()
        self.tokens = []
        while source:
            match = self.TOKEN.match(source)
            if not match:
                raise ValueError(f'unsupported expression syntax: {source!r}')
            self.tokens.append(match.group(1))
            source = source[match.end():]
        self.index = 0
        self.context = context

    def take(self, token: str) -> bool:
        if self.index < len(self.tokens) and self.tokens[self.index] == token:
            self.index += 1
            return True
        return False

    def parse(self):
        value = self.parse_or()
        if self.index != len(self.tokens):
            raise ValueError('unexpected trailing expression tokens')
        return value

    def parse_or(self):
        value = self.parse_and()
        while self.take('||'):
            other = self.parse_and()
            value = value or other
        return value

    def parse_and(self):
        value = self.parse_equal()
        while self.take('&&'):
            other = self.parse_equal()
            value = other if value else value
        return value

    def parse_equal(self):
        value = self.atom()
        if self.take('=='):
            other = self.atom()
            return str(value).lower() == str(other).lower()
        if self.take('!='):
            other = self.atom()
            return str(value).lower() != str(other).lower()
        return value

    def atom(self):
        if self.take('('):
            value = self.parse_or()
            if not self.take(')'):
                raise ValueError('unclosed expression parentheses')
            return value
        if self.index >= len(self.tokens):
            raise ValueError('missing expression value')
        token = self.tokens[self.index]
        self.index += 1
        if token.startswith("'"):
            return token[1:-1].replace("''", "'")
        if token == 'format' and self.take('('):
            template = self.parse_or()
            if not self.take(','):
                raise ValueError('format requires a template and one argument')
            argument = self.parse_or()
            if not self.take(')') or template.count('{0}') != 1:
                raise ValueError('only single-argument format templates are supported')
            return template.replace('{0}', str(argument))
        if token not in self.context:
            raise ValueError(f'unknown expression context: {token}')
        return self.context[token]


class WorkflowContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.monitor = (ROOT / '.github/workflows/phase3b.yml').read_text(encoding='utf-8').splitlines()
        cls.ci = (ROOT / '.github/workflows/ai-ci.yml').read_text(encoding='utf-8').splitlines()
        _, concurrency = field(cls.monitor, 'concurrency', 0)
        cls.group = scalar(field(concurrency, 'group', 2)[0])
        cls.cancel = field(concurrency, 'cancel-in-progress', 2)[0]
        _, cls.triggers = field(cls.ci, 'on', 0)

    def group_for(self, event: str, scope: str, ref: str, mode: str) -> str:
        return Expression(self.group, {
            'github.event_name': event, 'inputs.scope': scope, 'github.ref_name': ref,
            'github.ref': f'refs/heads/{ref}', 'inputs.mode': mode,
        }).parse()

    def triggered(self, event: str, branch: str, changed: list[str]) -> bool:
        value, config = field(self.triggers, event, 2)
        self.assertEqual(value, '')
        self.assertLessEqual(keys(config, 4), {'branches', 'paths-ignore'})
        branches = sequence(*field(config, 'branches', 4), 6)
        if not any(glob_matches(branch, pattern) for pattern in branches):
            return False
        if 'paths-ignore' not in keys(config, 4):
            return True
        ignored = sequence(*field(config, 'paths-ignore', 4), 6)
        return any(not any(glob_matches(path, pattern) for pattern in ignored) for path in changed)

    def test_production_events_share_one_non_cancelling_group(self):
        self.assertEqual(self.cancel, 'false')
        _, events = field(self.monitor, 'on', 0)
        self.assertTrue({'schedule', 'workflow_dispatch'} <= keys(events, 2))
        for ref, mode in itertools.product(('main', 'codex/recovery', 'assistant-fix'),
                                           ('monthly', 'incremental', 'full')):
            for event, scope in (('schedule', ''), ('workflow_dispatch', 'production')):
                with self.subTest(event=event, scope=scope, ref=ref, mode=mode):
                    self.assertEqual(self.group_for(event, scope, ref, mode), 'phase3b-production')

    def test_validation_and_bootstrap_do_not_share_production_queue(self):
        _, events = field(self.monitor, 'on', 0)
        _, dispatch = field(events, 'workflow_dispatch', 2)
        _, inputs = field(dispatch, 'inputs', 4)
        _, scope = field(inputs, 'scope', 6)
        self.assertEqual(set(sequence(*field(scope, 'options', 8), 10)),
                         {'validation', 'bootstrap', 'production'})
        default = scalar(field(scope, 'default', 8)[0])
        self.assertEqual(default, 'validation')
        for ref in ('main', 'codex/check'):
            for requested in ('validation', 'bootstrap', ''):
                with self.subTest(ref=ref, scope=requested):
                    actual = self.group_for('workflow_dispatch', requested, ref, 'full')
                    self.assertEqual(actual, 'phase3b-' + (requested or default))

    def test_safety_changes_trigger_push_for_all_development_branches(self):
        paths = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates/new/src/lib.rs',
                 'fixtures/new/case.json', 'monitor-config.json', 'local-materialization-policy.json',
                 'scripts/new-safety-check.py', 'scripts/test-workflow-contracts.py',
                 '.github/workflows/new-validation.yml', 'AGENTS.md', 'docs/NEW_GATE.md',
                 'future-safety-policy.json')
        for branch, path in itertools.product(('main', 'assistant-audit', 'codex/fix', 'codex/a/b'), paths):
            with self.subTest(branch=branch, path=path):
                self.assertTrue(self.triggered('push', branch, [path]))
                self.assertTrue(self.triggered('push', branch, ['monitor-state/latest.json', path]))

    def test_state_only_pushes_skip_but_pull_requests_always_report(self):
        state_paths = ['monitor-state/pending.json', 'monitor-state/catalog.json', 'evidence/run/report.json']
        for branch in ('main', 'assistant-audit', 'codex/fix'):
            self.assertFalse(self.triggered('push', branch, state_paths))
        for path in (*state_paths, 'README.md', 'local-materialization-policy.json'):
            self.assertTrue(self.triggered('pull_request', 'main', [path]))
        for branch in ('other', 'codex-other', 'assistant/audit'):
            self.assertFalse(self.triggered('push', branch, ['crates/new.rs']))
        self.assertFalse(self.triggered('pull_request', 'other', ['crates/new.rs']))
        self.assertIn('workflow_dispatch', keys(self.triggers, 2))

    def test_production_and_materialization_remain_disabled(self):
        for name, flag in (('monitor-config.json', 'production_enabled'),
                           ('local-materialization-policy.json', 'materialization_enabled')):
            with self.subTest(config=name):
                self.assertIs(json.loads((ROOT / name).read_text(encoding='utf-8'))[flag], False)


if __name__ == '__main__':
    unittest.main(verbosity=2)
