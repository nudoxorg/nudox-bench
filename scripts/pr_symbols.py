#!/usr/bin/env python3
"""Count lines changed against declaration-level symbols changed in merged PRs.

Tests one claim: that a review measured in symbols is much smaller than one
measured in lines. For each of the last N pull requests merged into a branch,
it diffs the merge commit against its first parent and reports lines changed,
lines in languages Nudox indexes, and the symbols those lines fall in.

A "symbol" is the innermost named declaration that holds a changed line:
functions and methods, types (struct/enum/union/trait/class/interface/record),
impl blocks, consts/statics/type aliases, macros, modules, and (Python/TS)
top-level variables. Changed lines outside any declaration count toward one
module-level symbol per file. Blank lines never count, and comment-only lines
count only inside a non-container declaration (not between members of an
impl, class, trait or module); comment-only lines outside a declaration count
toward none. Attributes, decorators and doc comments directly above a
declaration belong to it. Declarations come from tree-sitter (Rust, Python,
C#, TypeScript), not from Nudox's own index.

Vendored code (vendor/, third_party/) and lockfiles are counted separately and
left out of "lines changed".

Needs: pip install -r scripts/requirements-pr-symbols.txt

Example (a blobless clone keeps the history small; blobs are fetched per diff):
  git clone --filter=blob:none --no-checkout --single-branch -b canonical \
      --shallow-since=2026-08-15 https://github.com/nudoxorg/backend /tmp/backend-history
  python3 scripts/pr_symbols.py --repo /tmp/backend-history --branch canonical \
      --count 20 --out results/2026-11-18-pr-symbols
Then, optionally, scripts/pr_reach.py adds how far each change reaches.
"""
import json, re, subprocess, sys, collections
from tree_sitter import Language, Parser
import tree_sitter_rust, tree_sitter_python, tree_sitter_c_sharp, tree_sitter_typescript

REPO = None

LANGS = {
    'rs': ('rust', Language(tree_sitter_rust.language())),
    'py': ('python', Language(tree_sitter_python.language())),
    'cs': ('csharp', Language(tree_sitter_c_sharp.language())),
    'ts': ('typescript', Language(tree_sitter_typescript.language_typescript())),
    'tsx': ('typescript', Language(tree_sitter_typescript.language_tsx())),
}
PARSERS = {ext: Parser(lang) for ext, (_, lang) in LANGS.items()}
# Languages Nudox has frontends for (frontends/: clang csharp go java python rust typescript)
INDEXED_EXT = {'rs', 'py', 'ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs', 'go', 'java', 'cs',
               'c', 'h', 'cc', 'cpp', 'cxx', 'hpp', 'hh'}

DECL = {
    'rust': {'function_item': 'fn', 'function_signature_item': 'fn', 'struct_item': 'struct',
             'enum_item': 'enum', 'union_item': 'union', 'trait_item': 'trait', 'impl_item': 'impl',
             'const_item': 'const', 'static_item': 'static', 'type_item': 'type',
             'associated_type': 'type', 'macro_definition': 'macro', 'mod_item': 'mod'},
    'python': {'function_definition': 'def', 'class_definition': 'class'},
    'csharp': {'class_declaration': 'class', 'struct_declaration': 'struct',
               'interface_declaration': 'interface', 'enum_declaration': 'enum',
               'record_declaration': 'record', 'record_struct_declaration': 'record',
               'method_declaration': 'method', 'constructor_declaration': 'ctor',
               'destructor_declaration': 'dtor', 'property_declaration': 'property',
               'field_declaration': 'field', 'event_declaration': 'event',
               'event_field_declaration': 'event', 'delegate_declaration': 'delegate',
               'operator_declaration': 'operator', 'conversion_operator_declaration': 'operator',
               'indexer_declaration': 'indexer', 'local_function_statement': 'fn',
               'namespace_declaration': 'namespace', 'file_scoped_namespace_declaration': 'namespace'},
    'typescript': {'function_declaration': 'function', 'generator_function_declaration': 'function',
                   'class_declaration': 'class', 'abstract_class_declaration': 'class',
                   'method_definition': 'method', 'interface_declaration': 'interface',
                   'type_alias_declaration': 'type', 'enum_declaration': 'enum',
                   'internal_module': 'namespace', 'public_field_definition': 'field'},
}
LEADING = {  # siblings that belong to the declaration right below them
    'rust': {'attribute_item', 'line_comment', 'block_comment'},
    'python': {'comment'},
    'csharp': {'comment'},
    'typescript': {'comment', 'decorator'},
}
CONTAINERS = {'mod', 'impl', 'trait', 'class', 'namespace', 'interface'}
COMMENT_RE = {
    'rust': re.compile(r'^\s*(//|/\*|\*)'),
    'python': re.compile(r'^\s*#'),
    'csharp': re.compile(r'^\s*(//|/\*|\*)'),
    'typescript': re.compile(r'^\s*(//|/\*|\*)'),
}


