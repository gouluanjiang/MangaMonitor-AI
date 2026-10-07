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


class CIMaintenanceContracts(unittest.TestCase):
    """Enforce the reviewed pins and exercise actual maintenance routing.

    The pin catalog records the upstream runtime audit. These offline checks
    prevent mutable references and drift; they do not refetch or trust upstream
    releases at test time. New YAML forms must be reviewed before being accepted.
    """

    @classmethod
    def setUpClass(cls):
        directory = ROOT / '.github/workflows'
        cls.workflows = {
            path.name: path.read_text(encoding='utf-8').splitlines()
            for path in sorted(directory.iterdir()) if path.suffix in ('.yml', '.yaml')
        }

    def job(self, workflow: str, name: str) -> list[str]:
        _, jobs = field(self.workflows[workflow], 'jobs', 0)
        return field(jobs, name, 2)[1]

    def test_all_action_references_match_audited_commit_pins(self):
        catalog = json.loads((ROOT / '.github/action-pins.json').read_text(encoding='utf-8'))
        self.assertEqual(catalog['schema_version'], 1)
        self.assertEqual(catalog['runtime'], 'node24')
        actions = catalog['actions']
        self.assertIsInstance(actions, dict)
        self.assertTrue(actions)
        for action, pin in actions.items():
            with self.subTest(action=action):
                self.assertRegex(action, r'^[\w.-]+/[\w.-]+(?:/[\w.-]+)*$')
                self.assertRegex(pin['commit'], r'\A[0-9a-f]{40}\Z')
                self.assertRegex(pin['version'], r'^v\d+\.\d+\.\d+(?:[-+][\w.-]+)?$')

        references = 0
        for workflow, lines in self.workflows.items():
            for number, line in enumerate(lines, 1):
                if line.lstrip().startswith('#') or not re.search(r'''\buses['"]?\s*:''', line):
                    continue
                with self.subTest(workflow=workflow, line=number):
                    # Reject unsupported flow/quoted-key forms instead of
                    # silently skipping a newly introduced Action reference.
                    match = re.fullmatch(r'\s*(?:-\s+)?uses:\s*(\S+)(?:\s+#.*)?\s*', line)
                    self.assertIsNotNone(match, 'unsupported uses field syntax')
                    action, separator, commit = scalar(match.group(1)).rpartition('@')
                    self.assertEqual(separator, '@')
                    self.assertRegex(commit, r'\A[0-9a-f]{40}\Z')
                    self.assertIn(action, actions, 'Action is missing its reviewed catalog entry')
                    self.assertEqual(commit, actions[action]['commit'])
                    references += 1
        self.assertGreater(references, 0)

    def windows_triggered(self, event: str, branch: str, changed: list[str]) -> bool:
        _, events = field(self.workflows['windows-install-verification.yml'], 'on', 0)
        value, config = field(events, event, 2)
        self.assertEqual(value, '')
        self.assertEqual(keys(config, 4), {'branches', 'paths'})
        branches = sequence(*field(config, 'branches', 4), 6)
        paths = sequence(*field(config, 'paths', 4), 6)
        return (any(glob_matches(branch, pattern) for pattern in branches)
                and any(glob_matches(path, pattern) for path in changed for pattern in paths))

    def test_windows_contract_preserves_paths_and_covers_development_pushes(self):
        _, events = field(self.workflows['windows-install-verification.yml'], 'on', 0)
        self.assertEqual(keys(events, 2), {'push', 'pull_request', 'workflow_dispatch'})
        protected_paths = [
            'apps/local-workbench/tools/**',
            'apps/local-workbench/tests/windows-install-verification.tests.ps1',
            '.github/workflows/windows-install-verification.yml',
        ]
        for event, branches in (('push', ['main', 'assistant-*', 'codex/**']),
                                ('pull_request', ['main'])):
            _, config = field(events, event, 2)
            self.assertEqual(sequence(*field(config, 'branches', 4), 6), branches)
            self.assertEqual(sequence(*field(config, 'paths', 4), 6), protected_paths)

        affected = (
            'apps/local-workbench/tools/windows-install-verification.psm1',
            'apps/local-workbench/tools/nested/new-helper.ps1',
            'apps/local-workbench/tests/windows-install-verification.tests.ps1',
            '.github/workflows/windows-install-verification.yml',
        )
        for branch, path in itertools.product(('main', 'assistant-audit', 'codex/fix', 'codex/a/b'), affected):
            with self.subTest(event='push', branch=branch, path=path):
                self.assertTrue(self.windows_triggered('push', branch, [path]))
                self.assertTrue(self.windows_triggered('push', branch, ['README.md', path]))
        for path in affected:
            self.assertTrue(self.windows_triggered('pull_request', 'main', [path]))
            # Pull-request branch filters apply to the base, not the head.
            self.assertFalse(self.windows_triggered('pull_request', 'codex/fix', [path]))
        for branch in ('other', 'codex-other', 'assistant/audit'):
            self.assertFalse(self.windows_triggered('push', branch, list(affected)))
        unrelated = ('README.md', '.github/workflows/ai-ci.yml',
                     'apps/local-workbench/tools-old/helper.ps1',
                     'apps/local-workbench/tests/windows-install-verification.tests.ps1.bak')
        for event in ('push', 'pull_request'):
            self.assertFalse(self.windows_triggered(event, 'main', list(unrelated)))
            self.assertFalse(self.windows_triggered(event, 'main', []))

    def test_ubuntu26_is_bounded_opt_in_with_ubuntu24_defaults(self):
        for workflow, job in (('ai-ci.yml', 'rust-regression'), ('local-workbench.yml', 'frontend')):
            with self.subTest(workflow=workflow):
                _, events = field(self.workflows[workflow], 'on', 0)
                _, dispatch = field(events, 'workflow_dispatch', 2)
                _, inputs = field(dispatch, 'inputs', 4)
                _, runner = field(inputs, 'linux_runner', 6)
                self.assertEqual(scalar(field(runner, 'type', 8)[0]), 'choice')
                self.assertEqual(sequence(*field(runner, 'options', 8), 10),
                                 ['ubuntu-24.04', 'ubuntu-26.04'])
                self.assertEqual(scalar(field(runner, 'default', 8)[0]), 'ubuntu-24.04')
                selection = scalar(field(self.job(workflow, job), 'runs-on', 4)[0])
                for requested, expected in (('', 'ubuntu-24.04'), ('ubuntu-24.04', 'ubuntu-24.04'),
                                             ('ubuntu-26.04', 'ubuntu-26.04'),
                                             ('self-hosted', 'ubuntu-24.04'), ('other', 'ubuntu-24.04')):
                    self.assertEqual(Expression(selection, {'inputs.linux_runner': requested}).parse(), expected)
        for workflow, lines in self.workflows.items():
            for line in lines:
                if re.match(r'^\s*runs-on:', line):
                    with self.subTest(workflow=workflow, runner=line.strip()):
                        self.assertNotIn('ubuntu-latest', line)

    def test_ubuntu_validation_queues_and_cargo_restore_are_isolated(self):
        for workflow in ('ai-ci.yml', 'local-workbench.yml'):
            _, concurrency = field(self.workflows[workflow], 'concurrency', 0)
            template = scalar(field(concurrency, 'group', 2)[0])

            def group(requested: str, ref: str = 'codex/maintenance') -> str:
                context = {'inputs.linux_runner': requested, 'github.head_ref': '', 'github.ref_name': ref}
                return re.sub(r'\$\{\{.*?\}\}', lambda match: str(Expression(match.group(), context).parse()), template)

            with self.subTest(workflow=workflow):
                self.assertEqual(group(''), group('ubuntu-24.04'))
                self.assertNotEqual(group('ubuntu-24.04'), group('ubuntu-26.04'))
                self.assertNotEqual(group('ubuntu-26.04'), group('ubuntu-26.04', 'codex/other'))

        linux = self.job('ai-ci.yml', 'rust-regression')
        _, environment = field(linux, 'env', 4)
        cache_runner = scalar(field(environment, 'CI_LINUX_RUNNER', 6)[0])
        for requested in ('', 'ubuntu-24.04', 'ubuntu-26.04'):
            self.assertEqual(Expression(cache_runner, {'inputs.linux_runner': requested}).parse(),
                             requested or 'ubuntu-24.04')
        _, steps = field(linux, 'steps', 4)
        starts = [i for i, line in enumerate(steps) if line.startswith('      - ')]
        blocks = [steps[start:end] for start, end in zip(starts, starts[1:] + [len(steps)])]
        caches = [block for block in blocks if any(re.match(r'\s*uses:\s*actions/cache@', line) for line in block)]
        self.assertEqual(len(caches), 1)
        _, cache = field(caches[0], 'with', 8)
        marker = '${{ env.CI_LINUX_RUNNER }}'
        self.assertIn(marker, field(cache, 'key', 10)[0])
        style, restores = field(cache, 'restore-keys', 10)
        self.assertEqual(style, '|')
        prefixes = [line.strip() for line in restores if line.strip() and not line.lstrip().startswith('#')]
        self.assertTrue(prefixes)
        for prefix in prefixes:
            self.assertIn(marker, prefix, 'a generic fallback would mix Ubuntu target caches')

    def test_ubuntu26_dispatch_does_not_repeat_the_windows_baseline(self):
        condition = scalar(field(self.job('ai-ci.yml', 'windows-local-executor'), 'if', 4)[0])
        if not condition.startswith('${{'):
            condition = '${{ ' + condition + ' }}'
        for event, requested in itertools.product(('push', 'pull_request', 'workflow_dispatch'),
                                                   ('', 'ubuntu-24.04', 'ubuntu-26.04')):
            with self.subTest(event=event, runner=requested):
                actual = Expression(condition, {'github.event_name': event, 'inputs.linux_runner': requested}).parse()
                self.assertEqual(actual, not (event == 'workflow_dispatch' and requested == 'ubuntu-26.04'))


if __name__ == '__main__':
    unittest.main(verbosity=2)
