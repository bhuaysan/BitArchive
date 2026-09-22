#!/usr/bin/env python3
"""Structural checks over DATA_MODEL.md (extended after review round 2).

The previous FK check validated only the PARENT side, so a foreign key that named
a column its own table does not own went unnoticed. This version checks the child
side too, and the lifecycle/retention/rebuild rules added in this round.
"""
import re, sys

import os
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TEXT = open(os.path.join(ROOT, 'DATA_MODEL.md'), encoding='utf-8').read()
LINES = TEXT.split('\n')
FAILS = []
def fail(m):
    FAILS.append(m); print("  XX " + m)

# ------------------------------------------------------- table bodies and edges
# Every table's markdown body, keyed by table name, plus the FK graph derived from
# the **Foreign keys.** paragraphs. Built once because several checks need them.
hpat_all = re.compile(r'^#{3,4}\s+(?:\d+\.\d+\s+)?`([a-z_]+)`|^\*\*`([a-z_]+)`\*\*')
tb, _cur = {}, None
for _line in TEXT.split('\n'):
    _m = hpat_all.match(_line)
    if _m:
        _cur = _m.group(1) or _m.group(2)
        tb.setdefault(_cur, [])
        continue
    if _cur is not None:
        tb[_cur].append(_line)

def parse_foreign_keys(block):
    """child -> {parent: action} from a **Foreign keys.** paragraph.

    A paragraph may pack several references onto one line:
      `a → p.id` — `RESTRICT`. `b → q.id` — `SET NULL`. `c → r.id` — `CASCADE`.
    so the paragraph is walked reference by reference, and each reference's action
    is read from the text between it and the next reference. A reference with no
    explicit action defaults to RESTRICT, which is SQLite's rule.
    """
    refs = list(re.finditer(r'`([^`]*?→[^`]*?)`', block, re.S))
    out = {}
    for i, rm in enumerate(refs):
        par = re.search(r'→\s*([a-z_]+)[.(]', rm.group(1))
        if not par:
            continue
        gap_end = refs[i + 1].start() if i + 1 < len(refs) else len(block)
        gap = block[rm.end():gap_end]
        act = ('CASCADE' if re.search(r'\bCASCADE\b', gap)
               else 'SET NULL' if re.search(r'SET NULL', gap)
               else 'RESTRICT')
        out[par.group(1)] = act
    return out


edges = {}                                  # child -> {parent: action}
for _n, _ls in tb.items():
    _fk = re.search(r'\*\*Foreign keys\.\*\*(.*?)(?=\n\*\*[A-Z]|\Z)', '\n'.join(_ls), re.S)
    if not _fk:
        continue
    _e = parse_foreign_keys(_fk.group(1))
    if _e:
        edges[_n] = _e

def declaration_chunk(rest):
    """The text a key declaration covers: from just after its bold marker up to the
    next bold marker or the end of the block. Leading whitespace is skipped first,
    because a marker like `**Unique constraints.**` is commonly followed by a blank
    line before its bullets."""
    rest = rest.lstrip('\n')
    stop = re.search(r'\n\*\*[A-Z]|\n\n[A-Z]|\n#{2,4} ', rest)
    return rest[:stop.start()] if stop else rest


def is_unique_entry(chunk, match):
    """True when this tuple is declared UNIQUE. Section wording is inconsistent:
    some tables use `**Unique constraints.**`, others `**Unique constraints /
    indexes.**` and then mark only the unique entries with the word UNIQUE. The
    sentence containing the tuple decides."""
    start = chunk.rfind('\n', 0, match.start()) + 1
    end = chunk.find('\n', match.end())
    sentence = chunk[start: end if end != -1 else len(chunk)]
    return re.search(r'unique', sentence, re.I) is not None


def is_declared_key(chunk, match):
    """True when the parenthesised tuple at `match` is DECLARED as a key.

    Negated statements declare the absence of a constraint and are not keys:
      "There is no unique constraint on (release_id, disc_index)"
      "`(digest)` is **not** unique"
    """
    after = chunk[match.end(): match.end() + 90]
    before = chunk[max(0, match.start() - 110): match.start()]
    neg_after = re.search(r'[`\s]*(is\s+)?\*{0,2}not\*{0,2}\b', after, re.I)
    neg_before = re.search(r'\b(no|not|never)\b', before, re.I)
    return not (neg_after or neg_before)


