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

Two binding statements are checked for internal consistency rather than for shape,
because a section may carry both halves of a contradiction: the source-removal rule
of §19.2 (it deletes the removed source's `File` locations and no `ArchiveEntry`
location, so it may not also forbid deleting `content_locations` as a whole) and the
launch-resolution rule of §5.4 (the container-level `ArchiveEntry` key admits 0 or 1
row for `(archive, content)`, so no passage may order several entry paths or present
them as a normal state). The checks for both are deliberately narrow: this is not a
natural-language prover, and it must not fire on the document explaining the defect
it forbids.

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


# A claim may be legitimate when the same passage says the construct is gone,
# forbidden or impossible. Used by the checks that reject re-introduced claims.
NEGATED = re.compile(
    r"\b(?:not|never|no|neither|without|nicht|removed|removes|dropped|drop|"
    r"replaced|rather than|instead of|forbids?|refuses?|weaker|impossible|absent|"
    r"returned to|reintroduc\w+|none|nothing|forbidden|refused|no longer)\b",
    re.I)


def paragraphs(text):
    """Blocks a claim has to be judged within.

    A claim and the sentence that rules it out often sit in the same paragraph, and
    hard-wrapping must not split them. Markdown block structure (code fences, list
    items, table rows, block-quote lines) still separates blocks, so a mutation in
    one block cannot be excused by a negation in another.
    """
    out, buf, fence = [], [], False        # buf = [kind, raw_line, continuation…]
    for raw in text.split('\n'):
        line = raw.rstrip()
        stripped = line.strip()
        if stripped.startswith('```'):
            fence = not fence
            if buf:
                out.append(' '.join(buf[1:]))
                buf = []
            continue
        if fence:
            continue
        marker = re.match(r'^(?:[-*+]\s|>|#{1,6}\s|\d+\.\s|\|)', stripped)
        if marker:
            kind = 'li' if marker.group(0)[0] in '-*+0123456789' else marker.group(0)[0]
            body = re.sub(r'^(?:[-*+]\s|>\s?|#{1,6}\s|\d+\.\s)', '', stripped)
            grouped = bool(buf) and buf[0] == kind
            if not grouped and buf:
                out.append(' '.join(buf[1:]))
                buf = []
            if not buf:
                buf = [kind, body]
            else:
                buf.append(body)
            continue
        if not stripped or not buf or buf[0] not in ('>', 'li'):
            if buf:
                out.append(' '.join(buf[1:]))
                buf = []
            if stripped:
                buf = ['p', stripped]
            continue
        buf.append(stripped)        # a wrapped line of the same block
    if buf:
        out.append(' '.join(buf[1:]))
    return [p for p in out if p.strip()]


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

