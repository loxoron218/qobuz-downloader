#!/usr/bin/env bash
# post-compiler.sh — exit 1 if any unwanted pattern from post-compiler.md is found.
set -uo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")" || exit 2
[ $# -gt 0 ] && { echo "usage: ./post-compiler.sh (no arguments)" >&2; exit 2; }

command -v rg >/dev/null || { echo "ERROR: 'rg' not found in PATH" >&2; exit 2; }
command -v python3 >/dev/null || { echo "ERROR: 'python3' not found in PATH" >&2; exit 2; }
command -v jscpd >/dev/null || { echo "ERROR: 'jscpd' not found in PATH" >&2; exit 2; }

dirs=()
for d in src tests benches; do [ -e "$d" ] && dirs+=("$d"); done
[ "${#dirs[@]}" -gt 0 ] || { echo "ERROR: no scan dirs (src tests benches)" >&2; exit 2; }

fail=0
# Print the description plus matches when a query hits; otherwise stay silent.
show() { [ -n "$2" ] && { echo; echo "# $1"; printf '%s\n' "$2"; fail=1; }; }

# Underscore-prefixed identifiers; use meaningful names instead of discards.
# (String literals are stripped first, so GTK mnemonics like "Import _Album" don't count.)
show "Underscore-prefixed identifiers; use meaningful names instead of discards." "$(rg -n --glob '*.rs' -P '(?<![\w"}\*`])_[A-Za-z][A-Za-z0-9_]*\b' "${dirs[@]}" | python3 -c '
import sys,re
def strip_strings(s):
    s=re.sub(r"r#*\"(?:[^\"#]|\"(?!#))*\"#*", "\"\"", s)
    s=re.sub(r"\"([^\"\\\\]|\\\\.)*\"", "\"\"", s)
    return s
pat=re.compile(r"(?<![\w\"}\*`])_[A-Za-z][A-Za-z0-9_]*\b")
for line in sys.stdin:
    line=line.rstrip("\n")
    parts=line.split(":",2)
    if len(parts)<3:
        continue
    code=strip_strings(parts[2])
    if pat.search(code):
        print(line)