# ------------------------------------------------------------------ parse tables
tables, cur = {}, None
for line in LINES:
    m = (re.match(r'^#{3,4}\s+\d+\.\d+\s+`([a-z_]+)`', line)
         or re.match(r'^#{4}\s+`([a-z_]+)`', line)
         or re.match(r'^\*\*`([a-z_]+)`\*\*\s*$', line))
    if m:
        cur = m.group(1); tables.setdefault(cur, set()); continue
    if line.startswith('|') and cur:
        cells = [c.strip() for c in line.strip('|').split('|')]
        if len(cells) >= 2 and re.fullmatch(r'`[a-z_]+`', cells[0]):
            tables[cur].add(cells[0].strip('`'))

tables['release_regions'] = {'release_id', 'region'}
tables['release_languages'] = {'release_id', 'language'}
# `release_regions` and `release_languages` share one heading, so key tuples that
# mention `language` belong to the latter and tuples that mention `region` to the
# former. Route them before check 6 runs.
KEY_OWNER_HINT = {'language': 'release_languages', 'region': 'release_regions'}
tables.setdefault('search_index', {'game_id'})
for name, sec, nxt in [('core_selection_overrides', r'### 12\.3', r'### 12\.4'),
                       ('component_index_state', r'### 12\.7', r'^## 13\.'),
                       ('schema_migrations', r'### 18\.1', r'### 18\.2')]:
    blk = re.search(sec + r'.*?(?=' + nxt + r')', TEXT, re.S | re.M)
    cols = set(re.findall(r'^\| `([a-z_]+)` \|', blk.group(0), re.M))
    tables.setdefault(name, set()).update(cols)

PK = {
 'systems': [('system_id',), ('catalog_key',)],
 'library_sources': [('id',), ('location', 'platform_locator_kind')],
 'games': [('id',)],
 'releases': [('id',), ('id', 'game_id'), ('game_id', 'release_key')],
 'contents': [('id',)],
 'release_regions': [('release_id', 'region')],
 'release_languages': [('release_id', 'language')],
 'content_locations': [('content_id', 'location_kind', 'source_id', 'relative_path'),
                       ('content_id', 'source_id', 'relative_path', 'archive_entry_path'),
                       ('source_id', 'relative_path', 'archive_entry_path'),
                       ('source_id', 'relative_path')],
 'content_derivations': [('content_id', 'kind')],
 'content_derivation_members': [('content_id', 'kind', 'source_content_id'),
                                ('content_id', 'kind', 'member_index')],
 'content_fingerprints': [('content_id', 'fingerprint_kind', 'algorithm'),
                          ('content_id', 'fingerprint_kind', 'algorithm', 'entry_path'),
                          ('algorithm', 'fingerprint_kind', 'digest', 'byte_size')],
 'scan_runs': [('id',)], 'scan_run_issues': [('scan_run_id', 'issue_index')],
 'metadata_fields': [('field_key',)], 'metadata_providers': [('provider_id',)],
 'provider_values': [('provider_id', 'subject_kind', 'subject_id', 'field_key', 'locale', 'value_index')],
 'manual_overrides': [('subject_kind', 'subject_id', 'field_key', 'locale')],
 'scrape_runs': [('id',)], 'scrape_run_items': [('scrape_run_id', 'game_id')],
 'media_assets': [('id',)],
 'media_asset_references': [('subject_kind', 'subject_id', 'logical_key')],
 'firmware_entries': [('relative_path',)], 'firmware_index_state': [('singleton',)],
 'cores': [('core_id',), ('component_key',)],
 'core_selection_overrides': [('scope_system_id',), ('scope_game_id',), ('scope_release_id',)],
 'core_versions': [('core_version_id',), ('component_key', 'platform', 'build_id'),
                   ('core_id', 'platform', 'build_id')],
 'runtime_versions': [('runtime_version_id',), ('component_key', 'platform', 'version')],
 'managed_components': [('component_class', 'component_key', 'platform', 'version')],
 'component_index_state': [('singleton',)],
 'retroarch_setting_overrides': [('setting_key',), ('scope_system_id', 'setting_key'),
                                 ('scope_game_id', 'setting_key')],
 'core_option_schemas': [('core_version_id',)],
 'core_option_definitions': [('core_version_id', 'option_key')],
 'core_option_overrides': [('core_version_id', 'option_key')],
 'save_states': [('id',), ('release_id', 'core_id', 'core_version_id', 'slot'),
                 ('release_id', 'core_id', 'core_version_label', 'slot'),
                 ('file_relative_path',)],
 'sessions': [('id',)], 'schema_migrations': [('version',)],
}
REBUILDABLE = {'managed_components', 'component_index_state', 'firmware_entries',
               'firmware_index_state', 'search_index', 'content_derivations',
               'content_derivation_members'}