def git(*args, check=True):
    r = subprocess.run(['git', '-C', REPO, *args], capture_output=True)
    if check and r.returncode != 0:
        raise RuntimeError(f'git {args}: {r.stderr.decode(errors="replace")[:300]}')
    return r.stdout


def text(node):
    return node.text.decode('utf8', errors='replace') if node is not None else ''


def node_name(lang, node):
    t = node.type
    if lang == 'rust' and t == 'impl_item':
        ty = text(node.child_by_field_name('type'))
        tr = node.child_by_field_name('trait')
        return f'impl {text(tr)} for {ty}' if tr is not None else f'impl {ty}'
    n = node.child_by_field_name('name')
    if n is not None:
        return text(n)
    # C# fields/events: first variable_declarator identifier
    stack = list(node.children)
    while stack:
        c = stack.pop(0)
        if c.type == 'variable_declarator':
            nn = c.child_by_field_name('name')
            if nn is not None:
                return text(nn)
            for cc in c.children:
                if cc.type == 'identifier':
                    return text(cc)
        stack.extend(c.children)
    return '?'


def is_test_path(path):
    p = '/' + path
    fn = path.rsplit('/', 1)[-1]
    return ('/tests/' in p or '/test/' in p or '/fixtures/' in p or '/__tests__/' in p
            or fn.startswith('test_') or fn.endswith(('_test.py', '_test.rs', '_tests.rs', '.test.ts', '.test.tsx', '.spec.ts'))
            or fn in ('tests.rs', 'test.rs', 'conftest.py'))


def declarations(lang, ext, src, path):
    """Return list of decls: dict(start, end, key, kind, test) with 0-based inclusive rows."""
    tree = PARSERS[ext].parse(src)
    kinds = DECL[lang]
    out = []
    counts = collections.Counter()

    def leading_start(node):
        start = node.start_point[0]
        sib = node.prev_named_sibling
        cur = node
        while sib is not None and sib.type in LEADING[lang] and sib.end_point[0] >= cur.start_point[0] - 1:
            s = text(sib)
            if lang == 'rust' and (s.startswith('//!') or s.startswith('/*!')):
                break
            start = sib.start_point[0]
            cur = sib
            sib = sib.prev_named_sibling
        return start

    def rust_attrs(node):
        attrs = []
        sib = node.prev_named_sibling
        while sib is not None and sib.type in ('attribute_item', 'line_comment', 'block_comment'):
            if sib.type == 'attribute_item':
                attrs.append(text(sib))
            sib = sib.prev_named_sibling
        return ' '.join(attrs)

    def walk(node, quals, in_test):
        for child in node.children:
            t = child.type
            span_node = child
            if lang == 'python' and t == 'decorated_definition':
                inner = child.child_by_field_name('definition')
                if inner is None:
                    walk(child, quals, in_test)
                    continue
                span_node, child, t = child, inner, inner.type
            if t in kinds:
                name = node_name(lang, child)
                kind = kinds[t]
                test = in_test or is_test_path(path)
                if lang == 'rust':
                    a = rust_attrs(span_node)
                    if re.search(r'\btest\b|tokio::test|rstest|test_case|proptest', a) or (t == 'mod_item' and ('cfg(test)' in a or name in ('tests', 'test'))):
                        test = True
                elif lang == 'python':
                    if name.startswith('test') or (kind == 'class' and name.startswith('Test')):
                        test = True
                elif lang == 'csharp':
                    if re.search(r'\[(Fact|Theory|Test|TestMethod|TestCase)', text(child)[:400]):
                        test = True
                q = quals + [f'{kind} {name}']
                base_key = '::'.join(q)
                counts[base_key] += 1
                key = base_key if counts[base_key] == 1 else f'{base_key}#{counts[base_key]}'
                out.append(dict(start=leading_start(span_node), end=span_node.end_point[0] - (1 if span_node.end_point[1] == 0 else 0),
                                key=key, kind=kind, test=test))
                walk(child, q, test)
            elif lang in ('python', 'typescript') and node.type in ('module', 'program') and t in ('expression_statement', 'lexical_declaration', 'variable_declaration', 'export_statement'):
                # top-level variables as symbols (Python NAME = ..., TS const x = ...)
                nm = None
                if lang == 'python' and child.named_child_count and child.named_children[0].type == 'assignment':
                    left = child.named_children[0].child_by_field_name('left')
                    if left is not None and left.type == 'identifier':
                        nm = text(left)
                elif lang == 'typescript':
                    for c in child.named_children:
                        if c.type == 'variable_declarator':
                            nm = text(c.child_by_field_name('name'))
                            break
                        if c.type in ('lexical_declaration', 'variable_declaration'):
                            for cc in c.named_children:
                                if cc.type == 'variable_declarator':
                                    nm = text(cc.child_by_field_name('name'))
                                    break
                if nm:
                    q = quals + [f'var {nm}']
                    base_key = '::'.join(q)
                    counts[base_key] += 1
                    key = base_key if counts[base_key] == 1 else f'{base_key}#{counts[base_key]}'
                    out.append(dict(start=leading_start(child), end=child.end_point[0], key=key, kind='var',
                                    test=in_test or is_test_path(path)))
                walk(child, quals, in_test)
            else:
                walk(child, quals, in_test)

    walk(tree.root_node, [], False)
    return out