')"
# .ok() silently discards errors instead of propagating them with context.
show ".ok() silently discards errors instead of propagating them with context." "$(rg -n --glob '*.rs' -F '.ok()' "${dirs[@]}")"
# super:: paths; prefer crate::-relative imports.
show "super:: paths; prefer crate::-relative imports." "$(rg -n --glob '*.rs' -F 'super::' "${dirs[@]}")"
# pub(crate) visibility; keep visibility explicit and consistent.
show "pub(crate) visibility; keep visibility explicit and consistent." "$(rg -n --glob '*.rs' -F '(crate)' "${dirs[@]}")"
# Remove all // comments (URLs inside string literals are not comments).
show "Remove all // comments." "$(rg --files --glob '*.rs' "${dirs[@]}" | python3 -c '
import sys,re
def strip_strings(s):
    s=re.sub(r"r#*\"(?:[^\"#]|\"(?!#))*\"#*", "\"\"", s)
    s=re.sub(r"\"([^\"\\\\]|\\\\.)*\"", "\"\"", s)
    s=re.sub(r"'"'"'([^'"'"'\\\\]|\\\\.)*'"'"'", "\"\"", s)
    return s
pat=re.compile(r"(?<!/)//(?!/|!)")
for path in (l.strip() for l in sys.stdin if l.strip()):
    try:
        lines=open(path).read().splitlines()
    except OSError:
        continue
    for idx,l in enumerate(lines, start=1):
        code=strip_strings(l)
        if pat.search(code):
            print(f"{path}:{idx}:{l}")
')"
# Fully-qualified foo::Bar at call sites; import via use instead (constructors exempt).
# Note: $crate:: refs inside macro_rules! are hygienic and exempt (required for #[macro_export]).
show "Fully-qualified foo::Bar at call sites; import via use instead (constructors exempt)." "$(rg --files --glob '*.rs' "${dirs[@]}" | python3 -c '
import sys,re
def strip_strings(s):
    s=re.sub(r"r#*\"(?:[^\"#]|\"(?!#))*\"#*", "\"\"", s)
    s=re.sub(r"\"([^\"\\\\]|\\\\.)*\"", "\"\"", s)
    return s
use_start=re.compile(r"^\s*(pub\s+)?use\b")
comment=re.compile(r"\s*//")
pat=re.compile(r"\b(?!(?:Self|f32|f64|i8|i16|i32|i64|i128|isize|u8|u16|u32|u64|u128|usize|bool|char|str)\b)(crate|super|self|[a-z][a-z0-9_]*)::(?!new\b|builder\b|build\b|default\b)[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*")
for path in (l.strip() for l in sys.stdin if l.strip()):
    try:
        lines=open(path).read().splitlines()
    except OSError:
        continue
    in_use=False
    for idx,l in enumerate(lines, start=1):
        code=strip_strings(l)
        if not in_use and use_start.search(code):
            if ";" not in code:
                in_use=True
            continue
        if in_use:
            if ";" in code:
                in_use=False
            continue
        if comment.match(l):
            continue
        if "$crate::" in l:
            continue
        code=re.sub(r"//.*","",code)
        if pat.search(code):
            print(f"{path}:{idx}:{l}")
')"
# Enum variants used as Type::Variant at call sites; import all variants via nested
# use instead (constructors, Self::, single-letter generics like D::Error, and
# SCREAMING_CASE associated constants like Level::INFO stay qualified;
# variants of an enum defined in the same file stay qualified as Enum::Variant;
# $crate:: is macro hygiene).
show "Enum variants used as Type::Variant at call sites; import variants via nested use instead." "$(rg --files --glob '*.rs' "${dirs[@]}" | python3 -c '
import sys,re
def strip_strings(s):
    s=re.sub(r"r#*\"(?:[^\"#]|\"(?!#))*\"#*", "\"\"", s)
    s=re.sub(r"\"([^\"\\\\]|\\\\.)*\"", "\"\"", s)
    return s
use_start=re.compile(r"^\s*(pub\s+)?use\b")
comment=re.compile(r"\s*//")
enum_def=re.compile(r"^\s*(pub\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)\b")
variant=re.compile(r"\b(?!(?:Self|[A-Z])\b)([A-Z][A-Za-z0-9_]*)::(?!new\b|builder\b|build\b|default\b)(?![A-Z][A-Z0-9_]*\b)[A-Z][A-Za-z0-9_]*\b")
for path in (l.strip() for l in sys.stdin if l.strip()):
    try:
        lines=open(path).read().splitlines()
    except OSError:
        continue
    local_enums=set()
    for l2 in lines:
        m=enum_def.match(strip_strings(l2))
        if m:
            local_enums.add(m.group(2))
    in_use=False
    for idx,l in enumerate(lines, start=1):
        code=strip_strings(l)
        if not in_use and use_start.search(code):
            if ";" not in code:
                in_use=True
            continue
        if in_use:
            if ";" in code:
                in_use=False
            continue
        if comment.match(l):
            continue
        if "$crate::" in l:
            continue
        code=re.sub(r"//.*","",code)
        m=variant.search(code)
        if m and m.group(1) not in local_enums:
            print(f"{path}:{idx}:{l}")
')"
# trace! and debug! logging; use info!, warn!, or error! for significant events.
show "trace! and debug! logging; use info!, warn!, or error! for significant events." "$(rg -n --glob '*.rs' -P '\b(trace|debug)!' "${dirs[@]}")"
# prelude imports inside gtk/gio/glib/gdk/gdk_pixbuf/pango groups instead of
# libadwaita itself; use libadwaita::prelude instead. Deep libadwaita:: re-export
# paths stay allowed.
show "prelude imports inside gtk/gio/glib/gdk/gdk_pixbuf/pango groups; use libadwaita::prelude instead." "$(rg --files --glob '*.rs' "${dirs[@]}" | python3 -c '
import sys,re
def strip_strings(s):
    s=re.sub(r"r#*\"(?:[^\"#]|\"(?!#))*\"#*", "\"\"", s)
    s=re.sub(r"\"([^\"\\\\]|\\\\.)*\"", "\"\"", s)
    s=re.sub(r"'"'"'([^'"'"'\\\\]|\\\\.)*'"'"'", "\"\"", s)
    return s
comment=re.compile(r"\s*//")
gtk_parent=re.compile(r"\b(gtk|gio|glib|gdk|gdk_pixbuf|pango)::prelude\b")
bare_pre=re.compile(r"(?<![\w:])prelude::")
for path in (l.strip() for l in sys.stdin if l.strip()):
    try:
        lines=open(path).read().splitlines()
    except OSError:
        continue
    code_lines=[re.sub(r"//.*","",strip_strings(l)) for l in lines]
    text="\n".join(code_lines)
    flagged=set()
    def lineno(pos):
        return text.count("\n",0,pos)+1
    def commented(pos):
        ls=text.rfind("\n",0,pos)+1
        le=text.find("\n",pos)
        if le<0:
            le=len(text)
        if comment.match(text[ls:]):
            return True
        return "$crate::" in text[ls:le]
    for m in gtk_parent.finditer(text):
        if not commented(m.start()):
            flagged.add(lineno(m.start()))
    for m in bare_pre.finditer(text):
        if commented(m.start()):
            continue
        depth=0
        k=m.start()-1
        opener=-1
        while k>=0:
            ch=text[k]
            if ch=="}":
                depth+=1
            elif ch=="{":
                if depth==0:
                    opener=k
                    break
                depth-=1
            k-=1
        if opener<0:
            continue
        j=opener-1
        while j>=0 and text[j] in " \t":
            j-=1
        e=j
        while j>=0 and (text[j].isalnum() or text[j] in "_:"):
            j-=1
        segs=[s for s in text[j+1:e+1].split("::") if s]
        if segs and segs[0] in ("gtk","gio","glib","gdk","gdk_pixbuf","pango") and segs[-1] in ("gtk","gio","glib","gdk","gdk_pixbuf","pango") and "prelude" not in segs:
            flagged.add(lineno(m.start()))
    for ln in sorted(flagged):
        print(f"{path}:{ln}:{lines[ln-1]}")
')"
# Stray // and /// comments inside #[cfg(test)] modules (only # Errors/# Panics sections with summary exempt).
show "Stray // and /// comments inside #[cfg(test)] modules (only # Errors/# Panics sections with summary exempt)." "$(rg -l --glob '*.rs' '#\[cfg\(test\)\]' "${dirs[@]}" | python3 -c '
import sys,re
def strip_strings(s):
    s=re.sub(r"r#*\"(?:[^\"#]|\"(?!#))*\"#*", "\"\"", s)
    s=re.sub(r"\"([^\"\\\\]|\\\\.)*\"", "\"\"", s)
    return s
def strip(s):
    s=strip_strings(s)
    s=re.sub(r"//.*","",s)
    return s
for path in (l.strip() for l in sys.stdin if l.strip()):
    lines=open(path).read().splitlines()
    ranges=[];i=0;n=len(lines)
    while i<n:
        if re.search(r"#\[cfg\(test\)\]",lines[i]):
            j=i+1
            while j<n and (lines[j].strip()=="" or (lines[j].lstrip().startswith("#") and "mod" not in lines[j])):
                j+=1
                if j>i+6: break
            if j<n and re.search(r"\bmod\s+\w+.*\{",lines[j]):
                depth=0;k=j
                while k<n:
                    t=strip(lines[k]);depth+=t.count("{")-t.count("}");k+=1
                    if depth<=0 and k>j+1: break
                ranges.append((j,k));i=k;continue
        i+=1
    def in_test(ln):
        return any(a<=ln<b for a,b in ranges)
    idx=0
    while idx<n:
        l=lines[idx]
        code=strip_strings(l)
        if "//" in code and in_test(idx) and re.match(r"\s*//",l):
            j=idx
            while j<n and re.match(r"\s*//",lines[j]) and (j==idx or in_test(j)): j+=1
            block=lines[idx:j]
            headers=[n for n,b in enumerate(block) if "# Errors" in b or "# Panics" in b]
            if headers and headers[0]>0: idx=j;continue
            for b_idx in range(idx,j): print(f"{path}:{b_idx+1}:{lines[b_idx]}")
            idx=j;continue
        if "//" in code and in_test(idx):
            m=re.search(r"(?<!/)//(?!/|!)",code)
            comment=code[m.start():] if m else ""
            if "# Errors" in comment or "# Panics" in comment: idx+=1;continue
            print(f"{path}:{idx+1}:{l}")
        idx+=1
' | rg --color=never '//')"
# Files with more than one #[cfg(test)] block; one mod tests per file.
show "Files with more than one #[cfg(test)] block; one mod tests per file." "$(rg -n --glob '*.rs' '#\[cfg\(test\)\]' "${dirs[@]}" | rg -v '//.*#\[cfg\(test\)\]|/\*.*#\[cfg\(test\)\]' | awk -F: '{c[$1]++} END {for (f in c) if (c[f]>1) print f": "c[f]}')"
# anyhow outside test code and compliant app boundaries; library uses thiserror,
# binary/app boundaries use anyhow::{Context, Result} with .context()/.with_context(),
# tests use anyhow::Result. Parent-gated #[cfg(test)] file modules and binaries exempt.
show "anyhow outside test code and compliant app boundaries; library uses thiserror, binary/app boundaries use anyhow::{Context, Result} with .context()/.with_context(), tests use anyhow::Result." "$(rg --files --glob '*.rs' "${dirs[@]}" | python3 -c '
import sys,re
paths=[l.strip() for l in sys.stdin if l.strip()]
test_stems=set()
mod_file=re.compile(r"^\s*(pub(\([^)]*\))?\s+)?mod\s+(\w+)\s*;")
cfg=re.compile(r"#\[cfg\(test\)\]")
for path in paths:
    try:
        lines=open(path).read().splitlines()
    except OSError:
        continue
    for i,l in enumerate(lines):
        if cfg.search(l):
            for k in range(i+1,min(i+4,len(lines))):
                m=mod_file.search(lines[k])
                if m:
                    test_stems.add(m.group(3))
                    break
                if lines[k].strip()=="" or lines[k].lstrip().startswith("#"):
                    continue
                if "{" in lines[k]:
                    break
seen=set()
def strip_strings(s):
    s=re.sub(r"r#*\"(?:[^\"#]|\"(?!#))*\"#*", "\"\"", s)
    s=re.sub(r"\"([^\"\\\\]|\\\\.)*\"", "\"\"", s)
    return s
def strip(s):
    s=strip_strings(s)
    s=re.sub(r"//.*","",s)
    return s
for path in paths:
    if "/tests/" in path or path.startswith("tests/"):
        continue
    if path.endswith("src/main.rs") or "/src/bin/" in path or path.startswith("src/bin/"):
        continue
    stem=path.rsplit("/",1)[-1]
    if stem.endswith(".rs"):
        stem=stem[:-3]
    if stem in test_stems:
        continue
    try:
        lines=open(path).read().splitlines()
    except OSError:
        continue
    joined="\n".join(strip(l) for l in lines)
    if re.search(r"\banyhow\b",joined) and re.search(r"\bContext\b",joined) and (".context(" in joined or ".with_context(" in joined):
        continue
    ranges=[];i=0;n=len(lines)
    while i<n:
        if re.search(r"#\[cfg\(test\)\]",lines[i]):
            j=i+1
            while j<n and (lines[j].strip()=="" or (lines[j].lstrip().startswith("#") and "mod" not in lines[j])):
                j+=1
                if j>i+6: break
            if j<n and re.search(r"\bmod\s+\w+\s*;",lines[j]):
                ranges.append((0,n));break
            if j<n and re.search(r"\bmod\s+\w+.*\{",lines[j]):
                depth=0;k=j
                while k<n:
                    t=strip(lines[k]);depth+=t.count("{")-t.count("}");k+=1
                    if depth<=0 and k>j+1: break
                ranges.append((j,k));i=k;continue
        i+=1
    def in_test(ln):
        return any(a<=ln<b for a,b in ranges)
    for idx,l in enumerate(lines):
        if path not in seen and re.search(r"\banyhow\b",strip(l)) and not in_test(idx):
            print(f"{path}:{idx+1}:{l}")
            seen.add(path)
')"
# mod/pub mod and empty #[cfg(test)] mod tests at bottom instead of top; keep module declarations at top, tests at bottom with actual tests.
show "mod/pub mod and empty #[cfg(test)] mod tests at bottom instead of top; keep module declarations at top, tests at bottom with actual tests." "$(rg --files --glob '*.rs' "${dirs[@]}" | python3 -c '
import sys,re
def strip_strings(s):
    s=re.sub(r"r#*\"(?:[^\"#]|\"(?!#))*\"#*", "\"\"", s)
    s=re.sub(r"\"([^\"\\\\]|\\\\.)*\"", "\"\"", s)
    return s
mod_decl=re.compile(r"^\s*(pub(\s*\([^)]*\))?\s+)?mod\s+(\w+)\b")
test_attr=re.compile(r"#\s*\[\s*(tokio::test|test|rstest|case)\b")
code_item=re.compile(r"^\s*(pub(\s*\([^)]*\))?\s+)?(async\s+)?(unsafe\s+)?(use|extern|fn|struct|enum|trait|impl|const|static|type|macro_rules!|macro)\b")
macro_call=re.compile(r"^\s*(\w+::)*\w+!\b")
attr_prefix=re.compile(r"^\s*(#\[[^\]]*\]\s*)+")
for path in (l.strip() for l in sys.stdin if l.strip()):
    try:
        lines=open(path).read().splitlines()
    except OSError:
        continue
    n=len(lines)
    depth=0
    seen_code=False
    idx=0
    while idx<n:
        raw=lines[idx]
        code=re.sub(r"//.*", "", strip_strings(raw))
        if depth==0:
            no_attr=attr_prefix.sub("", code)
            m=mod_decl.match(no_attr)
            if m:
                name=m.group(3)
                rest=no_attr[m.end():]
                has_semi=";" in rest
                has_brace="{" in rest
                if has_brace and (not has_semi or rest.find("{")<rest.find(";")):
                    kind="inline"
                elif has_semi and (not has_brace or rest.find(";")<rest.find("{")):
                    kind="file"
                else:
                    k2=idx+1
                    found=None
                    while k2<n and k2<=idx+3:
                        c2=re.sub(r"//.*", "", strip_strings(lines[k2]))
                        if c2.strip()=="" or c2.strip().startswith("#"):
                            k2+=1
                            continue
                        if "{" in c2 and ";" not in c2:
                            found="inline"
                            break
                        if "{" in c2 and ";" in c2:
                            found="inline" if c2.find("{")<c2.find(";") else "file"
                            break
                        if ";" in c2:
                            found="file"
                            break
                        break
                    kind=found if found else "file"
                is_tests=(name=="tests")
                if is_tests and kind=="inline":
                    d2=0
                    started=False
                    k=idx
                    while k<n:
                        ck=re.sub(r"//.*", "", strip_strings(lines[k]))
                        o=ck.count("{")
                        c=ck.count("}")
                        if o>0:
                            started=True
                        d2+=o-c
                        k+=1
                        if started and d2<=0:
                            break
                    body="\n".join(re.sub(r"//.*", "", strip_strings(x)) for x in lines[idx:k])
                    if not test_attr.search(body):
                        print(f"{path}:{idx+1}:{raw}")
                    idx=k
                    continue
                if is_tests and kind=="file":
                    idx+=1
                    continue
                if seen_code:
                    print(f"{path}:{idx+1}:{raw}")
                if kind=="inline":
                    d2=0
                    started=False
                    k=idx
                    while k<n:
                        ck=re.sub(r"//.*", "", strip_strings(lines[k]))
                        o=ck.count("{")
                        c=ck.count("}")
                        if o>0:
                            started=True
                        d2+=o-c
                        k+=1
                        if started and d2<=0:
                            break
                    idx=k
                    continue
                depth+=code.count("{")-code.count("}")
                idx+=1
                continue
            s=code.strip()
            if s=="" or s.startswith("#") or s in ("{", "}", ";"):
                depth+=code.count("{")-code.count("}")
                if depth<0:
                    depth=0
                idx+=1
                continue
            no_attr2=attr_prefix.sub("", code)
            if code_item.search(no_attr2) or macro_call.search(no_attr2):
                seen_code=True
            depth+=code.count("{")-code.count("}")
            if depth<0:
                depth=0
            idx+=1
            continue
        else:
            depth+=code.count("{")-code.count("}")
            if depth<0:
                depth=0
            idx+=1
            continue
')"
# .rs files over 400 lines; split into a subdirectory with a parent index.
show ".rs files over 400 lines; split into a subdirectory with a parent index." "$(
    rg --files --glob '*.rs' "${dirs[@]}" | while IFS= read -r f; do
        n=$(wc -l < "$f" | tr -d ' ')
        [ -n "$n" ] && [ "$n" -gt 400 ] && echo "$f: $n lines"
    done
)"
# mod.rs files that are not Cargo target roots; use foo.rs parent indexes instead.
show "mod.rs files that are not Cargo target roots; use foo.rs parent indexes instead." "$(comm -23 <(rg --files --glob '*.rs' "${dirs[@]}" | rg '/mod\.rs$' | LC_ALL=C sort) <(rg -o --no-filename 'path\s*=\s*"[^"]+\.rs"' Cargo.toml | rg -o '"[^"]+"' | tr -d '"' | LC_ALL=C sort -u))"
# Copy-paste clones across src/benches/tests; ignore duplicates that are only import blocks.
out=$(jscpd --format rust --no-colors --no-tips --absolute --exit-code 1 "${dirs[@]}" | awk '/^Clone found/{p=1} /^┌/{p=0} p') || { show "Copy-paste clones across src/benches/tests; ignore duplicates that are only import blocks." "$out"; fail=1; }

exit "$fail"