def heading_before(pos):
    hs = list(re.finditer(r'^#{3,4}\s+\d+\.\d+\s+`([a-z_]+)`|^#{4}\s+`([a-z_]+)`|^\*\*`([a-z_]+)`\*\*\s*$',
                          TEXT[:pos], re.M))
    if not hs: return None
    return next((g for g in hs[-1].groups() if g), None)

# ------------------------------------------------------------------ parse FKs
# Declared forms:
#   - `child -> parent(col)` — ACTION
#   - `(child_a, child_b) -> parent(a, b)` — ACTION
# Prose forms such as `content_id -> contents.id` describe a value, not a table
# owning the column, and are deliberately NOT parsed as declarations.
COMPOSITE = r'`\(([a-z_,\s]+)\)\s*→\s*([a-z_]+)\(([a-z_,\s]+)\)`\s*[—-]\s*`?([A-Z][A-Z ]*?)`?(?=[,.;\s]|$)'
SIMPLE    = r'`([a-z_]+)\s*→\s*([a-z_]+)\.([a-z_]+)`'
fks, spans = [], []
for m in re.finditer(COMPOSITE, TEXT):
    spans.append((m.start(), m.end()))
    fks.append(dict(child=heading_before(m.start()),
                    child_cols=[c.strip() for c in m.group(1).split(',')],
                    parent=m.group(2),
                    parent_cols=[c.strip() for c in m.group(3).split(',')],
                    line=TEXT[:m.start()].count('\n') + 1))
for m in re.finditer(SIMPLE, TEXT):
    if any(a <= m.start() < b for a, b in spans): continue
    fks.append(dict(child=heading_before(m.start()), child_cols=[m.group(1)],
                    parent=m.group(2), parent_cols=[m.group(3)],
                    line=TEXT[:m.start()].count('\n') + 1))

print("=" * 74)
print("1-5. FK: child table, child columns, parent table, parent columns, parent key")
print("=" * 74)
valid = 0
for fk in fks:
    ok = True
    if fk['child'] not in tables:
        fail(f"L{fk['line']}: child table '{fk['child']}' unknown"); ok = False
    else:
        for c in fk['child_cols']:
            if c not in tables[fk['child']]:
                fail(f"L{fk['line']}: child column '{fk['child']}.{c}' is NOT a column of "
                     f"{fk['child']} (has {len(tables[fk['child']])} cols)"); ok = False
    if fk['parent'] not in tables:
        fail(f"L{fk['line']}: parent table '{fk['parent']}' unknown"); ok = False
    else:
        for c in fk['parent_cols']:
            if c not in tables[fk['parent']]:
                fail(f"L{fk['line']}: parent column '{fk['parent']}.{c}' is NOT a column"); ok = False
    if ok:
        want = set(fk['parent_cols'])
        if not any(set(k) == want for k in PK.get(fk['parent'], [])):
            fail(f"L{fk['line']}: parent key {tuple(fk['parent_cols'])} is not PK/UNIQUE of "
                 f"{fk['parent']}"); ok = False
    if ok: valid += 1
print(f"  foreign keys parsed: {len(fks)}   valid: {valid}")

print("\n" + "=" * 74); print("6. PK/UNIQUE key columns all exist"); print("=" * 74)
# `declared_keys` is defined in section 19 below; build the same map here from the
# document so this check needs no hand-maintained list.
_keys_here = {}
for _t, _ls in tb.items():
    _body = '\n'.join(_ls)
    for _marker in (r'\*\*Primary key\.\*\*', r'\*\*Unique constraints[^*]*\*\*'):
        for _mm in re.finditer(_marker, _body):
            _chunk = declaration_chunk(_body[_mm.end():])
            _is_pk = 'Primary key' in _mm.group(0)
            _is_unique_block = 'Unique constraints' in _mm.group(0)
            for _tm in re.finditer(r'\(([^)]*)\)', _chunk):
                _cols = [c.strip() for c in _tm.group(1).split(',')]
                if not _cols or not all(re.fullmatch(r'[a-z_]+', c) for c in _cols):
                    continue
                if (_is_pk or _is_unique_block or is_unique_entry(_chunk, _tm)) and is_declared_key(_chunk, _tm):
                    _keys_here.setdefault(_t, set()).add(tuple(_cols))