# Source removal after the ArchiveEntry fix. The detailed form of this rule is
# checked in section 14d; here the section only has to carry the same statements.
s192 = DATA[DATA.index('### 19.2 '):DATA.index('### 19.3 ')]
for needle, label in (("location_kind = 'File' AND source_id = X",
                       "source removal deletes File locations only"),
                      ('MUST NOT destroy an `ArchiveEntry`', 'entry survives'),
                      ('no `ArchiveEntry` location, ever',
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
# 16b. The ArchiveEntry location key is the container-level product rule.
# --------------------------------------------------------------------------- #
banner("14b. One imported playable entry per archive is a database constraint")

# `PRODUCT.md` §10 supports a ZIP only when it holds exactly one unambiguously
# playable content, and `DATA_MODEL.md` §6.3 states "at most one playable entry is
# imported per archive". The constraint that protects that rule is the
# container-level key, not an entry-path key: a key over
# (archive_content_id, archive_entry_path) accepts two playable entry rows for one
# archive, and then `LaunchContent::ArchiveEntry { archive, content }` would have
# two answers.
ARCHIVE_KEY = ('archive_content_id',)
ARCHIVE_PRED = "location_kind = 'ArchiveEntry'"
FILE_KEY = ('source_id', 'relative_path')
FILE_PRED = "location_kind = 'File'"

loc_keys = keys.get('content_locations', {}).get('unique', [])
loc_pairs = {(u, p) for u, p in loc_keys}

# §3.8 must classify the same two keys: the audit table lists them as `U*` tuples,
# so a mutable constraint surfaces there even when §5.2 is left alone.
audit_row = re.search(r'^\| `content_locations` \|(.*?)\|\s*$', DATA, re.M)
audit_keys = re.findall(r'U\*? \(([a-z_,\s]+)\)\s*WHERE\s*([^|;]+)', audit_row.group(1)) \
    if audit_row else []
audit_pairs = {(tuple(c.strip() for c in u.split(',')),
                norm(p).rstrip('| ').strip().rstrip('`').strip())
               for u, p in audit_keys}
if audit_pairs and audit_pairs != loc_pairs:
    fail(f"§3.8 does not classify the location keys of §5.2.\n      §3.8: "
         f"{sorted(audit_pairs)}\n      §5.2: {sorted(loc_pairs)}")
elif not audit_pairs:
    fail("§3.8 does not list the content_locations keys as `U* (…) WHERE …`")
else:
    ok("§3.8 classifies exactly the two location keys of §5.2")

if (FILE_KEY, FILE_PRED) not in loc_pairs:
    fail(f"§5.2: the File location key {FILE_KEY} WHERE {FILE_PRED} is missing "
         f"(found {sorted(loc_pairs)})")
else:
    ok("§5.2 File location key is UNIQUE (source_id, relative_path)")
if (ARCHIVE_KEY, ARCHIVE_PRED) not in loc_pairs:
    fail(f"§5.2: the ArchiveEntry location key is not "
         f"UNIQUE (archive_content_id) WHERE {ARCHIVE_PRED} (found "
         f"{sorted(loc_pairs)}). A key over (archive_content_id, "
         f"archive_entry_path) admits several playable entries per archive, so "
         f"LaunchContent::ArchiveEntry {{ archive, content }} could resolve to 2 "
         f"rows.")
else:
    ok("§5.2 ArchiveEntry key is UNIQUE (archive_content_id) "
       "WHERE location_kind = 'ArchiveEntry'")

def claims_entry_path_key(sentence):
    """True when the sentence puts archive_entry_path into a key.

    The prose may name the entry path freely — as the technical attribute of the
    entry row, or as the thing the *container* key replaces. Only a statement that a
    key contains it is a regression.
    """
    if re.search(r'UNIQUE\s*\([^)]*archive_entry_path', sentence, re.I):
        return True
    if re.search(r'archive_entry_path\s*[`\'"]?\s*(?:is|as)\s+[^.;]{0,40}'
                 r'key component', sentence, re.I):
        if NEGATED.search(sentence):
            return False
    return bool(re.search(r'\bkeys?\b[^.;]{0,60}archive_entry_path', sentence, re.I) or
                re.search(r'archive_entry_path[^.;]{0,60}\bkeys?\b', sentence, re.I) or
                re.search(r'archive_entry_path[^.;]{0,60}key component',
                          sentence, re.I))


def negation_context(flat, start, end):
    """The text that may rule a claim out: the paragraph plus the step before it."""
    before = flat.rfind('\n\n', 0, start)
    after = flat.find('\n\n', end)
    return flat[(before + 2 if before != -1 else 0):(after if after != -1 else len(flat))]


def first_key_sentence(text):
    """The passage that first *claims* a key over archive_entry_path, if any.

    Naming the entry path is not the defect — including it in a key is. The claim
    has to say so (`UNIQUE (…)`, or `key` with `over`/`in`/`component`), which is
    what the round-4 key did and what the round-5 rule removed.
    """
    flat = re.sub(r'\s+', ' ', text)
    for mm in re.finditer(r'archive_entry_path', flat):
        start, end = mm.start(), mm.end()
        window = flat[max(0, start - 120):end + 120]
        if not claims_entry_path_key(window):
            continue
        # A key over (archive_content_id) is the replacement, not the defect.
        if re.search(r'UNIQUE\s*\(\s*archive_content_id\s*\)', window, re.I):
            continue
        if NEGATED.search(negation_context(flat, start, end)) or \
           NEGATED.search(window):
            continue
        return window
    return None


if any('archive_entry_path' in u for u, _p in loc_keys):
    fail("§5.2 declares a key component archive_entry_path; the container-level rule "
         "replaces it, because a key over (archive_content_id, archive_entry_path) "
         "admits several playable entries per archive")
else:
    ok("§5.2 declares no key component archive_entry_path")


def key_window(text):
    """The part of a table's own key declaration that may name a key component.

    Only the declaration itself, not the prose around it: `archive_entry_path` is
    legitimately named in §5.2's prose — as an attribute, and as the thing the
    container-level key replaced — but never in a `UNIQUE` tuple.
    """
    flat = norm(text)
    for mm in re.finditer(r'UNIQUE\s*\([^)]*\)', flat):
        if 'archive_entry_path' in mm.group(0):
            return mm.group(0)
    return None


claimed = first_key_sentence(DATA)
if claimed:
    fail("a key over archive_entry_path is claimed again, which weakens the "
         f"container-level rule: …{claimed[:150]}…")
elif key_window(sections.get('content_locations', '')):
    fail("§5.2 declares a key containing archive_entry_path: "
         f"…{key_window(sections['content_locations'])[:160]}…")
elif key_window(CHECKLIST['content_locations']['uniqueness']):
    fail("§20.1 row 9 declares a key containing archive_entry_path: "
         f"…{key_window(CHECKLIST['content_locations']['uniqueness'])[:160]}…")
else:
    ok("no key over archive_entry_path is claimed anywhere")

# `archive_entry_path` must stay declared as a plain column of the one entry row.
if 'archive_entry_path' not in columns.get('content_locations', {}):
    fail("§5.2 no longer declares the archive_entry_path column")
else:
    ok("archive_entry_path stays an ordinary column of the entry row")

# --------------------------------------------------------------------------- #
# 16c. Launch resolution: 0 or 1 row, never an entry-path tie-break.
# --------------------------------------------------------------------------- #
banner("14c. Launch resolution admits no entry-path tie-break (§5.4)")

# An *entry-path* tie-break is the round-5 defect: the resolver must not choose
# among several archive_entry_path values of one (archive, content) pair, because
# the container-level constraint makes that state impossible. What must not come
# back is a rule that *picks* one, or a passage that presents several entry paths as
# a normal state. Documentation that describes the defect while saying it is gone,
# forbidden or impossible is not a defect — which is exactly how §20.3 and this
# script's own table describe it.
ORDERING = (r'smallest|tie[- ]break|\bpicks?\b|\bchooses?\b|\bselects?\b|\bwins\b|'
            r'\borders?\b|\bsorts?\b|one is chosen|resolv\w+')


def key_claim_violation(sentence):
    """True when a sentence makes several entry paths a key, an order or a state."""
    if 'archive_entry_path' not in sentence and \
       not re.search(r'\bentry[- ]paths?\b', sentence, re.I):
        return False
    if re.search(r'UNIQUE\s*\([^)]*archive_entry_path', sentence, re.I):
        return True                    # the round-4 key itself
    if re.search(r'archive_entry_path[^.;]{0,40}\bkeys?\b', sentence, re.I) or \
       re.search(r'\bkeys?\b[^.;]{0,60}archive_entry_path', sentence, re.I) or \
       re.search(r'archive_entry_path[^.;]{0,60}key component', sentence, re.I):
        return True
    plural = re.search(r'\b(?:several|multiple|two|more than one)\b[^.;]{0,60}'
                       r'\bentry[- ]paths?\b', sentence, re.I) or \
        re.search(r'\bentry[- ]paths?\b[^.;]{0,60}'
                  r'\b(?:several|multiple|two|more than one)\b', sentence, re.I)
    if plural and re.search(ORDERING, sentence, re.I):
        return True
    if re.search(r'\b(?:several|multiple|two|more than one)\b[^.;]{0,80}'
                 r'archive_entry_path', sentence, re.I) and \
       re.search(ORDERING, sentence, re.I):
        return True
    return False


def sentence_of(flat, start, end):
    """The sentence around a match, hard-wrapping removed.

    A conjunction may carry the negation ("…, because the constraint makes that
    impossible"), so the sentence is kept whole: from the previous full stop to the
    next one, not truncated at the comma. Hard-wrapped lines are joined first, so
    "the `ArchiveEntry` key of review round 4." is one sentence and not two.
    """
    flat = re.sub(r'\s+', ' ', flat)
    begin = flat.rfind('. ', 0, start) + 1
    finish = flat.find('. ', end)
    finish = len(flat) if finish == -1 else finish + 1
    return flat[begin:finish].strip()


claims = []
for mm in re.finditer(r'archive_entry_path|\bentry[- ]paths?\b', DATA, re.I):
    sentence = sentence_of(DATA, mm.start(), mm.end())
    if not key_claim_violation(sentence):
        continue
    if NEGATED.search(sentence):
        continue        # "no entry-path tie-break exists", "resolver picks none"
    claims.append(sentence)
if claims:
    for s in sorted(set(claims)):
        fail("an entry-path key, order or legitimate state is stated again; the "
             "container-level constraint makes several entry paths for one "
             f"(archive, content) impossible — …{norm(s)[:170]}…")
else:
    ok("no entry-path key, no entry-path ordering, no several-entry-paths claim")

# The container-level key is what replaces the tie-break, so §5.4 must say so.
if 'UNIQUE (archive_content_id) WHERE location_kind = \'ArchiveEntry\'' not in DATA:
    fail("the container-level ArchiveEntry key is not stated in the document, so "
         "§5.4's 0-or-1-row resolution rests on nothing")
else:
    ok("§5.4's resolution is anchored in the stated key")

# --------------------------------------------------------------------------- #
# 16d. Source removal: File locations go, ArchiveEntry locations stay.
# --------------------------------------------------------------------------- #
banner("14d. Source removal deletes File locations only (§19.2)")

s192 = DATA[DATA.index('### 19.2 '):DATA.index('### 19.3 ')]

# (a) the binding rule must name the two kinds.
for needle, label in (
        ("The binding retention rule in one sentence.", "binding retention rule"),
        ("belonging to X", "removal deletes the File locations of X"),
        ("MUST NOT delete `ArchiveEntry`",
         "removal must not delete an ArchiveEntry location"),
        ("location_kind = 'File' AND source_id = X",
         "the File delete predicate")):
    if needle not in re.sub(r'\s+', ' ', s192):
        fail(f"§19.2: the binding source-removal statement does not {label} "
             f"(needle {needle!r} not found)")
    else:
        ok(f"§19.2: {label}")

# (b) no sentence may forbid deleting content_locations in general while the same
#     section deletes this source's File rows. That pair was the round-5 defect.
def removal_clauses(body):
    """Sentences that forbid a source removal from deleting a row type."""
    out = []
    for para in paragraphs(body):
        for sentence in re.split(r'(?<=[.!?])\s+', norm(para)):
            if not re.search(r'remov\w+|delet\w+', sentence, re.I):
                continue
            if not re.search(r'MUST NOT|may not|never|does not|doesn\'t',
                             sentence, re.I):
                continue
            out.append(sentence)
    return out


def mentions_bare_content_locations(clause):
    """True when the clause protects `content_locations` as a whole row type.

    A clause that says `ArchiveEntry` `content_locations` is not protecting the
    whole table, and a clause that names location_kind = 'File' is the intended
    deletion statement, not a contradiction of it.
    """
    if 'content_locations' not in clause:
        return False
    if 'ArchiveEntry' in clause:
        return False
    if "location_kind" in clause:
        return False
    return True


contradictions = [c for c in removal_clauses(s192) if mentions_bare_content_locations(c)]
if contradictions:
    for c in contradictions:
        fail(f"§19.2 contradicts itself: the section deletes this source's "
             f"content_locations rows and also forbids deleting content_locations "
             f"in general — …{c[:160]}…")
else:
    ok("§19.2 never forbids deleting content_locations as a whole")

# (c) the ArchiveEntry row is kept, and reachability is derived from the
#     container's File locations.
for needle, label in (
        ("no `ArchiveEntry` location, ever", "an entry location is never deleted"),
        ("A has no File location left", "the last File location is the reachability edge"),
        ("the ArchiveEntry row REMAINS", "the row remains when unreachable"),
        ("unreachable / not found through normal reconciliation",
         "unreachable is a state, not a deletion"),
        ("No `ArchiveEntry` row is ever deleted because of a source removal",
         "the no-ArchiveEntry-deletion rule is stated in prose too")):
    if needle not in s192:
        fail(f"§19.2: missing the ArchiveEntry rule — {label}")
    else:
        ok(f"§19.2: {label}")

# --------------------------------------------------------------------------- #
# 16e. The EntryList fingerprint record stays independent of the import rule.
# --------------------------------------------------------------------------- #
banner("14e. EntryList is not reduced to one fingerprint per archive (§7.1)")

LIST_PRED = "fingerprint_kind = 'EntryList'"
fp_keys = keys.get('content_fingerprints', {}).get('unique', [])
list_keys = [u for u, p in fp_keys if p == LIST_PRED]
if not list_keys:
    fail(f"§7.1: no unique key with predicate {LIST_PRED!r} (found "
         f"{[(u, p) for u, p in fp_keys]})")
else:
    for u in list_keys:
        if 'entry_path' not in u:
            fail(f"§7.1: the EntryList key {u} no longer contains entry_path, so an "
                 f"archive could hold only one EntryList fingerprint. An archive may "
                 f"contain N physical entries and keep N fingerprints for diagnosis; "
                 f"only imported playable ArchiveEntry locations are bounded.")
        if 'content_id' not in u:
            fail(f"§7.1: the EntryList key {u} is not scoped by content_id")
        if 'archive_content_id' in u:
            fail(f"§7.1: the EntryList key {u} is scoped by archive_content_id, "
                 f"which is not even a column of content_fingerprints")
    if all('entry_path' in u and 'content_id' in u and 'archive_content_id' not in u
           for u in list_keys):
        ok(f"§7.1 EntryList key keeps entry_path and is scoped per content: "
           f"{list_keys}")

# The document must state the distinction explicitly, not only in the key.
for needle, label in (
        ("archive may contain N physical entries", "N physical entries allowed"),
        ("EntryList may store N fingerprints", "N fingerprints allowed"),
        ("imported playable ArchiveEntry locations", "the bounded thing is named"),
        ("at most 1", "the bound is 1")):
    if needle not in DATA:
        fail(f"the EntryList/ArchiveEntry distinction does not state that {label} "
             f"(needle {needle!r} not found)")
    else:
        ok(f"the distinction states: {label}")

if re.search(r'EntryList[^.\n]{0,80}\bat most one\b', DATA, re.I):
    fail("a passage bounds EntryList to one row per archive; only imported playable "
         "ArchiveEntry locations are bounded (§6.3, §7.1)")
else:
    ok("no passage bounds EntryList cardinality")

# --------------------------------------------------------------------------- #
# 14f. §19.4 separates the known content_locations retention cases and must not
#      close the set in either direction: not by presenting the source removal as
#      the only deletion, and not by fixing a total count of deletion operations
#      while the per-game index reset is still owned by #53.
# --------------------------------------------------------------------------- #
banner("14f. content_locations retention: cases separate, no closed count (§19.4)")

s194 = DATA[DATA.index('### 19.4 '):DATA.index('### 19.5 ')]

# (a) the known cases are named, each with its own scope, and the undecided one is
#     delegated rather than invented. The needles are matched against the
#     whitespace-normalised section, so hard-wrapping and the block-quote markers do
#     not decide whether a rule is present.
s194_flat = re.sub(r'>\s*', '', norm(s194))
for needle, label in (
        ("An ordinary missing or unreachable observation never deletes a "
         "`content_locations` row.", "observation deletes nothing"),
        ("A source removal deletes only the removed source's `File` locations.",
         "source removal named with its exact scope"),
        ("and it **never** deletes an `ArchiveEntry` location",
         "source removal spares ArchiveEntry locations"),
        ("A library rebuild clears all `content_locations` rows",
         "library rebuild clears all rows, not one source's"),
        ("specified by §19.3", "the rebuild clear points at §19.3"),
        ("A full reset removes `content_locations` together with all other "
         "BitArchive-owned database state.",
         "full reset removes content_locations"),
        ("deletion semantics stay with the scan-reconciliation work (#53",
         "the per-game index reset is delegated to #53")):
    if needle not in s194_flat:
        fail(f"§19.4 does not state that {label} (needle {needle!r} not found)")
    else:
        ok(f"§19.4: {label}")

# (b) the round-6 defect: calling the source removal the only deletion that can ever
#     affect content_locations, which contradicts §19.3's whole-index clear.
ONLY_CLAIMS = [
    r'only\s+(?:the\s+)?(?:one\s+)?deletion[^.\n]{0,80}content_locations',
    r'only\s+deletion\s+that[^.\n]{0,80}content_locations',
    r'the\s+only\s+[^.\n]{0,40}\bthat\s+touches?\s+`?content_locations',
    r'content_locations[^.\n]{0,80}\bonly\s+deletion\b',
    r'no\s+other\s+operation[^.\n]{0,80}\bdelet\w+[^.\n]{0,40}content_locations',
    r'content_locations[^.\n]{0,60}\bnever\s+cleared\b',
    r'\bnever\s+clears?\s+`?content_locations',
]
only_hits = [m.group(0) for p in ONLY_CLAIMS
             for m in re.finditer(p, s194_flat, re.I)]
if only_hits:
    for h in only_hits:
        fail(f"§19.4 claims the source removal (or a single operation) is the only "
             f"deletion that affects content_locations — …{h[:120]}… — but §19.3 "
             f"clears ALL content_locations in a library rebuild")
else:
    ok("§19.4 does not present one operation as the only content_locations deletion")

# (b2) the round-7 defect, in the other direction: closing the set by fixing a total
#      number of the operations that delete a `content_locations` row. §19.3's full
#      reset deletes the table too, and the exact scope of the per-game
#      "Indexeintrag zurücksetzen" is still owned by #53 (§19.3), so the known cases
#      define the rule without claiming to be all of them.
#
#      Scoped to §19.4 like the check above: §20.3 quotes the rejected wordings when
#      it lists the mutations they were verified against, and a check must not fire
#      on the document explaining the defect it forbids.
CLOSED_COUNT_CLAIMS = [
    (r'deleted in exactly those \w+', 'the set closed as "exactly those …"'),
    (r'and by no (?:third|other|further|fourth) one', '"and by no third one"'),
    (r'exactly (?:two|three|four|five|six|\d+) (?:explicit )?'
     r'(?:deletion|delete|removal)\w* operations?',
     'a counted set of deletion operations'),
    (r'exactly (?:two|three|four|five|six|\d+) operations? '
     r'(?:that )?(?:may |can |could |will )?(?:ever )?(?:delet|clear|remov)\w*',
     'a counted set of operations that delete'),
    (r'\b(?:two|three|four|five) (?:explicit )?(?:deletion|delete|removal)\w* '
     r'operations?\b', 'a bare count of deletion operations'),
    (r'no (?:third|other|further|fourth) operation\b[^.\n]{0,80}'
     r'\b(?:delet|clear|remov)\w*', 'a claim that no further operation deletes'),
    # The two "only operations" forms need a deletion verb in reach: §19.4 says the
    # full reset is the only operation that destroys *identities*, which is a
    # different claim and stays true.
    (r'only these operations?\b[^.\n]{0,80}\b(?:delet|clear|remov)\w*',
     '"only these operations delete"'),
    (r'(?:are|is) the only operations?\b[^.\n]{0,80}\b(?:delet|clear|remov)\w*',
     '"are the only operations that delete"'),
    (r'(?:complete|exhaustive|closed) (?:list|set|count|number|enumeration) of '
     r'(?:the )?(?:deletion|delete|removal)', 'a "complete/exhaustive" deletion set'),
]
closed_hits = [(m.group(0), label) for pattern, label in CLOSED_COUNT_CLAIMS
               for m in re.finditer(pattern, s194_flat, re.I)]
if closed_hits:
    for hit, label in closed_hits:
        fail(f"§19.4 closes the set of content_locations deletion operations — "
             f"…{hit[:120]}… ({label}) — but §19.3's full reset deletes the table as "
             f"well, and the per-game index reset is still owned by #53")
else:
    ok("§19.4 claims no total count of content_locations deletion operations")

# (c) an observation may never delete a row: missing, unreachable, offline and
#     "not found" are states. Checked in §19.4 and in the retention summary.
#
#     Only the deletion verb itself may decide this: "the row is deleted and
#     `last_seen_at` is not advanced" is a defect even though the sentence contains a
#     "not", and "never deleted" is fine even though the verb is there. Each table
#     row is judged on its own, so the maintenance-queue row of §19.7 cannot speak
#     for the retention row next to it.
STATE_WORDS = r'missing|unreachable|not found|offline|permission[- ]denied|' \
              r'not seen|reappears|is gone|no longer present|absent'

DELETION_VERB = re.compile(
    r'\b(?:delet\w+|remov\w+|clears?|cleared|drops?|dropped)\b', re.I)
# A negation counts only when it sits on the verb itself: immediately before it
# ("never deletes", "does not delete the row") or immediately after the participle
# with nothing but a pronoun in between ("is not deleted", "are never deleted").
# A negation that merely shares the clause ("is deleted and `last_seen_at` is not
# advanced") does not excuse the claim.
NEGATION = r'(?:not|never|no|neither|nor|does\s*n[o\']?t|do\s*n[o\']?t|' \
           r'without|cannot|can\s*not)'
NEG_BEFORE = re.compile(NEGATION + r'[\s`*_)\]\w]{0,20}$', re.I)
NEG_AFTER = re.compile(r'^(?:[\s`*_.,)\]]{0,4}' + NEGATION + r'\b|'
                       r'[\s`*_.,)\]]{1,4}(?:it|them|the\s+rows?|the\s+row)\s+'
                       + NEGATION + r'\b)', re.I)


def negated_deletion(sentence, verb):
    """True when negation sits on the deletion verb, not merely in the sentence."""
    before = sentence[max(0, verb.start() - 40):verb.start()]
    if NEG_BEFORE.search(before):
        return True
    return bool(NEG_AFTER.match(sentence[verb.end():]))


def table_rows(section):
    """The rows of every Markdown table in a section, as cell lists.

    A row is one statement even though its state and its effect sit in different
    cells — "A location is not seen … | … the row is deleted" claims that a missing
    observation deletes a row. The rows are recovered from the raw lines, because
    neighbouring rows must not be merged into one statement.
    """
    rows = []
    for line in section.split('\n'):
        line = line.strip()
        if not line.startswith('|'):
            continue
        row = line.strip('|').strip()
        if not row or set(row) <= set('-| '):
            continue        # the `|---|---|` separator
        rows.append([c.strip() for c in row.split('|')])
    return rows


def clauses(unit):
    """Clauses of a statement: a state and a deletion must share one.

    "`Missing` is a state, not a deletion. A source removal deletes that source's
    `File` rows only" is two independent statements, and only the second one talks
    about deleting. Semicolons, commas and coordinating conjunctions separate them —
    but a table row is never split on its cell separator, because the state in one
    cell and the effect in the next are one statement ("A location is not seen …
    | … the row is deleted").
    """
    return [c.strip() for c in re.split(r'[;,]|\band\b|\bbut\b|\bso\b|\btherefore\b',
                                        unit, flags=re.I) if c.strip()]


# The actor of a deletion is named in the sentence that states it: "a source
# removal deletes …", "a library rebuild clears …", "only the full reset removes …".
# Without such an actor, a clause inside a statement about something missing or
# unreachable may not delete anything.
DELETE_ACTOR = re.compile(
    r'source removal|library rebuild|full reset|per-user|garbage collect\w*|'
    r'provider refresh|\bprun\w+|reset\b|maintenance|scaveng\w+|sweep\w*|'
    r'reconciliation pass|discovery pass|\bcascade\w*', re.I)


def deletion_clauses(cell_or_sentence):
    """Clauses that contain a deletion verb, as (clause, verb) pairs.

    A `deletion` as a noun is not a deletion verb: `Only after a successful file
    deletion; Missing is a state` describes a state, not an operation.
    """
    out = []
    for clause in clauses(cell_or_sentence):
        for m in DELETION_VERB.finditer(clause):
            if re.fullmatch(r'deletions?', m.group(0), re.I):
                continue
            out.append((clause, m))
    return out


def observation_deletion(parts):
    """The deletion clauses of a statement whose actor is not named.

    A statement may legitimately delete (`a source removal deletes the File rows`,
    `a library rebuild clears the table`). What may not delete is an observation:
    a statement that talks about something being missing, unreachable or offline and
    names no actor for the deletion.
    """
    text = ' '.join(parts)
    if DELETE_ACTOR.search(text):
        return []
    return [clause for clause, _verb in deletion_clauses(text)
            if not any(negated_deletion(clause, v)
                       for _c, v in deletion_clauses(clause))]


def covered_by_negation(parts):
    """True when the statement explicitly says a deletion does not happen."""
    return any(negated_deletion(clause, verb)
               for clause, verb in deletion_clauses(' '.join(parts)))


deleting_states = []
for section_name, body in (('§19.4', s194),
                           ('§19.7', DATA[DATA.index('### 19.7 '):DATA.index('### 19.8 ')])):
    # Tables: one statement per row, judged across its cells.
    for cells in table_rows(body):
        row_text = ' ; '.join(cells)
        if not re.search(STATE_WORDS, row_text, re.I):
            continue
        bad = observation_deletion(cells)
        if bad and not covered_by_negation(cells):
            deleting_states.append((section_name, row_text.strip()))
    # Prose: one statement per sentence.
    for block in paragraphs(body):
        if block.lstrip().startswith('|'):
            continue        # handled as rows above
        for sentence in re.split(r'(?<=[.!?])\s+', norm(block)):
            if not re.search(STATE_WORDS, sentence, re.I):
                continue
            if observation_deletion([sentence]):
                deleting_states.append((section_name, sentence.strip()))
if deleting_states:
    for name, s in sorted(set(deleting_states)):
        fail(f"{name}: a missing/unreachable/offline observation is described as "
             f"deleting a row — …{norm(s)[:150]}…")
else:
    ok("no section lets a missing/unreachable/offline observation delete a row")

# (d) the known cases must be enumerated together somewhere, so no two of them can
#     be merged silently again: §19.4's binding rule and §20.3 test 46. The needles
#     assert the cases, not a count of them.
for needle, label in (
        ("An ordinary missing or unreachable observation never deletes",
         "§19.4 separates the observation case"),
        ("A source removal deletes only the removed source's `File` locations",
         "§19.4 names the source removal and its scope"),
        ("A library rebuild clears all `content_locations` rows",
         "§19.4 names the rebuild"),
        ("A full reset removes `content_locations` together with all other",
         "§19.4 names the full reset as a deletion of the table"),
        ("scan-reconciliation work (#53", "§19.4 delegates the per-game reset to #53"),
        ("`content_locations` retention operations remain distinct",
         "§20.3 test 46 asserts the cases stay distinct"),
        ("its exact deletion scope belongs to #53",
         "§20.3 test 46 leaves the per-game scope to #53")):
    if needle not in DATA:
        fail(f"the content_locations retention cases are not enumerated together: "
             f"{label}")
    else:
        ok(f"the cases are enumerated: {label}")

# (e) §19.7 must still say the same things about content_locations (and is not
#     reformulated if it does). Its row is the retention summary the other sections
#     are read against, so the four known cases have to survive there too.
s197_flat = norm(DATA[DATA.index('### 19.7 '):DATA.index('### 19.8 ')])
for needle, label in (
        ("`Missing` is a state, not a deletion", "the state is not a deletion"),
        ("deletes that source's `File` rows only", "source removal deletes File rows only"),
        ("a library rebuild clears the table", "the rebuild clears the table"),
        ("the full reset drops it with the rest of the state",
         "the full reset drops the table with the state")):
    if needle not in s197_flat:
        fail(f"§19.7 retention summary does not state that {label} "
             f"(needle {needle!r} not found)")
    else:
        ok(f"§19.7: {label}")

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
