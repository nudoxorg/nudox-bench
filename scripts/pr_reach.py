#!/usr/bin/env python3
"""Approximate how far each counted PR reaches: existing declarations that use what it changed.

Reads the results.json that scripts/pr_symbols.py wrote, adds reach.json beside
it, and rewrites report.md and summary.json with a reach column.

For each PR, take the symbols it modified or removed (not added, not tests, not
modules, impl blocks or namespaces). Keep only names that are distinctive
(snake_case with an underscore and 8+ characters, or CamelCase with 2+ capitals
and 6+ characters) and declared exactly once in --tree. Every other declaration
in --tree whose body mentions such a name as a whole word counts as reached;
declarations the PR itself changed do not.

This is a name-based stand-in for Nudox's references, run on one snapshot
(--tree), not on each PR's own base. Common names (new, parse, run) cannot be
measured this way, so every figure is a floor. Needs ripgrep (rg) on PATH.

Example:
  python3 scripts/pr_reach.py --results results/2026-11-18-pr-symbols --tree /path/to/Backend
"""
import argparse, collections, json, os, re, subprocess, sys, tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pr_symbols as ps  # noqa: E402

EXCLUDE = ['!vendor/**', '!third_party/**', '!target/**', '!.git/**']
CODE_GLOBS = ['*.rs', '*.py', '*.cs', '*.ts', '*.tsx']
SKIP_KINDS = {'module', 'mod', 'impl', 'namespace'}


def simple(key):
    last = re.sub(r'#\d+$', '', key.split('::')[-1])
    return last.split(' ', 1)[-1]


def distinctive(n):
    if '_' in n and len(n) >= 8 and re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', n):
        return True
    return len(n) >= 6 and sum(c.isupper() for c in n) >= 2 and re.fullmatch(r'[A-Za-z][A-Za-z0-9]*', n) is not None


def rg(tree, args):
    # An explicit path and a closed stdin: with neither, rg searches stdin whenever it is a pipe and waits forever.
    cmd = ['rg', '--no-config', '--hidden'] + sum([['-g', g] for g in EXCLUDE + CODE_GLOBS], []) + args + ['.']
    return subprocess.run(cmd, cwd=tree, capture_output=True, stdin=subprocess.DEVNULL).stdout.decode('utf8', errors='replace')


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--results', required=True, help='directory holding pr_symbols.py output (results.json)')
    ap.add_argument('--tree', required=True, help='a checkout of the same repo to search for dependents')
    a = ap.parse_args()

    results = json.load(open(f'{a.results}/results.json'))
    targets = collections.defaultdict(set)  # name -> PRs
    eligible = collections.Counter()
    for r in results:
        for s in r['symbol_list']:
            if s['status'] in ('modified', 'removed') and not s['test'] and s['kind'] not in SKIP_KINDS:
                eligible[r['pr']] += 1
                n = simple(s['key'])
                if distinctive(n):
                    targets[n].add(r['pr'])
    names = sorted(targets)

    unique, hits = set(), collections.defaultdict(list)
    if names:
        # 1. how many declarations carry each name
        decl_re = (r'\b(?:fn|struct|enum|trait|type|const|static|union|mod|macro_rules!|class|def|interface)\s+('
                   + '|'.join(map(re.escape, names)) + r')\b')
        out = rg(a.tree, ['-o', '-N', '--no-filename', '-r', '$1', '-e', decl_re])
        decl_count = collections.Counter(m for m in out.split('\n') if m)
        unique = {n for n in names if decl_count[n] == 1}
        # 2. whole-word mentions of the uniquely declared names
        with tempfile.NamedTemporaryFile('w', suffix='.txt', delete=False) as f:
            f.write('\n'.join(sorted(unique)) + '\n')
            pattern_file = f.name
        try:
            for line in rg(a.tree, ['--json', '-w', '-F', '-f', pattern_file]).split('\n'):
                if not line:
                    continue
                ev = json.loads(line)
                if ev.get('type') != 'match':
                    continue
                d = ev['data']
                path = d['path'].get('text')
                path = path[2:] if path.startswith('./') else path  # same form as results.json paths
                for sm in d['submatches']:
                    hits[path].append((d['line_number'] - 1, sm['match']['text'], d['lines'].get('text', '')))
        finally:
            os.unlink(pattern_file)

    # 3. map each mention to the innermost declaration holding it
    dependents = collections.defaultdict(set)  # name -> {(file, key, test)}
    for path, items in hits.items():
        ext = path.rsplit('.', 1)[-1]
        if ext not in ps.LANGS:
            continue
        lang = ps.LANGS[ext][0]
        decls = ps.declarations(lang, ext, open(os.path.join(a.tree, path), 'rb').read(), path)
        for row, name, text in items:
            if ps.COMMENT_RE[lang].match(text):
                continue
            best = None
            for d in decls:
                if d['start'] <= row <= d['end'] and (best is None or d['end'] - d['start'] < best['end'] - best['start']):
                    best = d
            key = best['key'] if best else '<module>'
            test = best['test'] if best else ps.is_test_path(path)
            if best is not None and simple(key) == name:
                continue  # the declaration itself
            dependents[name].add((path, key, test))

    reach = []
    for r in results:
        changed = {(s['file'], s['key']) for s in r['symbol_list']}
        measured = sorted(n for n, prs in targets.items() if r['pr'] in prs and n in unique)
        deps = set()
        for n in measured:
            deps |= {d for d in dependents.get(n, ()) if (d[0], d[1]) not in changed}
        reach.append(dict(pr=r['pr'], eligible=eligible[r['pr']], measured=len(measured),
                          dependents=len(deps), dependents_nontest=sum(1 for d in deps if not d[2]),
                          measured_names=measured,
                          top=sorted(((n, len(dependents.get(n, ()))) for n in measured), key=lambda x: -x[1])[:5]))
        print(f"#{r['pr']}: measured {len(measured)}/{eligible[r['pr']]} reach={len(deps)} "
              f"(outside tests {reach[-1]['dependents_nontest']})", flush=True)
    json.dump(reach, open(f'{a.results}/reach.json', 'w'), indent=1)
    ps.write_report(a.results, results, reach)


if __name__ == '__main__':
    main()