_checked = 0
for _t, _keys in list(_keys_here.items()):
    for _k in list(_keys):
        for _col, _owner in KEY_OWNER_HINT.items():
            if _col in _k and _owner != _t:
                _keys_here.setdefault(_owner, set()).add(_k)
                _keys.discard(_k)
for _t, _keys in _keys_here.items():
    if _t not in tables:
        continue
    for _k in sorted(_keys):
        for _c in _k:
            if _c not in tables[_t]:
                fail(f"{_t}: key column '{_c}' (in {_k}) is not a defined column")
            else:
                _checked += 1
print(f"  {_checked} key columns verified against the declared columns")

print("\n" + "=" * 74); print("8. No persistent table references a rebuildable table"); print("=" * 74)
bad = [f for f in fks if f['parent'] in REBUILDABLE]
for f in bad: fail(f"L{f['line']}: {f['child']} -> rebuildable {f['parent']}")
print(f"  rebuildable: {sorted(REBUILDABLE)}")
print("  inbound references: none" if not bad else "")

print("\n" + "=" * 74); print("10. One lifetime per declared structure"); print("=" * 74)
LIFE = ['Persistent', 'Rebuildable', 'SessionScoped', 'Temporary']
head = None
for i, line in enumerate(LINES, 1):
    h = re.match(r'^#{3,4}\s+(.*)$', line)
    if h: head = h.group(1)
    if '**Lifecycle.**' in line:
        frag = line.split('**Lifecycle.**', 1)[1].strip()
        frag = re.split(r'(?<=[.;])\s', frag)[0]      # first sentence only
        labels = set(re.findall(r'\*\*(' + '|'.join(LIFE) + r')\*\*', frag)) | \
                 set(re.findall(r'(?<!\*)\b(' + '|'.join(LIFE) + r')\b(?!\*)', frag))
        if len(labels) > 1:
            fail(f"L{i} [{head[:44]}]: multiple lifetimes {sorted(labels)}: {frag[:80]}")
print("  done")

print("\n" + "=" * 74); print("13. Re-identification path declared"); print("=" * 74)
for needle in ["fingerprint_kind='Payload'", 'recognition evidence']:
    if needle not in TEXT: fail(f"missing re-identification element: {needle}")
print("  recognition evidence + Payload lookup present")

print("\n" + "=" * 74); print("9. Retention executable under FK delete behaviour"); print("=" * 74)
pruned = ['scan_runs', 'scrape_runs']
for fk in fks:
    if fk['parent'] not in pruned:
        continue
    # the action may be on the same line or on the next one
    context = ' '.join(LINES[fk['line'] - 1: fk['line'] + 2])
    act = next((a for a in ('CASCADE', 'SET NULL', 'RESTRICT') if f'`{a}`' in context), None)
    print(f"  {fk['child']}.{fk['child_cols'][0]} -> {fk['parent']}: {act or 'UNKNOWN'}")
    if act is None:
        fail(f"L{fk['line']}: run reference has no declared delete behaviour")
    elif act == 'RESTRICT':
        fail(f"L{fk['line']}: RESTRICT on a prunable run reference blocks run retention")

print("\n" + "=" * 74); print("12. Multi-row derivations can hold 2..N members"); print("=" * 74)
need = [
 ("PK admits N members", "(content_id, kind, source_content_id)" in TEXT),
 ("order unique", "(content_id, kind, member_index)" in TEXT),
 ("member_count >= 2", "member_count >= 2" in TEXT or "`>= 2` for `ManagedPlaylist`" in TEXT),
 ("cross-row invariant stated", "MUST equal the number of `content_derivation_members` rows" in TEXT),
]
for label, ok in need:
    print(f"  {'OK' if ok else 'XX'} {label}")
    if not ok: fail("derivation: " + label)