def attribute(lang, ext, src, path, rows):
    """Map changed rows (0-based) to symbol keys. Returns (dict key->decl, unattributed_rows)."""
    if not rows:
        return {}, 0
    lines = src.decode('utf8', errors='replace').split('\n')
    decls = declarations(lang, ext, src, path)
    # paint rows with innermost declaration (smaller spans paint last)
    owner = {}
    for i, d in sorted(enumerate(decls), key=lambda x: -(x[1]['end'] - x[1]['start'])):
        for r in range(d['start'], d['end'] + 1):
            if r in rows:
                owner[r] = i
    hit, skipped = {}, 0
    for r in rows:
        line = lines[r] if r < len(lines) else ''
        blank = not line.strip()
        comment = bool(COMMENT_RE[lang].match(line))
        if blank:
            skipped += 1  # whitespace never makes a symbol "changed"
            continue
        if r in owner:
            d = decls[owner[r]]
            if comment and d['kind'] in CONTAINERS:
                skipped += 1  # a comment between members is not a change to the container
                continue
            hit[d['key']] = d
            continue
        if comment:
            skipped += 1
            continue
        hit['<module>'] = dict(key='<module>', kind='module', test=is_test_path(path))
    return hit, skipped


def parse_diff(base, head):
    raw = git('diff', '-U0', '-M', '--no-color', '--no-ext-diff', base, head).decode('utf8', errors='replace')
    files = []
    cur = None
    for line in raw.split('\n'):
        if line.startswith('diff --git '):
            m = re.match(r'diff --git a/(.*) b/(.*)$', line)
            cur = dict(old=m.group(1), new=m.group(2), status='M', removed=set(), added=set(), binary=False)
            files.append(cur)
        elif cur is None:
            continue
        elif line.startswith('new file mode'):
            cur['status'] = 'A'
        elif line.startswith('deleted file mode'):
            cur['status'] = 'D'
        elif line.startswith('rename from '):
            cur['old'] = line[len('rename from '):]; cur['status'] = 'R'
        elif line.startswith('rename to '):
            cur['new'] = line[len('rename to '):]
        elif line.startswith('Binary files'):
            cur['binary'] = True
        elif line.startswith('@@'):
            m = re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@', line)
            os_, oc, ns, nc = int(m.group(1)), int(m.group(2) or 1), int(m.group(3)), int(m.group(4) or 1)
            cur['removed'].update(range(os_ - 1, os_ - 1 + oc))
            cur['added'].update(range(ns - 1, ns - 1 + nc))
    return files


def classify(path):
    fn = path.rsplit('/', 1)[-1]
    ext = fn.rsplit('.', 1)[-1] if '.' in fn else ''
    if path.startswith(('vendor/', 'third_party/')):
        return 'vendored', ext
    if fn.endswith('.lock'):
        return 'lockfile', ext
    if ext in INDEXED_EXT:
        return 'code', ext
    return 'other', ext



