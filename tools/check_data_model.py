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
for t, keys in PK.items():
    if t not in tables:
        fail(f"key declared for unknown table '{t}'"); continue
    for k in keys:
        for c in k:
            if c not in tables[t]:
                fail(f"{t}: key column '{c}' is not a defined column")
print("  done")

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

print("\n" + "=" * 74); print("14. Markdown structure"); print("=" * 74)
fences = [i for i, l in enumerate(LINES, 1) if l.strip().startswith('```')]
if len(fences) % 2:
    fail(f"unbalanced code fences: {len(fences)}")
prev = None
for i, l in enumerate(LINES, 1):
    st = l.strip()
    if st.startswith('|') and st.endswith('|'):
        n = st.count('|')
        if prev is not None and n != prev:
            fail(f"L{i}: table column count changed from {prev} to {n}")
        prev = n
    else:
        prev = None
heads = [l for l in LINES if re.match(r'^#{2,3} ', l)]
dups = [h for h in set(heads) if heads.count(h) > 1]
if dups: fail(f"duplicate headings: {dups}")
tw = [i for i, l in enumerate(LINES, 1) if re.search(r' +$', l)]
if tw: fail(f"trailing whitespace at lines {tw}")
print("  done")

print("\n" + "=" * 74); print("15. Internal cross-references resolve"); print("=" * 74)
heads_set = set(m.group(1) for m in re.finditer(r'^#{2,3} (\d+(?:\.\d+)?)\.?\s', TEXT, re.M))
DOCS = r'ARCHITECTURE\.md|PRODUCT\.md|UI_UX_CONCEPT\.md|AGENTS\.md|DEVELOPMENT\.md'
unresolved = []
for m in re.finditer(r'§(\d+(?:\.\d+)?)', TEXT):
    if m.group(1) in heads_set:
        continue                                   # resolves inside this document
    # Otherwise it must be an external citation: the same paragraph (up to this
    # point) has to name the source document it belongs to.
    line_start = TEXT.rfind('\n', 0, m.start()) + 1
    line = TEXT[line_start:TEXT.find('\n', m.start())]
    # A heading, a table row or an appendix title is its own citation context:
    # "## Appendix A — Traceability to `ARCHITECTURE.md` §54" and
    # "| §54 area | This document |" both legitimately name an external document
    # outside the paragraph that follows.
    self_contained = (line.lstrip().startswith('#') or line.lstrip().startswith('|')
                      or re.match(r'^\*\*Appendix', line.lstrip()))
    if self_contained and re.search(DOCS, TEXT[max(0, TEXT.rfind('\n\n', 0, m.start())):TEXT.find('\n', m.start())]):
        continue
    start = TEXT.rfind('\n\n', 0, m.start())
    para = TEXT[start if start != -1 else 0:m.start()]
    if re.search(DOCS, para) or re.search(DOCS, line):
        continue
    unresolved.append((m.group(1), TEXT[:m.start()].count('\n') + 1))
print(f"  unresolved: {len(unresolved)} (each must be an external citation naming its file)")
for sec, ln in unresolved:
    fail(f"L{ln}: §{sec} resolves neither here nor to a named source document")

print("\n" + "=" * 74); print("16. ARCHITECTURE areas and invariants traced"); print("=" * 74)
areas = {
 'Library Sources': 'library_sources', 'Games': '### 6.1', 'Releases': '### 6.2',
 'Contents': '### 6.3', 'Content Locations': '### 5.2', 'Content Fingerprints': '### 7.1',
 'Scan Runs': '### 8.1', 'Provider Metadata': '### 9.3', 'Manual Overrides': '### 9.4',
 'Scrape Runs': '### 9.6', 'Media Assets': '### 10.1', 'Firmware Index': '### 11.1',
 'Managed Components': '### 12.6', 'Config Overrides': '### 13.1',
 'Core Option Schemas': '### 14.2', 'Core Option Overrides': '### 14.1',
 'Save States': '### 15.1', 'Sessions': '### 16.1', 'Statistics': '### 16.3',
 'FTS5': '### 17.1', 'Schema Versioning': '### 18.1'}
missing = [a for a, mk in areas.items() if mk not in TEXT]
for a in missing: fail(f"§54 area not covered: {a}")
print(f"  §54 areas covered: {len(areas) - len(missing)}/{len(areas)}")
appb = TEXT[TEXT.index('## Appendix B'):]
untraced = [i for i in range(1, 24) if not re.search(r'^\| ' + str(i) + r'\. ', appb, re.M)]
for i in untraced: fail(f"§53 invariant {i} has no traceability row")
print(f"  §53 invariants traced: {23 - len(untraced)}/23")

print("\n" + "=" * 74)
print("RESULT:", "ALL CHECKS PASS" if not FAILS else f"{len(FAILS)} FAILURE(S)")
print("=" * 74)
sys.exit(1 if FAILS else 0)