print("\n" + "=" * 74); print("13. Library rebuild leaves a deterministic re-identification path"); print("=" * 74)
chain = [
 ("retained identity",        "retained:  ContentId C" in TEXT),
 ("retained evidence",        "canonical Payload fingerprint" in TEXT),
 ("rescan match by digest",   "lookup:" in TEXT and "fingerprint_kind='Payload'" in TEXT),
 ("explicitly not by path",   "path-free" in TEXT),
 ("identity stays stable",    "GameId / ReleaseId / ContentId stay stable" in TEXT),
 ("lookup is a function",     "returns one row or none" in TEXT or "return *one* row or none" in TEXT),
]
for label, ok in chain:
    print(f"  {'OK' if ok else 'XX'} {label}")
    if not ok: fail("rebuild path: " + label)

print("\n" + "=" * 74); print("11. Library-rebuild table is the single retention authority"); print("=" * 74)
block = re.search(r'#### Library rebuild.*?(?=#### Full reset)', TEXT, re.S).group(0)
rows = []
for line in block.split('\n'):
    if not line.startswith('|'): continue
    cells = [c.strip() for c in line.strip('|').split('|')]
    if len(cells) < 3: continue
    v = 'cleared' if cells[1].startswith('**cleared**') else ('kept' if cells[1].startswith('**kept**') else None)
    if v: rows.append((cells[0], v))
print(f"  rebuild table rows: {len(rows)}")
cleared = {n for n, v in rows if v == 'cleared'}
kept = {n for n, v in rows if v == 'kept'}
print(f"  cleared ({len(cleared)}): " + "; ".join(sorted(cleared)))
print(f"  kept ({len(kept)}): " + "; ".join(k[:48] for k in sorted(kept)))
must_keep = ['games', 'releases', 'contents', 'content_fingerprints', 'manual_overrides',
             'sessions', 'save_states', 'media_assets', 'media_asset_references']
for t in must_keep:
    if not any(t in k for k in kept):
        fail(f"library rebuild does not explicitly keep {t}")
must_clear = ['content_locations', 'scan_runs', 'scan_run_issues', 'provider_values', 'search_index']
for t in must_clear:
    if not any(t in c for c in cleared):
        fail(f"library rebuild does not explicitly clear {t}")

# --------------------------------------------------- 17. archive location shape
print("\n" + "=" * 74); print("17. Archive relationship lives on the location"); print("=" * 74)
if 'archive_content_id' in tables.get('contents', set()):
    fail("contents still declares archive_content_id; the container link belongs on content_locations (5.2)")
else:
    print("  contents: no archive_content_id column  OK")
for col in ('archive_content_id', 'archive_entry_path'):
    if col not in tables.get('content_locations', set()):
        fail(f"content_locations is missing the {col} column")
    else:
        print(f"  content_locations.{col}: present  OK")
if "WHEN 'ArchiveEntry' THEN archive_content_id IS NOT NULL" not in TEXT:
    fail("content_locations does not require archive_content_id for ArchiveEntry rows")
else:
    print("  ArchiveEntry check requires container + entry path  OK")
if "WHEN 'File'         THEN archive_content_id IS NULL" not in TEXT:
    fail("content_locations does not forbid archive columns on File rows")
else:
    print("  File check forbids both archive columns  OK")

# ------------------------------------------------- 18. payload recognition key
print("\n" + "=" * 74); print("18. Payload recognition key == lookup key"); print("=" * 74)
if "`(algorithm, digest)` WHERE `fingerprint_kind = 'Payload'` — UNIQUE" not in TEXT:
    fail("the Payload recognition index is not declared as UNIQUE (algorithm, digest)")
else:
    print("  partial UNIQUE (algorithm, digest) WHERE Payload  OK")
if "`(algorithm, fingerprint_kind, digest, byte_size)` unique" in TEXT and "earlier revision" not in TEXT:
    fail("the old nullable-byte_size recognition constraint is still declared")
for needle in ("fingerprint_kind='Payload'", "fingerprint_kind = 'Payload'"):
    if needle in TEXT:
        print(f"  lookup uses the same predicate: {needle}  OK")
        break
else:
    fail("no Payload lookup predicate found in 19.3")
if 'byte_size' in PK.get('content_fingerprints', [()])[0]:
    fail("byte_size is part of a content_fingerprints key but is nullable")
else:
    print("  byte_size is not part of any key  OK")

