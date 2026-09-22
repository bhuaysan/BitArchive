#!/usr/bin/env python3
"""Structural checks over DATA_MODEL.md and its interface to ARCHITECTURE.md.

One source of truth for keys
----------------------------
This script keeps **no key list of its own**. Every primary key and every unique
constraint is parsed out of `DATA_MODEL.md` (the per-table `**Domain identity.**` /
`**Storage primary key.**` / `**Business uniqueness.**` declarations), and the
summary table in §20.1 is parsed as well and compared against them. An earlier
version carried a hand-maintained `PK` map next to a derived one; the two drifted,
and the stale copy silently accepted a nullable key component.

The parse distinguishes four things the model must not conflate:

    storage PRIMARY KEY          exactly one per table, never partial
    full UNIQUE key              the only kind a foreign key may reference
    partial UNIQUE index         never a primary key, never an FK parent target
    ordinary (non-unique) index  not a key at all

Run: python3 tools/check_data_model.py
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA_PATH = os.path.join(ROOT, 'DATA_MODEL.md')
ARCH_PATH = os.path.join(ROOT, 'ARCHITECTURE.md')

DATA = open(DATA_PATH, encoding='utf-8').read()
ARCH = open(ARCH_PATH, encoding='utf-8').read()

FAILS = []


def fail(msg):
    FAILS.append(msg)
    print("  XX " + msg)


def ok(msg):
    print("  OK " + msg)


def banner(title):
    print("\n" + "=" * 74)
    print(title)
    print("=" * 74)


def norm(text):
    """Whitespace-normalised text, for comparing a declaration with its summary."""
    return re.sub(r'\s+', ' ', text).strip()


# --------------------------------------------------------------------------- #
# 1. The table list comes from the document's own §20.1 checklist.
# --------------------------------------------------------------------------- #
banner("1. §20.1 is the table list; every table declares its identity and keys")

m = re.search(r'### 20\.1 Tables.*?\n(\|.*?)\n\n', DATA, re.S)
if not m:
    fail("§20.1 check-list table not found")
    print("RESULT: cannot continue without §20.1")
    sys.exit(1)

CHECKLIST = {}
for line in m.group(1).split('\n'):
    if not line.startswith('|'):
        continue
    cells = [c.strip() for c in line.strip('|').split('|')]
    if len(cells) != 6 or not cells[0].isdigit():
        continue
    name = re.match(r'`([a-z_]+)`', cells[1])
    if not name:
        continue
    CHECKLIST[name.group(1)] = dict(
        row=int(cells[0]), section=cells[2], domain=cells[3],
        pk=cells[4], uniqueness=cells[5])
print(f"  {len(CHECKLIST)} tables in §20.1")
if len(CHECKLIST) < 37:
    fail(f"§20.1 lists only {len(CHECKLIST)} tables")

# --------------------------------------------------------------------------- #
# 2. Section boundaries, per table.
# --------------------------------------------------------------------------- #
TABLE_MARKER = re.compile(
    r'^(?:#{3,4}\s+(?:\d+\.\d+\s+)?`([a-z_]+)`[^\n]*'   # ### 5.1 `library_sources`
    r'|\*\*`([a-z_]+)`\*\*[^\n]*)$',                     # **`scrape_run_items`**
    re.M)

marks = []
for mm in TABLE_MARKER.finditer(DATA):
    name = mm.group(1) or mm.group(2)
    if name in CHECKLIST:
        marks.append((name, mm.start(), mm.end()))

# A table may be marked more than once (a heading plus a bold re-reference);
# the FIRST marker owns the section, and the section runs to the next marker of
# any known table.
first = {}
for name, start, end in marks:
    first.setdefault(name, (start, end))

ordered = sorted(((v[0], k) for k, v in first.items()))
sections = {}
for i, (start, name) in enumerate(ordered):
    end = ordered[i + 1][0] if i + 1 < len(ordered) else len(DATA)
    sections[name] = DATA[start:end]

missing_sections = [t for t in CHECKLIST if t not in sections]
if missing_sections:
    fail(f"no section found for: {sorted(missing_sections)}")

# --------------------------------------------------------------------------- #
# 3. Columns, from the standard four-column column tables.
# --------------------------------------------------------------------------- #
COL_ROW = re.compile(r'^\| `([a-z_]+)` \| ([^|]+?) \| (yes|no) \|', re.M)

columns = {}      # table -> {column: (type, nullable)}
for name, body in sections.items():
    cols = {}
    for cm in COL_ROW.finditer(body):
        cols[cm.group(1)] = (cm.group(2).strip(), cm.group(3) == 'yes')
    if cols:
        columns[name] = cols
print(f"  columns parsed for {len(columns)} tables")

# --------------------------------------------------------------------------- #
# 4. Declarations: identity, primary key, uniqueness.
# --------------------------------------------------------------------------- #
def decl_chunk(body, marker_end):
    """Text a declaration covers: to the next bold marker, heading or paragraph."""
    rest = body[marker_end:].lstrip('\n')
    stop = re.search(r'\n\*\*[A-Z]|\n#{2,4} ', rest)
    return rest[:stop.start()] if stop else rest


TUPLE = re.compile(r'\(([a-z_,\s]+)\)')
UNIQUE_RE = re.compile(r'UNIQUE\s*\(([a-z_,\s]+)\)(.*?)(?=UNIQUE\s*\(|\Z)', re.S)


def predicate_of(tail):
    """The WHERE predicate of a unique declaration, normalised.

    Handles inline backticked predicates, predicates broken across a line, and
    the SQL-comment-aligned predicates of the §9.3/§9.4 code blocks.
    """
    cut = re.search(r'`|--|\n\s*\n', tail)
    text = tail[:cut.start()] if cut else tail
    text = text.split('\n- ')[0]
    text = re.sub(r'\s+', ' ', text).strip().rstrip('.,;:')
    w = re.search(r'WHERE\s+(.*)$', text, re.I)
    return w.group(1).strip() if w else ''


keys = {}         # table -> {'pk': tuple|'rowid', 'unique': [(tuple, predicate)]}
domains = {}      # table -> declared domain identity text
undeclared = []

for name in sorted(CHECKLIST, key=lambda t: CHECKLIST[t]['row']):
    body = sections.get(name, '')
    cols = columns.get(name, {})

    dm = re.search(r'\*\*Domain identity\.\*\*(.*?)(?=\n\*\*[A-Z]|\Z)', body, re.S)
    pm = re.search(r'\*\*Storage primary key\.\*\*(.*?)(?=\n\*\*[A-Z]|\Z)', body, re.S)
    # `**Primary key.**` is the pre-round-4 marker; it must not come back.
    legacy = re.search(r'\*\*Primary key\.\*\*(.*?)(?=\n\*\*[A-Z]|\Z)', body, re.S)
    um = re.search(r'\*\*Business uniqueness\.\*\*(.*?)(?=\n\*\*[A-Z]|\Z)', body, re.S)

    if legacy:
        fail(f"{name}: uses the retired '**Primary key.**' marker: "
             f"{norm(legacy.group(1))[:90]!r}")
    if not dm:
        fail(f"{name}: no '**Domain identity.**' declaration")
    if not pm:
        fail(f"{name}: no '**Storage primary key.**' declaration")
        undeclared.append(name)
        continue
    if not um:
        fail(f"{name}: no '**Business uniqueness.**' declaration")
        undeclared.append(name)
        continue

    domains[name] = norm(dm.group(1))

    # ---- primary key
    chunk = decl_chunk(body, body.index('**Storage primary key.**') +
                       len('**Storage primary key.**'))
    tuples = [norm(t.group(1)) for t in TUPLE.finditer(chunk)]
    valid = [t for t in tuples if all(c.strip() in cols for c in t.split(','))]
    if valid:
        if tuples and tuples[0] != valid[0]:
            fail(f"{name}: primary key names a column the table does not declare: "
                 f"({tuples[0]})")
        pk = tuple(c.strip() for c in valid[0].split(','))
    elif 'rowid' in chunk:
        pk = 'rowid'                      # FTS5 implicit rowid (§17.4)
    else:
        fail(f"{name}: primary key is not a column tuple and is not FTS5 rowid: "
             f"{norm(chunk)[:90]!r}")
        undeclared.append(name)
        continue
    sentence = re.split(r'(?<=[.;])\s', norm(chunk))[0]
    if 'partial' in sentence.lower():
        fail(f"{name}: the primary key declaration mentions 'partial'; a partial "
             f"index is never a primary key (§3.9)")
    keys[name] = dict(pk=pk, unique=[])

    # ---- uniqueness
    ubody = decl_chunk(body, body.index('**Business uniqueness.**') +
                       len('**Business uniqueness.**'))
    for umatch in UNIQUE_RE.finditer(ubody):
        ucols = tuple(c.strip() for c in norm(umatch.group(1)).split(','))
        pred = predicate_of(umatch.group(2))
        keys[name]['unique'].append((ucols, pred))

declared = [t for t in CHECKLIST if t not in undeclared]
print(f"  {len(declared)} tables declare domain identity, primary key and uniqueness")

# --------------------------------------------------------------------------- #
# 5. Every declared key column exists, and every table has exactly one PK.
# --------------------------------------------------------------------------- #
banner("2. Key columns exist; exactly one storage primary key per table")

for name in sorted(declared, key=lambda t: CHECKLIST[t]['row']):
    cols = columns.get(name, {})
    if not cols:
        if keys[name]['pk'] == 'rowid':
            print(f"  note: {name} is an FTS5 projection (rowid key, no column table)")
        else:
            fail(f"{name}: no column table found, so its keys cannot be verified")
        continue
    pk = keys[name]['pk']
    if pk != 'rowid':
        for c in pk:
            if c not in cols:
                fail(f"{name}: primary key column {c!r} is not a declared column")
    for ucols, _pred in keys[name]['unique']:
        for c in ucols:
            if c not in cols:
                fail(f"{name}: unique key column {c!r} is not a declared column")

row_id_tables = [t for t in declared if keys[t]['pk'] == ('row_id',)]
print(f"  row_id storage key: {', '.join(sorted(row_id_tables))}")

# --------------------------------------------------------------------------- #
# 6. §20.1 must agree with the per-table declarations.
# --------------------------------------------------------------------------- #
banner("3. §20.1 agrees with every per-table declaration")

for name in sorted(declared, key=lambda t: CHECKLIST[t]['row']):
    row = CHECKLIST[name]
    pk_txt = norm(row['pk'])
    pk = keys[name]['pk']
    pk_plain = norm(pk_txt.replace('`', ''))
    if pk == 'rowid':
        if 'rowid' not in pk_plain:
            fail(f"§20.1 row {row['row']} ({name}): storage key does not name rowid")
    else:
        expected = '(' + ', '.join(pk) + ')'
        if pk_plain != expected and not pk_plain.startswith(expected + ' '):
            fail(f"§20.1 row {row['row']} ({name}): storage key {pk_plain!r} does "
                 f"not name the declared primary key {expected!r}")
    section_domain = domains.get(name, '')
    if norm(row['domain']) == 'none':
        if not section_domain.lower().startswith('none'):
            fail(f"§20.1 row {row['row']} ({name}): §20.1 says domain identity 'none' "
                 f"but the section declares {section_domain[:50]!r}")
    elif norm(row['domain']) not in section_domain:
        fail(f"§20.1 row {row['row']} ({name}): §20.1 names domain identity "
             f"{norm(row['domain'])!r}, which the section does not declare "
             f"({section_domain[:50]!r})")
    want = {(u, p) for u, p in keys[name]['unique']}
    have = set()
    for umatch in UNIQUE_RE.finditer(row['uniqueness']):
        ucols = tuple(c.strip() for c in norm(umatch.group(1)).split(','))
        have.add((ucols, predicate_of(umatch.group(2))))
    if want != have:
        fail(f"§20.1 row {row['row']} ({name}): business uniqueness differs from "
             f"the section.\n      section: {sorted(want)}\n      §20.1   : {sorted(have)}")
print("  storage keys and unique rules compared row by row")

# §20.1 row 37 is the FTS5 projection, whose section is §17.4.
if 'rowid' not in norm(CHECKLIST['search_index']['pk']):
    fail("§20.1: search_index has no rowid storage key")

# --------------------------------------------------------------------------- #
# 7. No key component is nullable without a predicate that pins it.
# --------------------------------------------------------------------------- #
banner("4. No nullable key component without a pinning predicate (§3.5, §3.8)")

for name in sorted(declared, key=lambda t: CHECKLIST[t]['row']):
    cols = columns.get(name, {})
    body = sections[name]
    pk = keys[name]['pk']
    if pk != 'rowid':
        nullable = [c for c in pk if cols.get(c, (None, False))[1]]
        if nullable:
            fail(f"{name}: primary key {pk} contains nullable column(s) {nullable}; "
                 f"a nullable key component disables the key in SQLite")
    for ucols, pred in keys[name]['unique']:
        nullable = [c for c in ucols if cols.get(c, (None, False))[1]]
        if not nullable:
            continue
        if not pred:
            fail(f"{name}: unique key {ucols} contains nullable column(s) {nullable} "
                 f"and has no partial predicate")
            continue
        for c in nullable:
            pinned = (f"{c} IS NOT NULL" in body) or (c in pred and 'IS ' in pred.upper())
            if not pinned:
                fail(f"{name}: unique key {ucols} has predicate {pred!r}, which does "
                     f"not prove {c} is non-NULL for the rows it covers")
        print(f"  {name:28} {str(ucols):58} pinned by {pred!r}")

audit = re.search(r'### 3\.8 .*?(?=\n---\n)', DATA, re.S)
if not audit:
    fail("§3.8 (nullable-key audit) is missing")
else:
    absent = [t for t in declared if f"`{t}`" not in audit.group(0)]
    if absent:
        fail(f"§3.8 does not classify: {sorted(absent)}")
    else:
        print(f"  §3.8 classifies all {len(declared)} tables")
if '### 3.9 One real primary key per table' not in DATA:
    fail("§3.9 (one real primary key per table) is missing")

# --------------------------------------------------------------------------- #
# 8. Foreign keys: child side, parent side, and parent-key eligibility.
# --------------------------------------------------------------------------- #
banner("5. Foreign keys: both sides, and parent keys SQLite will accept")

HEADINGS = re.compile(
    r'^(?:#{3,4}\s+(?:\d+\.\d+\s+)?`([a-z_]+)`[^\n]*'
    r'|\*\*`([a-z_]+)`\*\*[^\n]*)$', re.M)


def table_at(pos):
    owners = [mm for mm in HEADINGS.finditer(DATA[:pos])
              if (mm.group(1) or mm.group(2)) in CHECKLIST]
    if not owners:
        return None
    return owners[-1].group(1) or owners[-1].group(2)


COMPOSITE = (r'`\(([a-z_,\s]+)\)\s*\u2192\s*([a-z_]+)\(([a-z_,\s]+)\)`\s*[\u2014-]\s*'
             r'`?([A-Z][A-Z ]*?)`?(?=[,.;\s]|$)')
SIMPLE = r'`([a-z_]+)\s*\u2192\s*([a-z_]+)\.([a-z_]+)`'
# A single child column referencing a composite parent key:
#   `content_id -> content_fingerprints(algorithm, digest)`
TO_COMPOSITE = r'`([a-z_]+)\s*\u2192\s*([a-z_]+)\(([a-z_,\s]+)\)`'

ALL_REFS = [mm for mm in re.finditer(
    TO_COMPOSITE + '|' + SIMPLE + '|' + COMPOSITE, DATA)]


def action_after(pos):
    """The delete behaviour of the reference ending at `pos`: the first action
    token before the next reference, or RESTRICT (SQLite's default)."""
    end = min([mm.start() for mm in ALL_REFS if mm.start() >= pos] or [pos + 60])
    context = DATA[pos:end]
    hits = [(context.index(a), a) for a in ('CASCADE', 'SET NULL', 'RESTRICT')
            if a in context]
    return min(hits)[1] if hits else 'RESTRICT'


fks, spans = [], []
for fm in re.finditer(TO_COMPOSITE, DATA):
    if any(a <= fm.start() < b for a, b in spans):
        continue
    spans.append((fm.start(), fm.end()))
    fks.append(dict(child=table_at(fm.start()), child_cols=[fm.group(1)],
                    parent=fm.group(2),
                    parent_cols=[c.strip() for c in fm.group(3).split(',')],
                    action=action_after(fm.end()),
                    line=DATA[:fm.start()].count('\n') + 1))
for fm in re.finditer(COMPOSITE, DATA):
    spans.append((fm.start(), fm.end()))
    fks.append(dict(child=table_at(fm.start()),
                    child_cols=[c.strip() for c in fm.group(1).split(',')],
                    parent=fm.group(2),
                    parent_cols=[c.strip() for c in fm.group(3).split(',')],
                    action=fm.group(4).strip(),
                    line=DATA[:fm.start()].count('\n') + 1))
for fm in re.finditer(SIMPLE, DATA):
    if any(a <= fm.start() < b for a, b in spans):
        continue
    action = action_after(fm.end())
    fks.append(dict(child=table_at(fm.start()), child_cols=[fm.group(1)],
                    parent=fm.group(2), parent_cols=[fm.group(3)],
                    action=action,
                    line=DATA[:fm.start()].count('\n') + 1))

eligible_targets = {}
for name in declared:
    pk = keys[name]['pk']
    targets = set()
    if pk != 'rowid':
        targets.add(pk)
    for ucols, pred in keys[name]['unique']:
        if not pred:
            targets.add(ucols)
    eligible_targets[name] = targets

partial_only = {}
for name in declared:
    partial = {ucols for ucols, pred in keys[name]['unique'] if pred}
    full = eligible_targets[name]
    partial_only[name] = partial - full

valid_fks = 0
for fk in fks:
    problems = []
    child, parent = fk['child'], fk['parent']
    if child not in CHECKLIST:
        problems.append(f"child table {child!r} is not a §20.1 table")
    else:
        ccols = columns.get(child, {})
        for c in fk['child_cols']:
            if c not in ccols:
                problems.append(f"child column {child}.{c} is not declared")
    if parent not in CHECKLIST:
        problems.append(f"parent table {parent!r} is not a §20.1 table")
    else:
        pcols = columns.get(parent, {})
        for c in fk['parent_cols']:
            if c not in pcols:
                problems.append(f"parent column {parent}.{c} is not declared")
        want = tuple(fk['parent_cols'])
        if want not in eligible_targets[parent]:
            if want in partial_only[parent]:
                problems.append(
                    f"parent key {want} of {parent} is a PARTIAL unique index; "
                    f"SQLite rejects it as a foreign-key target (foreign key mismatch)")
            else:
                problems.append(
                    f"parent key {want} is neither the primary key nor a full "
                    f"UNIQUE key of {parent} (has {sorted(eligible_targets[parent])})")
        # SET NULL requires nullable child columns.
        if fk['action'] == 'SET NULL':
            ccols = columns.get(child, {})
            for c in fk['child_cols']:
                if c in ccols and not ccols[c][1]:
                    problems.append(
                        f"delete behaviour SET NULL on {child}.{c}, which is NOT NULL")
    if problems:
        for p in problems:
            fail(f"L{fk['line']}: " + p)
    else:
        valid_fks += 1
print(f"  foreign keys parsed: {len(fks)}   valid: {valid_fks}")
if not fks:
    fail("no foreign keys parsed at all; the parser is probably broken")

# --------------------------------------------------------------------------- #
# 9. Locale: no sentinel, and the two exhaustive partial indexes.
# --------------------------------------------------------------------------- #
banner("6. A locale is never encoded by a sentinel (§3.5, §9.3, §9.4)")

if 'locale_key' in DATA:
    fail("DATA_MODEL.md still mentions `locale_key`, the removed storage sentinel")
else:
    ok("no `locale_key` column anywhere")
if 'COALESCE(locale' in DATA:
    fail("DATA_MODEL.md still maps NULL to a tag with COALESCE(locale, …)")
else:
    ok("no COALESCE(locale, …) normalisation")

for t, pk in (('provider_values', '(provider_id, subject_kind, subject_id, field_key, value_index)'),
              ('manual_overrides', '(subject_kind, subject_id, field_key)')):
    rules = {tuple(u): p for u, p in keys.get(t, {}).get('unique', [])}
    want_null = [u for u, p in rules.items() if p == 'locale IS NULL']
    want_tag = [u for u, p in rules.items() if p == 'locale IS NOT NULL']
    if not want_null or not want_tag:
        fail(f"{t}: expected an exhaustive pair of partial unique indexes "
             f"(locale IS NULL / locale IS NOT NULL), found {sorted(rules.items())}")
    else:
        ok(f"{t}: locale split into IS NULL / IS NOT NULL partial indexes")
    if 'locale' not in columns.get(t, {}):
        fail(f"{t}: no `locale` column")
    elif columns[t]['locale'][1] is False:
        fail(f"{t}: `locale` is NOT NULL, so language-neutral rows are unrepresentable")

if "und" in DATA and not re.search(r'never.*`und`|`und`.*never', DATA, re.I):
    fail("DATA_MODEL.md mentions `und` without stating that it is a real tag, "
         "not a marker")
else:
    ok("`und` is documented as a real BCP-47 tag")
if 'canonical case' not in DATA or 'COLLATE NOCASE' not in DATA:
    fail("the locale canonicalisation rule (canonical case + COLLATE NOCASE) is absent")
else:
    ok("locale canonicalisation is stated and enforced by the indexes")

# --------------------------------------------------------------------------- #
# 10. content_locations: both variants pinned in both directions.
# --------------------------------------------------------------------------- #
banner("7. content_locations matches ContentLocation (both variants, both ways)")

loc = sections.get('content_locations', '')
need = [
    ("File requires source_id", "WHEN 'File'         THEN source_id IS NOT NULL"),
    ("File requires relative_path", "AND relative_path IS NOT NULL"),
    ("File forbids archive_content_id", "AND archive_content_id IS NULL"),
    ("File forbids archive_entry_path", "AND archive_entry_path IS NULL"),
    ("ArchiveEntry forbids source_id", "WHEN 'ArchiveEntry' THEN source_id IS NULL"),
    ("ArchiveEntry forbids relative_path", "AND relative_path IS NULL"),
    ("ArchiveEntry requires archive_content_id", "AND archive_content_id IS NOT NULL"),
    ("ArchiveEntry requires archive_entry_path", "AND archive_entry_path IS NOT NULL"),
]
for label, needle in need:
    if needle not in loc:
        fail(f"content_locations CASE check: missing {label}")
    else:
        print(f"  OK {label}")
if 'archive_content_id' in columns.get('contents', {}):
    fail("contents declares archive_content_id; the container link belongs to the "
         "location (§5.2)")
else:
    ok("contents owns no archive_content_id")
if 'ContentLocation' not in DATA:
    fail("§5.2 does not reference ARCHITECTURE.md's ContentLocation")

# --------------------------------------------------------------------------- #
# 11. Archive entries and the architecture agree.
# --------------------------------------------------------------------------- #
banner("8. ArchiveEntry: DATA_MODEL.md and ARCHITECTURE.md say the same thing")

lc = re.search(r'### 14\.2 LaunchContent(.*?)(?=\n### 14\.3)', ARCH, re.S)
if not lc:
    fail("ARCHITECTURE.md §14.2 LaunchContent not found")
else:
    block = lc.group(1)
    for needle in ('archive: ContentId', 'content: ContentId'):
        if needle not in block:
            fail(f"ARCHITECTURE.md §14.2 LaunchContent lacks {needle!r}")
        else:
            ok(f"ARCHITECTURE.md §14.2: {needle}")
    if 'ExistingPlaylist' not in block or 'ManagedPlaylist' not in block:
        fail("ARCHITECTURE.md §14.2 lost a LaunchContent variant")

for line_no, line in enumerate(ARCH.split('\n'), 1):
    if 'ArchiveEntryId' in line and not re.search(
            r'\b(kein|keine|no|not|never|without)\b', line, re.I):
        fail(f"ARCHITECTURE.md L{line_no}: states an ArchiveEntryId identity that "
             f"DATA_MODEL.md does not have: {line.strip()[:90]!r}")
print("  no un-negated ArchiveEntryId in ARCHITECTURE.md")
for needle in ('no `ArchiveEntryId`', 'archive_entry_path'):
    if needle not in DATA:
        fail(f"DATA_MODEL.md does not state {needle!r}")

# --------------------------------------------------------------------------- #
# 12. Library rebuild: one meaning, everywhere.
# --------------------------------------------------------------------------- #
banner("9. Library rebuild means the same thing in every section (§19.2, §19.3)")

DESTRUCTIVE = [
    (r'forget everything', "calls a library rebuild 'forget everything'"),
    (r'rebuild[^.]{0,80}\b(?:deletes?|clears?|removes?)\b[^.]{0,80}'
     r'\b(?:games|releases|contents)\b',
     "says a library rebuild deletes games/releases/contents"),
    (r'\b(?:games|releases|contents)\b[^.]{0,60}\b(?:are|is)\s+'
     r'(?:deleted|cleared|removed|reset)\b[^.]{0,40}rebuild',
     "says games/releases/contents are deleted by a rebuild"),
    (r'resets? the index\s*\u2014\s*contents', "lists contents as reset by a rebuild"),
]
# A sentence may mention a rebuild and an identity table and still be correct: it
# may be stating what the rebuild KEEPS ("clears the index but keeps the
# content"), or stating the rule that forbids the wrong wording. Only an
# unqualified destructive claim is a defect.
ALLOWED_IN_SENTENCE = (r'\b(?:keeps?|kept|preserv\w*|retains?|retained|survives?|'
                       r'stays?|remains?|stable)\b'
                       r'|\b(?:not|never|no|nothing|neither|without|cannot)\b')
SCAN_SECTIONS = [sections.get(t, '') for t in
                 ('library_sources', 'content_locations', 'games', 'releases',
                  'contents', 'content_fingerprints', 'media_assets',
                  'provider_values', 'manual_overrides')]
rebuild_block = re.search(r'#### Library rebuild(.*?)#### Full reset', DATA, re.S)
if not rebuild_block:
    fail("§19.3 'Library rebuild' block not found")
else:
    for name, start in [('§19.2 section', DATA.index('### 19.2 ')),
                        ('§19.7 section', DATA.index('### 19.7 '))]:
        end = DATA.index('###', start + 5)
        SCAN_SECTIONS.append(DATA[start:end])
    for section in SCAN_SECTIONS:
        for sentence in re.split(r'(?<=[.!?])\s+', section):
            flat = re.sub(r'\s+', ' ', sentence)
            for pattern, why in DESTRUCTIVE:
                hit = re.search(pattern, flat, re.I | re.S)
                if not hit:
                    continue
                if re.search(ALLOWED_IN_SENTENCE, flat, re.I):
                    continue        # a keep-statement or the rule that forbids it
                fail(f"{why}: …{flat[max(0, hit.start() - 60):hit.end() + 60]}…")
    if 'forget' in rebuild_block.group(1):
        fail("§19.3 itself mentions forgetting")

keep_table = rebuild_block.group(1)
for t in ('games', 'releases', 'contents'):
    row = re.search(r'\| `games`, `releases`, `contents` \|(.*?)\|', keep_table)
    if not row:
        fail(f"§19.3 rebuild table has no row for {t}")
    elif '**kept**' not in row.group(1):
        fail(f"§19.3 rebuild table does not keep {t}")
if '**kept**' in keep_table:
    ok("§19.3 keeps every identity row")
if 'not a "forget everything"\noperation' not in DATA and \
   'not a "forget everything"' not in DATA:
    fail("§19.2 does not state that a rebuild is not a 'forget everything' operation")
else:
    ok("§19.2 states the rebuild semantics from §19.3")

# Source removal after the ArchiveEntry fix.
s192 = DATA[DATA.index('### 19.2 '):DATA.index('### 19.3 ')]
for needle, label in (("location_kind = 'File' AND source_id = X",
                       "source removal deletes File locations only"),
                      ('MUST NOT destroy an `ArchiveEntry`', 'entry survives'),
                      ('location_kind = \'ArchiveEntry\'',
                       "source removal leaves ArchiveEntry locations alone")):
    if needle not in s192:
        fail(f"§19.2: missing rule — {label}")
    else:
        ok(f"§19.2: {label}")

# --------------------------------------------------------------------------- #
# 13. Save states: technical addressing, no sentinel.
# --------------------------------------------------------------------------- #
banner("10. Save states: technical addressing without a sentinel (§15.1)")

ss = sections.get('save_states', '')


def only_in_removed_context(body, needle):
    """True when every occurrence of `needle` sits in a sentence that says the
    construct was removed or does not exist."""
    for sentence in re.split(r'(?<=[.!?:])\s+', re.sub(r'\s+', ' ', body)):
        if needle not in sentence:
            continue
        if not re.search(r'\b(?:removed|gone|no longer|absent|deleted|not|never|'
                         r'no|without|earlier revision|previous revision)\b',
                         sentence, re.I):
            return False
    return True


if 'empty-string sentinel' in ss and not only_in_removed_context(
        ss, 'empty-string sentinel'):
    fail("§15.1 reintroduced an empty-string slot sentinel")
elif "slot = ''" in ss:
    fail("§15.1 assigns meaning to an empty-string slot value")
else:
    ok("no empty-string slot sentinel")
if 'state_name' in ss and not only_in_removed_context(ss, 'state_name'):
    fail("§15.1 declares `state_name`, for which no technical RetroArch property "
         "is established")
else:
    ok("no `state_name` column")
slot_row = re.search(r'^\| `slot` \| ([^|]+?) \| (yes|no) \|', ss, re.M)
if not slot_row:
    fail("§15.1 declares no `slot` column")
else:
    if 'INTEGER' not in slot_row.group(1):
        fail(f"§15.1 slot is {slot_row.group(1).strip()}; a RetroArch slot is a "
             f"number, not a name")
    else:
        ok(f"slot is {slot_row.group(1).strip()}, Null={slot_row.group(2)}")
    if slot_row.group(2) == 'no':
        fail("§15.1 slot is NOT NULL, which forces a sentinel for the state that "
             "is not addressed by a numbered slot")
if 'slot' in ' '.join(keys.get('save_states', {}).get('pk', ())) or \
   any('slot' in u for u, _ in keys.get('save_states', {}).get('unique', [])):
    fail("§15.1 uses `slot` as a key component again")
else:
    ok("slot is an attribute, not a key component")
phys = [u for u, p in keys.get('save_states', {}).get('unique', [])
        if u == ('file_relative_path',)]
if not phys:
    fail("§15.1 does not declare UNIQUE (file_relative_path), so discovery is not "
         "idempotent")
else:
    ok("UNIQUE (file_relative_path) makes discovery idempotent")

# --------------------------------------------------------------------------- #
# 14. Polymorphic references have a write-path rule and a test.
# --------------------------------------------------------------------------- #
banner("11. Polymorphic subject references are defined, not implied")
for t in ('provider_values', 'manual_overrides', 'media_asset_references'):
    body = sections.get(t, '')
    checks = {
        'states the reference is polymorphic': 'polymorphic' in body.lower(),
        'names the subject kinds': 'Game' in body and 'Release' in body,
        'states it cannot be a foreign key':
            'not a foreign key' in body.lower() or
            'cannot be a declarative foreign key' in body.lower(),
        'names the write path': 'write path' in body.lower(),
        'names the integrity test': '§20.3' in body,
    }
    for label, present in checks.items():
        if not present:
            fail(f"{t}: polymorphic reference — does not {label}")
    if all(checks.values()):
        print(f"  OK {t}: polymorphic, kind-bounded, write-path rule + test named")

# --------------------------------------------------------------------------- #
# 15. Derived-data checks that survived the rewrite.
# --------------------------------------------------------------------------- #
banner("12. Rebuildable ownership, lifetimes and run retention")

REBUILDABLE = {'managed_components', 'component_index_state', 'firmware_entries',
               'firmware_index_state', 'search_index', 'content_derivations',
               'content_derivation_members'}
bad = [fk for fk in fks if fk['parent'] in REBUILDABLE]
for fk in bad:
    fail(f"L{fk['line']}: {fk['child']} references rebuildable {fk['parent']}")
print(f"  rebuildable: {sorted(REBUILDABLE)}")
print("  inbound references: none" if not bad else "")

LIFE = ['Persistent', 'Rebuildable', 'SessionScoped', 'Temporary']
for line_no, line in enumerate(DATA.split('\n'), 1):
    if '**Lifecycle.**' not in line:
        continue
    frag = line.split('**Lifecycle.**', 1)[1].strip()
    frag = re.split(r'(?<=[.;])\s', frag)[0]
    labels = set(re.findall(r'\*\*(' + '|'.join(LIFE) + r')\*\*', frag)) | \
             set(re.findall(r'(?<!\*)\b(' + '|'.join(LIFE) + r')\b(?!\*)', frag))
    if len(labels) > 1:
        fail(f"L{line_no}: multiple lifetimes {sorted(labels)}: {frag[:70]}")
ok("every declared lifecycle names exactly one lifetime")

print("  run references and their declared delete behaviour:")
for fk in fks:
    if fk['parent'] not in ('scan_runs', 'scrape_runs'):
        continue
    print(f"    {fk['child']}.{fk['child_cols'][0]} -> {fk['parent']}: {fk['action']}")
    if fk['action'] == 'RESTRICT':
        fail(f"L{fk['line']}: RESTRICT on a prunable run reference blocks run "
             f"retention (§19.7)")

banner("13. Derivations, recognition evidence and rebuild semantics")
need = [
    ("PK admits N members", "(content_id, kind, source_content_id)" in DATA),
    ("order unique", "UNIQUE (content_id, kind, member_index)" in DATA),
    ("member_count >= 2", "member_count >= 2" in DATA),
    ("cross-row invariant stated",
     "MUST equal the number of `content_derivation_members` rows" in DATA),
]
for label, present in need:
    print(f"  {'OK' if present else 'XX'} {label}")
    if not present:
        fail("derivation: " + label)

chain = [
    ("retained identity", "retained:  ContentId C" in DATA),
    ("retained evidence", "canonical Payload fingerprint" in DATA),
    ("rescan match by digest", "fingerprint_kind='Payload'" in DATA),
    ("explicitly not by path", "path-free" in DATA),
    ("identity stays stable", "GameId / ReleaseId / ContentId stay stable" in DATA),
    ("lookup is a function", "returns one row or none" in DATA),
]
for label, present in chain:
    print(f"  {'OK' if present else 'XX'} {label}")
    if not present:
        fail("rebuild path: " + label)

block = re.search(r'#### Library rebuild.*?(?=#### Full reset)', DATA, re.S).group(0)
rows = []
for line in block.split('\n'):
    if not line.startswith('|'):
        continue
    cells = [c.strip() for c in line.strip('|').split('|')]
    if len(cells) < 3:
        continue
    value = ('cleared' if cells[1].startswith('**cleared**')
             else 'kept' if cells[1].startswith('**kept**') else None)
    if value:
        rows.append((cells[0], value))
cleared = {n for n, v in rows if v == 'cleared'}
kept = {n for n, v in rows if v == 'kept'}
print(f"  rebuild table rows: {len(rows)}; cleared {len(cleared)}; kept {len(kept)}")
for t in ('games', 'releases', 'contents', 'content_fingerprints',
          'manual_overrides', 'sessions', 'save_states', 'media_assets',
          'media_asset_references'):
    if not any(t in k for k in kept):
        fail(f"library rebuild does not explicitly keep {t}")
for t in ('content_locations', 'scan_runs', 'scan_run_issues', 'provider_values',
          'search_index'):
    if not any(t in c for c in cleared):
        fail(f"library rebuild does not explicitly clear {t}")

# --------------------------------------------------------------------------- #
# 16. Full-reset order, parsed from the document and checked against the FK graph.
# --------------------------------------------------------------------------- #
banner("14. The documented full-reset order is executable")

edges = {}
for fk in fks:
    if fk['child'] in CHECKLIST and fk['parent'] in CHECKLIST and fk['action'] == 'RESTRICT':
        edges.setdefault(fk['child'], set()).add(fk['parent'])

block = re.search(r'```text\n 1\. DELETE core_option_overrides.*?```', DATA, re.S)
if not block:
    fail("the full-reset plan block was not found in §19.3")
else:
    documented = [t for raw in block.group(0).split('\n')
                  for t in re.findall(r'DELETE\s+([a-z_]+)', raw)]
    pos = {}
    for i, t in enumerate(documented):
        pos.setdefault(t, i)
    missing = [t for t in edges if t not in pos]
    if missing:
        fail(f"the reset order omits tables with RESTRICT foreign keys: {missing}")
    violations = [(c, p) for c, parents in edges.items() if c in pos
                  for p in parents if p in pos and pos[p] < pos[c] and p != c]
    for c, p in violations:
        fail(f"the documented reset order deletes parent {p} before its RESTRICT "
             f"child {c}")
    if not violations:
        print(f"  parsed {len(pos)} tables; every RESTRICT child precedes its parent")
        colour, cycles = {}, []

        def dfs(u, stack):
            colour[u] = 1
            stack.append(u)
            for v in sorted(edges.get(u, ())):
                if colour.get(v, 0) == 1:
                    cycles.append(stack[stack.index(v):] + [v])
                elif colour.get(v, 0) == 0:
                    dfs(v, stack)
            stack.pop()
            colour[u] = 2

        for node in sorted(edges):
            if colour.get(node, 0) == 0:
                dfs(node, [])
        seen, unique = set(), []
        for cycle in cycles:
            k = tuple(sorted(set(cycle)))
            if k not in seen:
                seen.add(k)
                unique.append(cycle)
        if unique:
            fail(f"the model contains a RESTRICT cycle {unique}")
        else:
            ok("no RESTRICT cycle, so no pointer has to be pre-cleared")
    if re.search(r'\bUPDATE\s+(games|sessions)', block.group(0)):
        fail("the reset plan pre-clears pointers the order does not need")

# --------------------------------------------------------------------------- #
# 17. Markdown structure and traceability to ARCHITECTURE.md.
# --------------------------------------------------------------------------- #
banner("15. Markdown structure and ARCHITECTURE.md traceability")

fences = DATA.count('\n```')
if fences % 2:
    fail(f"unbalanced code fences in DATA_MODEL.md ({fences} fence lines)")
else:
    ok(f"{fences // 2} code blocks, fences balanced")
for section in ('## Appendix A', '## Appendix B'):
    if section not in DATA:
        fail(f"{section} is missing")

def arch_section(number):
    mm = re.search(r'^## ' + str(number) + r'\..*?(?=\n---\n)', ARCH, re.S | re.M)
    return mm.group(0) if mm else ''


sec54 = arch_section(54)
areas = re.findall(r'^- (.+)$', sec54, re.M)
appendix_a = DATA[DATA.index('## Appendix A'):DATA.index('## Appendix B')]
missing_areas = [a for a in areas if f'| {a} ' not in appendix_a
                 and f'| {a} |' not in appendix_a]
if not areas:
    fail("ARCHITECTURE.md §54 lists no areas to trace")
elif missing_areas:
    fail(f"Appendix A does not trace §54 areas: {missing_areas}")
else:
    ok(f"Appendix A traces all {len(areas)} §54 areas")

sec53 = arch_section(53)
invariants = re.findall(r'^(\d+)\. ', sec53, re.M)
appendix_b = DATA[DATA.index('## Appendix B'):]
missing_inv = [n for n in invariants
               if not re.search(r'^\| ' + n + r'\. ', appendix_b, re.M)]
if not invariants:
    fail("ARCHITECTURE.md §53 lists no invariants to trace")
elif missing_inv:
    fail(f"Appendix B does not trace §53 invariants: {missing_inv}")
else:
    ok(f"Appendix B traces all {len(invariants)} §53 invariants")

print("\n" + "=" * 74)
print("RESULT:", "ALL CHECKS PASS" if not FAILS else f"{len(FAILS)} FAILURE(S)")
print("=" * 74)
sys.exit(1 if FAILS else 0)