MERGE_GITEA = re.compile(r"Merge pull request '(.*)' \(#(\d+)\)")
MERGE_GITHUB = re.compile(r'Merge pull request #(\d+) from (\S+)')


def list_prs(branch, count):
    """The last `count` PR merge commits on `branch`'s first-parent chain."""
    out = git('log', '--first-parent', '--format=%H|%P|%ad|%s', '--date=short', branch).decode('utf8', errors='replace')
    lines = [l for l in out.split('\n') if l and (MERGE_GITEA.search(l) or MERGE_GITHUB.search(l))]
    return lines[:count]


def pr_title(subject):
    m = MERGE_GITEA.search(subject)
    if m:
        return m.group(1), int(m.group(2))
    m = MERGE_GITHUB.search(subject)
    if m:
        return m.group(2), int(m.group(1))
    return subject, None


def count_pr(line):
    h, parents, date, subject = line.rstrip('\n').split('|', 3)
    base = parents.split()[0]
    title, num = pr_title(subject)
    files = parse_diff(base, h)
    lines = collections.Counter()
    symbols = {}
    per_file = []
    unattributed = 0
    for f in files:
        path = f['new'] if f['status'] != 'D' else f['old']
        cls, ext = classify(path)
        n = len(f['added']) + len(f['removed'])
        if f['binary']:
            lines['binary_files'] += 1
            continue
        lines[cls] += n
        if cls == 'code':
            lines['code_' + ext] += n
            lines['code_test' if is_test_path(path) else 'code_nontest'] += n
        if cls != 'code' or ext not in LANGS:
            if cls == 'code':
                lines['code_unparsed'] += n
            continue
        lang = LANGS[ext][0]
        old_src = git('show', f'{base}:{f["old"]}') if f['status'] != 'A' else None
        new_src = git('show', f'{h}:{f["new"]}') if f['status'] != 'D' else None
        old_hits, new_hits = {}, {}
        if f['removed'] and old_src is not None:
            old_hits, s1 = attribute(lang, ext, old_src, path, f['removed'])
            unattributed += s1
        if f['added'] and new_src is not None:
            new_hits, s2 = attribute(lang, ext, new_src, path, f['added'])
            unattributed += s2
        # keys present in the other version decide added/removed/modified
        old_keys_all = ({d['key'] for d in declarations(lang, ext, old_src, path)} | {'<module>'}) if old_src is not None else None
        new_keys_all = ({d['key'] for d in declarations(lang, ext, new_src, path)} | {'<module>'}) if new_src is not None else None
        fsyms = 0
        for key, d in list(old_hits.items()) + list(new_hits.items()):
            k = (path, key)
            in_old = old_keys_all is not None and key in old_keys_all
            in_new = new_keys_all is not None and key in new_keys_all
            status = 'modified' if (in_old and in_new) else ('added' if in_new else 'removed')
            if k not in symbols:
                fsyms += 1
            symbols[k] = dict(file=path, key=key, kind=d['kind'], test=d['test'], status=status, lang=lang)
        per_file.append(dict(path=path, lines=n, symbols=fsyms, status=f['status']))
    sym_list = list(symbols.values())
    c = collections.Counter(s['status'] for s in sym_list)
    return dict(
        pr=num, title=title, date=date, merge=h[:10], base=base[:10],
        files=len(files), lines=dict(lines),
        reviewable_lines=lines['code'] + lines['other'],
        code_lines=lines['code'],
        symbols=len(sym_list), added=c['added'], modified=c['modified'], removed=c['removed'],
        test_symbols=sum(1 for s in sym_list if s['test']),
        module_symbols=sum(1 for s in sym_list if s['key'] == '<module>'),
        kinds=dict(collections.Counter(s['kind'] for s in sym_list)),
        unattributed_blank_or_comment_lines=unattributed,
        top_files=sorted(per_file, key=lambda x: -x['lines'])[:8],
        symbol_list=sym_list,
    )


CONTAINER_KINDS = {'module', 'mod', 'impl', 'namespace'}