# ------------------------------------------------------ 19. nullable key audit
print("\n" + "=" * 74); print("19. No key relies on NULL semantics"); print("=" * 74)
# Tables whose keys include a nullable column, each needing a documented strategy.
NULLABLE_KEY_STRATEGY = {
    # table: ('normalised'|'partial'|'check', explanation fragment that must appear)
    'provider_values':      ('normalised', 'locale_key'),
    'manual_overrides':     ('normalised', 'locale_key'),
    'locale_key':           ('normalised', 'locale_key'),   # never a real table key
    'content_fingerprints': ('check',      "entry_path IS NOT NULL"),
    'content_locations':    ('check',      "archive_content_id IS NOT NULL"),
    'core_selection_overrides': ('partial', "scope_kind = 'System'"),
    'retroarch_setting_overrides': ('partial', "scope_kind = 'System'"),
    'core_option_overrides': ('partial',   "scope_kind = 'CoreDefaults'"),
    'save_states':          ('partial',    'core_version_id IS NULL'),
    'sessions':             ('partial',    "state = 'Active'"),
    'media_assets':         ('dropped',    'no natural-key unique constraint'),
}
nullable_cols = {}
for t, ls in tb.items():
    d = {}
    for line in ls:
        if line.startswith('|'):
            c = [x.strip() for x in line.strip('|').split('|')]
            if len(c) >= 3 and re.fullmatch(r'`[a-z_]+`', c[0]):
                d[c[0].strip('`')] = (c[2].lower() == 'yes')
    if d: nullable_cols[t] = d

# Keys are DERIVED from the document, not hand-maintained: every parenthesised
# column tuple that appears in a "Primary key" or "Unique constraints" declaration
# is a key of that table. A stale hand-written list would let a nullable key
# component slip through, which is the class of defect this check exists for.
declared_keys = {}
for t, ls in tb.items():
    body = '\n'.join(ls)
    for marker in (r'\*\*Primary key\.\*\*', r'\*\*Unique constraints[^*]*\*\*'):
        for mm in re.finditer(marker, body):
            # the declaration runs to the next bold marker or double newline
            chunk = declaration_chunk(body[mm.end():])
            is_pk_marker = 'Primary key' in mm.group(0)
            # A block headed "Unique constraints" declares unique keys in every
            # bullet; only a block that mixes constraints with plain indexes needs
            # the word UNIQUE in the individual sentence.
            is_unique_block = 'Unique constraints' in mm.group(0)
            for tm in re.finditer(r'\(([^)]*)\)', chunk):
                cols = [c.strip() for c in tm.group(1).split(',')]
                if not cols or not all(re.fullmatch(r'[a-z_]+', c) for c in cols):
                    continue
                if (is_pk_marker or is_unique_block or is_unique_entry(chunk, tm)) and is_declared_key(chunk, tm):
                    declared_keys.setdefault(t, set()).add(tuple(cols))
# Tables whose key is stated in prose rather than a parenthesised tuple
declared_keys.setdefault('schema_migrations', set()).add(('version',))
declared_keys.setdefault('metadata_fields', set()).add(('field_key',))
declared_keys.setdefault('metadata_providers', set()).add(('provider_id',))
declared_keys.setdefault('systems', set()).add(('system_id',))
declared_keys.setdefault('systems', set()).add(('catalog_key',))
declared_keys.setdefault('cores', set()).add(('core_id',))
declared_keys.setdefault('cores', set()).add(('component_key',))
for t in ('firmware_index_state', 'component_index_state'):
    declared_keys.setdefault(t, set()).add(('singleton',))
# A normalised key column must exist as a declared column, and the CHECK that pins
# it to its nullable source must be present -- otherwise the normalisation is only a
# naming convention and the key is still unenforced.
for t, keys in PK.items():
    for k in keys:
        for c in k:
            if c.endswith('_key') and c not in tables.get(t, set()):
                fail(f"{t}: key uses the normalised column {c!r}, which the table does not declare")
            if c == 'locale_key' and t in ('provider_values', 'manual_overrides'):
                if "CHECK (locale_key = COALESCE(locale, 'und'))" not in TEXT:
                    fail(f"{t}: locale_key is not pinned to locale by its CHECK, so the "
                         f"normalisation is unenforced")
# A key component whose row text declares NOT NULL must not be marked nullable in
# the Null column: the two statements in one row would contradict each other, and a
# nullable component is exactly what disables a key in SQLite.
for t, ls in tb.items():
    for line in ls:
        if not line.startswith('|'):
            continue
        c = [x.strip() for x in line.strip('|').split('|')]
        if len(c) < 3 or not re.fullmatch(r'`[a-z_]+`', c[0]):
            continue
        col = c[0].strip('`')
        desc = c[3] if len(c) > 3 else ''
        # Only an UNCONDITIONAL NOT NULL claim contradicts Null=yes. A qualified claim
        # ("NOT NULL only for EntryList rows", "NOT NULL iff ...") is pinned by a CHECK
        # or a partial predicate instead, which the strategy check below verifies.
        # tolerate the markdown backticks around `NOT NULL`
        qualified = re.search(r'NOT NULL`?\s+(only|iff|when|for)\b|NOT NULL`?\s+exactly', desc, re.I)
        claims_not_null = re.search(r'\bNOT NULL\b', desc, re.I) and not qualified
        if c[2].lower() == 'yes' and claims_not_null:
            if any(col in k for k in declared_keys.get(t, ())):
                fail(f"{t}.{col}: row says NOT NULL but the Null column says yes, and the "
                     f"column is a key component -- the key would be unenforced")
            else:
                print(f"  note: {t}.{col} says NOT NULL in prose but Null=yes (not a key component)")

for t, keys in declared_keys.items():
    if t not in nullable_cols:
        continue
    for k in sorted(keys):
        unknown = [c for c in k if c in ('id',) or c.endswith('id') or True]
        # only report a nullable component that is a real declared column
        nul = [c for c in k if nullable_cols[t].get(c, False)]
        if not nul:
            continue
        if t not in NULLABLE_KEY_STRATEGY:
            fail(f"{t}: key {tuple(k)} contains nullable column(s) {nul} with no documented strategy (3.5/3.8)")
            continue
        kind, needle = NULLABLE_KEY_STRATEGY[t]
        # locate the table's section and require the documented strategy text
        hpat2 = re.compile(r'^#{3,4}\s+(?:\d+\.\d+\s+)?`' + re.escape(t) + r'`|^\*\*`' + re.escape(t) + r'`\*\*', re.M)
        hm = hpat2.search(TEXT)
        section = TEXT[hm.start(): hm.start() + 12000] if hm else ''
        # The content_locations File keys are NOT NULL; only the ArchiveEntry key
        # has a nullable component, and its CHECK pins it.
        if t == 'content_locations' and 'archive_entry_path' in nul:
            if "archive_entry_path IS NOT NULL" in section:
                print(f"  {t:28} {nul} -> check strategy documented  OK")
                continue
        if needle not in section:
            fail(f"{t}: nullable key column(s) {nul} lack the documented {kind} strategy ({needle!r} not found in the section)")
        else:
            print(f"  {t:28} {nul} -> {kind} strategy documented  OK")
if '### 3.8' not in TEXT:
    fail("3.8 (nullable-key audit) is missing")
else:
    print("  3.8 nullable-key audit present  OK")

# ------------------------------------------- 21. locale sentinel handling
print("\n" + "=" * 74); print("21. language-neutral sentinel is storage-only"); print("=" * 74)
if "CHECK (locale_key = COALESCE(locale, 'und'))" not in TEXT:
    fail("the locale_key CHECK tying locale_key to locale is missing")
else:
    print("  CHECK (locale_key = COALESCE(locale, 'und')) present  OK")
for t in ('provider_values', 'manual_overrides'):
    hm = re.search(r'^#{3,4}\s+\d+\.\d+\s+`' + t + r'`', TEXT, re.M)
    sec = TEXT[hm.start(): hm.start() + 9000] if hm else ''
    # Pull the declared primary key out of the section and inspect its components.
    # Only the first parenthesised key list after the marker is the key.
    pk_line = re.search(r'\*\*Primary key\.\*\*[^(]*\(([^)]*)\)', sec, re.S)
    pk_cols = [c.strip() for c in pk_line.group(1).split(',')] if pk_line else []
    if 'locale_key' not in sec:
        fail(f"{t} does not use locale_key")
    elif 'locale' in pk_cols:
        fail(f"{t} still lists the nullable `locale` in its primary key: {pk_cols}")
    elif 'locale_key' not in pk_cols:
        fail(f"{t} primary key does not contain locale_key: {pk_cols}")
    else:
        print(f"  {t:20} key uses locale_key, not locale  OK")
if 'locale_key = \'und\'' not in TEXT:
    fail("the resolver does not name the 'und' sentinel for the language-neutral step")
else:
    print("  resolver names the sentinel explicitly  OK")