def summarize(results, reach=None):
    """One row per PR plus headline statistics. `reach` is pr_reach.py's output, if run."""
    import statistics as st
    reach = {r['pr']: r for r in (reach or [])}
    rows = []
    for r in results:
        syms = r['symbol_list']
        rr = reach.get(r['pr'], {})
        rows.append(dict(
            pr=r['pr'], date=r['date'], title=r['title'], merge=r['merge'],
            lines_changed=r['reviewable_lines'], code_lines=r['code_lines'],
            vendored_lines=r['lines'].get('vendored', 0), lockfile_lines=r['lines'].get('lockfile', 0),
            symbols=len(syms), symbols_without_containers=sum(1 for s in syms if s['kind'] not in CONTAINER_KINDS),
            test_symbols=r['test_symbols'], added=r['added'], modified=r['modified'], removed=r['removed'],
            lines_per_symbol=round(r['code_lines'] / len(syms), 1) if syms else None,
            reach=rr.get('dependents'), reach_nontest=rr.get('dependents_nontest'),
            reach_measured=rr.get('measured'), reach_eligible=rr.get('eligible'),
        ))
    code = [x for x in rows if x['code_lines'] > 0 and x['symbols'] > 0]
    stats = dict(prs=len(rows), prs_with_code=len(code),
                 prs_without_code=[x['pr'] for x in rows if x['code_lines'] == 0])
    if code:
        stats.update(
            median_lines_per_symbol=round(st.median(x['code_lines'] / x['symbols'] for x in code), 1),
            min_lines_per_symbol=round(min(x['code_lines'] / x['symbols'] for x in code), 1),
            max_lines_per_symbol=round(max(x['code_lines'] / x['symbols'] for x in code), 1),
            pooled_lines_per_symbol=round(sum(x['code_lines'] for x in code) / sum(x['symbols'] for x in code), 1),
            median_lines_per_symbol_without_containers=round(st.median(
                x['code_lines'] / x['symbols_without_containers'] for x in code if x['symbols_without_containers']), 1),
            median_code_lines=st.median(x['code_lines'] for x in code),
            median_symbols=st.median(x['symbols'] for x in code),
        )
    return rows, stats


def fmt(n):
    return '–' if n is None else f'{n:,}'


def write_report(out_dir, results, reach=None):
    rows, stats = summarize(results, reach)
    json.dump(dict(stats=stats, rows=rows), open(f'{out_dir}/summary.json', 'w'), indent=1)
    md = ['| PR | merged | lines changed | code lines | symbols (tests) | lines per symbol | reach |',
          '|---|---|---:|---:|---:|---:|---:|']
    for x in rows:
        sym = f"{x['symbols']:,} ({x['test_symbols']:,})" if x['symbols'] else '–'
        lps = '–' if x['lines_per_symbol'] is None else f"{x['lines_per_symbol']:g}"
        if x['reach'] is None or x['code_lines'] == 0 or (x['reach_measured'] == 0 and x['reach_eligible']):
            rch = '–'
        else:
            rch = fmt(x['reach'])
        title = x['title'] if len(x['title']) <= 60 else x['title'][:57] + '...'
        md.append(f"| #{x['pr']} {title} | {x['date']} | {fmt(x['lines_changed'])} | {fmt(x['code_lines'])} | {sym} | {lps} | {rch} |")
    md.append('')
    md.append('```')
    md.append(json.dumps(stats, indent=1))
    md.append('```')
    open(f'{out_dir}/report.md', 'w').write('\n'.join(md) + '\n')
    return stats


def main():
    import argparse, os
    global REPO
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--repo', required=True, help='git checkout or clone holding the history (blobless is fine)')
    ap.add_argument('--branch', default='canonical', help='branch whose first-parent merges are PRs (default: canonical)')
    ap.add_argument('--count', type=int, default=20, help='how many of the latest merged PRs (default: 20)')
    ap.add_argument('--prs', help="file of 'sha|parents|date|subject' lines to use instead of listing the branch")
    ap.add_argument('--out', required=True, help='directory for prs.txt, results.json, summary.json and report.md')
    a = ap.parse_args()
    REPO = a.repo
    os.makedirs(a.out, exist_ok=True)
    prs = open(a.prs).read().splitlines() if a.prs else list_prs(a.branch, a.count)
    open(f'{a.out}/prs.txt', 'w').write('\n'.join(prs) + '\n')
    results = []
    for line in prs:
        r = count_pr(line)
        results.append(r)
        print(f"#{r['pr']} lines={r['reviewable_lines']} code={r['code_lines']} symbols={r['symbols']} "
              f"(+{r['added']} ~{r['modified']} -{r['removed']}) tests={r['test_symbols']}", flush=True)
    json.dump(results, open(f'{a.out}/results.json', 'w'), indent=1)
    print(json.dumps(write_report(a.out, results), indent=1))


if __name__ == '__main__':
    main()