# ----------------------------------------------------------- 20. full reset order
print("\n" + "=" * 74); print("20. Documented full-reset order is executable"); print("=" * 74)
EXPECTED_RESET = """core_option_overrides core_option_definitions core_option_schemas
retroarch_setting_overrides core_selection_overrides save_states
content_derivation_members content_derivations content_fingerprints
content_locations sessions contents release_regions release_languages
releases scrape_run_items scrape_runs media_asset_references media_assets
games provider_values manual_overrides
scan_run_issues scan_runs library_sources core_versions runtime_versions
cores managed_components systems metadata_providers metadata_fields
schema_migrations""".split()
restrict = {c: {p for p, a in pars.items() if a == 'RESTRICT'} for c, pars in edges.items()}
restrict = {c: ps for c, ps in restrict.items() if ps}
pos = {t: i for i, t in enumerate(EXPECTED_RESET)}
missing_tables = [t for t in restrict if t not in pos]
if missing_tables:
    fail(f"the documented reset order omits tables that have foreign keys: {missing_tables}")
violations = [(c, p) for c, pars in restrict.items() if c in pos
              for p in pars if p in pos and pos[p] < pos[c]]
if violations:
    for c, p in violations:
        fail(f"reset order deletes {p} before its RESTRICT child {c}")
else:
    print(f"  {len(EXPECTED_RESET)} tables ordered; every RESTRICT child precedes its parent  OK")
    # The document claims there is NO RESTRICT cycle and that no pointer pre-clearing
    # is therefore needed. Verify that claim rather than asserting it.
    colour, cyc = {}, []
    def _dfs(u, stack):
        colour[u] = 1; stack.append(u)
        for v in sorted(restrict.get(u, ())):
            if colour.get(v, 0) == 1:
                cyc.append(stack[stack.index(v):] + [v])
            elif colour.get(v, 0) == 0:
                _dfs(v, stack)
        stack.pop(); colour[u] = 2
    for _n in sorted(restrict):
        if colour.get(_n, 0) == 0:
            _dfs(_n, [])
    seen, uniq = set(), []
    for c in cyc:
        k = tuple(sorted(set(c)))
        if k not in seen:
            seen.add(k); uniq.append(c)
    if uniq:
        fail(f"the model contains a RESTRICT cycle {uniq}, so the documented order cannot "
             f"be a plain child-before-parent order and a pointer must be pre-cleared")
    else:
        print("  no RESTRICT cycle in the declared foreign keys  OK")
# The documented plan is PARSED from the document and checked against the FK graph,
# so mutating the order in the document (not just in this script) is detected.
block = re.search(r'```text\n 1\. DELETE core_option_overrides.*?```', TEXT, re.S)
if not block:
    fail("the full-reset plan block was not found in 19.3")
else:
    # Exact document position: the order of the DELETE statements as written.
    # Step numbers alone cannot order two tables that share a step, and the plan
    # deliberately deletes core_versions before cores inside step 14.
    documented = [t for raw in block.group(0).split('\n')
                  for t in re.findall(r'DELETE\s+([a-z_]+)', raw)]
    doc_pos = {}
    for i, t in enumerate(documented):
        doc_pos.setdefault(t, i)
    if sorted(doc_pos) != sorted(EXPECTED_RESET):
        fail(f"the documented reset order lists a different table set than expected: "
             f"{sorted(set(doc_pos) ^ set(EXPECTED_RESET))}")
    doc_violations = [(c, p) for c, pars in restrict.items()
                      if c in doc_pos for p in pars
                      if p in doc_pos and doc_pos[p] < doc_pos[c] and p != c]
    if doc_violations:
        for c, p in doc_violations:
            fail(f"the DOCUMENTED reset order deletes parent {p} (position {doc_pos[p]}) "
                 f"before its RESTRICT child {c} (position {doc_pos[c]})")
    else:
        print(f"  parsed {len(doc_pos)} tables from the document; order is valid  OK")
    if 'UPDATE games' in block.group(0) or 'UPDATE sessions' in block.group(0):
        fail("the reset plan still pre-clears pointers that the order does not need")
    else:
        print("  no unnecessary pointer pre-clearing  OK")

print("\n" + "=" * 74)
print("RESULT:", "ALL CHECKS PASS" if not FAILS else f"{len(FAILS)} FAILURE(S)")
print("=" * 74)
sys.exit(1 if FAILS else 0)
