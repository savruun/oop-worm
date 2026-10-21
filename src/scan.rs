/*!
The class model of one Luau file, from a token walk.

The worm reads plain text and owns no type information, so everything
here is convention and lexical structure: a class is a table a statement
binds by name, a context is a function or a table that the class owns,
and a private member is an underscore name or one a doc comment marks
`@private`. The lints and the completion filter both read this model, so
the two always agree on what is inside a class and what is not.
*/

use std::collections::BTreeMap;

/// One token. Comments never land here; they live in `Model::comments`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Ident,
    Str,
    Num,
    Op,
}

#[derive(Debug, Clone, Copy)]
pub struct Tok {
    pub kind: Kind,
    pub s: u32,
    pub e: u32,
}

/// What sits before the dot of a member access
#[derive(Debug, Clone, PartialEq)]
pub enum Recv {
    /// The literal name `self`
    Zelf,
    /// A bare identifier, ex: `Class` in `Class._x`
    Named(String),
    /// Anything longer, ex: `a.b._x` or `f()._x`
    Complex,
}

#[derive(Debug, Clone)]
pub struct Access {
    pub name: String,
    /// The span of the member name itself
    pub span: (u32, u32),
    pub recv: Recv,
    /// The next token is a single `=`, so this access writes
    pub write: bool,
    /// The access is a definition head, ex: the `m` of `function C.m()`
    pub def_head: bool,
}

#[derive(Debug, Clone)]
pub struct Member {
    /// The span of the defining name
    pub span: (u32, u32),
    /// Underscore-named, or marked `@private` in a doc comment
    pub private: bool,
    /*
    Marked `@lifecycle`: something outside this file calls it.

    A framework calls the methods it names, ex: chief calls the
    lifecycle of a provider, and no line of the file that declares one
    ever does. Without the mark that reads as dead code, and the author
    is told to delete the method the framework needs.
    */
    pub lifecycle: bool,
}

#[derive(Debug, Default)]
pub struct Class {
    pub name: String,
    pub members: BTreeMap<String, Member>,
    /// The byte ranges this class owns: its tables and its functions
    pub contexts: Vec<(u32, u32)>,
}

impl Class {
    pub fn declares_private(&self, name: &str) -> bool {
        self.members.get(name).is_some_and(|m| m.private)
    }
}

#[derive(Debug, Default)]
pub struct Model {
    pub comments: Vec<(u32, u32)>,
    pub classes: Vec<Class>,
    pub accesses: Vec<Access>,
    /// `function C:name` heads, as (class index, span of `C:name`)
    pub colon_defs: Vec<(usize, (u32, u32))>,
    /// `setmetatable(_, C)` sites, as (class index, span of the call name)
    pub setmeta: Vec<(usize, (u32, u32))>,
}

impl Model {
    pub fn class_named(&self, name: &str) -> Option<&Class> {
        self.classes.iter().find(|c| c.name == name)
    }

    /// The innermost class context that holds this offset
    pub fn context_at(&self, at: u32) -> Option<&Class> {
        self.classes
            .iter()
            .flat_map(|c| {
                c.contexts
                    .iter()
                    .filter(move |&&(s, e)| s <= at && at < e)
                    .map(move |&(s, e)| (e - s, c))
            })
            .min_by_key(|&(width, _)| width)
            .map(|(_, c)| c)
    }
}

pub fn lex(src: &str) -> (Vec<Tok>, Vec<(u32, u32)>) {
    let b = src.as_bytes();
    let mut toks = Vec::new();
    let mut comments = Vec::new();
    let mut i = 0usize;

    // A shebang line is legal Luau and holds no tokens.
    if b.starts_with(b"#!") {
        while i < b.len() && b[i] != b'\n' {
            i += 1;
        }
    }

    while i < b.len() {
        let c = b[i];

        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        if c == b'-' && i + 1 < b.len() && b[i + 1] == b'-' {
            let start = i;
            i += 2;

            if let Some(end) = long_bracket(b, i) {
                comments.push((start as u32, end as u32));
                i = end;
            } else {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                comments.push((start as u32, i as u32));
            }
            continue;
        }

        if c == b'[' {
            if let Some(end) = long_bracket(b, i) {
                toks.push(Tok {
                    kind: Kind::Str,
                    s: i as u32,
                    e: end as u32,
                });
                i = end;
                continue;
            }
        }

        if c == b'"' || c == b'\'' {
            let start = i;
            i += 1;
            while i < b.len() && b[i] != c {
                if b[i] == b'\\' {
                    i += 1;
                }
                if b[i] == b'\n' {
                    break;
                }
                i += 1;
            }
            i = (i + 1).min(b.len());
            toks.push(Tok {
                kind: Kind::Str,
                s: start as u32,
                e: i as u32,
            });
            continue;
        }

        if c == b'`' {
            // An interpolated string; braces inside stay out of the balance.
            let start = i;
            i += 1;
            while i < b.len() && b[i] != b'`' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(b.len());
            toks.push(Tok {
                kind: Kind::Str,
                s: start as u32,
                e: i as u32,
            });
            continue;
        }

        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            toks.push(Tok {
                kind: Kind::Ident,
                s: start as u32,
                e: i as u32,
            });
            continue;
        }

        if c.is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                // `1..2` is a number then concat, not a longer number.
                if b[i] == b'.' && i + 1 < b.len() && b[i + 1] == b'.' {
                    break;
                }
                i += 1;
            }
            toks.push(Tok {
                kind: Kind::Num,
                s: start as u32,
                e: i as u32,
            });
            continue;
        }

        toks.push(Tok {
            kind: Kind::Op,
            s: i as u32,
            e: (i + 1) as u32,
        });
        i += 1;
    }

    (toks, comments)
}

/// The end of a `[[`/`[=[` bracket that opens at `i`, past `--` if a comment
fn long_bracket(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i;
    if j >= b.len() || b[j] != b'[' {
        return None;
    }
    j += 1;

    let mut level = 0;
    while j < b.len() && b[j] == b'=' {
        level += 1;
        j += 1;
    }
    if j >= b.len() || b[j] != b'[' {
        return None;
    }
    j += 1;

    let close: Vec<u8> = std::iter::once(b']')
        .chain(std::iter::repeat_n(b'=', level))
        .chain(std::iter::once(b']'))
        .collect();

    while j < b.len() {
        if b[j] == b']' && b[j..].starts_with(&close) {
            return Some(j + close.len());
        }
        j += 1;
    }

    Some(b.len())
}

fn text<'a>(src: &'a str, t: &Tok) -> &'a str {
    &src[t.s as usize..t.e as usize]
}

/// A `=` that assigns: not `==`, `<=`, `>=`, `~=`, and not compound.
fn assigns(toks: &[Tok], src: &str, k: usize) -> bool {
    let Some(t) = toks.get(k) else {
        return false;
    };

    if t.kind != Kind::Op || text(src, t) != "=" {
        return false;
    }

    let glued_next = toks
        .get(k + 1)
        .is_some_and(|n| n.s == t.e && n.kind == Kind::Op && text(src, n) == "=");

    let glued_prev = k > 0 && {
        let p = &toks[k - 1];
        p.e == t.s
            && p.kind == Kind::Op
            && matches!(
                text(src, p),
                "=" | "<" | ">" | "~" | "+" | "-" | "*" | "/" | "%" | "^"
            )
    };

    !glued_next && !glued_prev
}

/// The index of the `}` that closes the `{` at `open`
fn close_brace(toks: &[Tok], src: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;

    for (k, t) in toks.iter().enumerate().skip(open) {
        if t.kind == Kind::Op {
            match text(src, t) {
                "{" => depth += 1,
                "}" => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(k);
                    }
                }
                _ => {}
            }
        }
    }

    None
}

/// The index of the `)` that closes the `(` at `open`
fn close_paren(toks: &[Tok], src: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;

    for (k, t) in toks.iter().enumerate().skip(open) {
        if t.kind == Kind::Op {
            match text(src, t) {
                "(" => depth += 1,
                ")" => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(k);
                    }
                }
                _ => {}
            }
        }
    }

    None
}

/*
The index of the `end` that closes the block opening at `at`.

`function`, `if`, `do`, and `repeat` open a block. `while` and `for` do
not, because their own `do` opens it. `until` closes `repeat`, `end`
closes the rest, and `elseif`/`else` change nothing.
*/
fn block_end(toks: &[Tok], src: &str, at: usize) -> Option<usize> {
    let mut depth = 0i32;

    for (k, t) in toks.iter().enumerate().skip(at) {
        if t.kind != Kind::Ident {
            continue;
        }

        match text(src, t) {
            "function" | "if" | "do" | "repeat" => depth += 1,
            "end" | "until" => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }

    None
}

/// The string content of a quoted token, without its quotes
fn unquote<'a>(src: &'a str, t: &Tok) -> &'a str {
    let raw = text(src, t);

    raw.strip_prefix(['"', '\''])
        .and_then(|r| r.strip_suffix(['"', '\'']))
        .unwrap_or(raw)
}

const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

pub fn scan(src: &str) -> Model {
    let (toks, comments) = lex(src);
    let mut model = Model {
        comments,
        ..Default::default()
    };

    let is_kw = |t: &Tok| t.kind == Kind::Ident && KEYWORDS.contains(&text(src, t));

    /*
    Class discovery: a statement that binds a table to a name.

    `local C = {...}`, `C = {...}`, and either with `setmetatable({...}, _)`
    around the table. The name is the class, the table is its first context,
    and the top level keys of the table are its first members.
    */
    let mut defs: Vec<(usize, String, (u32, u32))> = Vec::new(); // class, member, span

    for k in 0..toks.len() {
        let t = &toks[k];

        if t.kind != Kind::Ident || is_kw(t) {
            continue;
        }

        /*
        `const` binds a name the way `local` does, and larvae reads it
        everywhere `local` is read. A class the author wrote with it
        went undiscovered, so every `function C.m()` of that class
        looked like an access of a private member rather than the
        declaration of one.
        */
        let after_local = k > 0
            && toks[k - 1].kind == Kind::Ident
            && matches!(text(src, &toks[k - 1]), "local" | "const");
        let statement_head = k == 0
            || after_local
            || (toks[k - 1].kind == Kind::Op
                && !matches!(
                    text(src, &toks[k - 1]),
                    "." | ":" | "," | "(" | "{" | "[" | "="
                ))
            || (toks[k - 1].kind == Kind::Ident
                && matches!(text(src, &toks[k - 1]), "do" | "then" | "else" | "end"));

        if !statement_head {
            continue;
        }

        if !assigns(&toks, src, k + 1) {
            continue;
        }

        // The right side: a table, or setmetatable around one.
        let mut rhs = k + 2;
        if toks
            .get(rhs)
            .is_some_and(|t| t.kind == Kind::Ident && text(src, t) == "setmetatable")
            && toks.get(rhs + 1).is_some_and(|t| text(src, t) == "(")
        {
            rhs += 2;
        }

        let Some(open) = toks.get(rhs) else {
            continue;
        };

        if !(open.kind == Kind::Op && text(src, open) == "{") {
            continue;
        }

        let Some(close) = close_brace(&toks, src, rhs) else {
            continue;
        };

        let name = text(src, t).to_string();

        /*
        `local self = setmetatable({}, Class)` is a constructor building
        an instance, not a class. The name is what tells the two apart,
        and `self` is never a class.
        */
        if name == "self" || model.class_named(&name).is_some() {
            continue;
        }

        let mut class = Class {
            name,
            ..Default::default()
        };
        class.contexts.push((open.s, toks[close].e));

        // The top level keys of the table are members.
        let mut depth = 0i32;
        let mut idx = rhs;
        while idx <= close {
            let t = &toks[idx];

            if t.kind == Kind::Op {
                match text(src, t) {
                    "{" => depth += 1,
                    "}" => depth -= 1,
                    "[" if depth == 1
                        && toks.get(idx + 1).map(|n| n.kind) == Some(Kind::Str)
                        && toks.get(idx + 2).is_some_and(|n| text(src, n) == "]")
                        && assigns(&toks, src, idx + 3) =>
                    {
                        let name = unquote(src, &toks[idx + 1]).to_string();
                        let span = (toks[idx + 1].s, toks[idx + 1].e);
                        defs.push((model.classes.len(), name, span));
                    }
                    _ => {}
                }
            }

            if depth == 1 && t.kind == Kind::Ident && !is_kw(t) && assigns(&toks, src, idx + 1) {
                defs.push((model.classes.len(), text(src, t).to_string(), (t.s, t.e)));
            }

            idx += 1;
        }

        model.classes.push(class);
    }

    /*
    Statement scans against the known names: added members, contexts,
    colon heads, and the metatable uses.
    */
    for k in 0..toks.len() {
        let t = &toks[k];

        if t.kind != Kind::Ident {
            continue;
        }

        // setmetatable(_, C): the class becomes a metatable here.
        if text(src, t) == "setmetatable" && toks.get(k + 1).is_some_and(|n| text(src, n) == "(") {
            if let Some(close) = close_paren(&toks, src, k + 1) {
                // The last two tokens before `)` read `, C` when the
                // metatable is a bare class name.
                if close >= 2
                    && toks[close - 2].kind == Kind::Op
                    && text(src, &toks[close - 2]) == ","
                    && toks[close - 1].kind == Kind::Ident
                {
                    let name = text(src, &toks[close - 1]);

                    if let Some(idx) = model.classes.iter().position(|c| c.name == name) {
                        let span = (toks[close - 1].s, toks[close - 1].e);
                        model.setmeta.push((idx, span));
                    }
                }
            }
            continue;
        }

        let Some(idx) = model.classes.iter().position(|c| c.name == text(src, t)) else {
            continue;
        };

        let prev_is = |what: &str| k > 0 && text(src, &toks[k - 1]) == what;

        // function C.m() / function C:m() / function C.sub.m()
        if k > 0 && toks[k - 1].kind == Kind::Ident && text(src, &toks[k - 1]) == "function" {
            let mut j = k + 1;
            let mut first: Option<(String, (u32, u32))> = None;
            let mut colon = false;

            while let Some(sep) = toks.get(j) {
                let s = text(src, sep);

                if s != "." && s != ":" {
                    break;
                }

                let Some(name) = toks.get(j + 1) else {
                    break;
                };

                if name.kind != Kind::Ident {
                    break;
                }

                if first.is_none() {
                    first = Some((text(src, name).to_string(), (name.s, name.e)));
                    colon = s == ":";
                }

                if s == ":" {
                    j += 2;
                    break;
                }

                j += 2;
            }

            if let Some((name, span)) = first {
                defs.push((idx, name, span));

                if colon {
                    model.colon_defs.push((idx, (t.s, toks[j - 1].e)));
                }

                if let Some(end) = block_end(&toks, src, k - 1) {
                    model.classes[idx]
                        .contexts
                        .push((toks[k - 1].s, toks[end].e));
                }
            }
            continue;
        }

        // C.m = rhs / C["m"] = rhs, at a statement head
        if prev_is(".") || prev_is(":") {
            continue; // someone else's member named like the class
        }

        let (member, after) = match toks.get(k + 1).map(|n| text(src, n)) {
            Some(".") if toks.get(k + 2).is_some_and(|n| n.kind == Kind::Ident) => {
                let n = &toks[k + 2];
                ((text(src, n).to_string(), (n.s, n.e)), k + 3)
            }

            Some("[")
                if toks.get(k + 2).map(|n| n.kind) == Some(Kind::Str)
                    && toks.get(k + 3).is_some_and(|n| text(src, n) == "]") =>
            {
                let n = &toks[k + 2];
                ((unquote(src, n).to_string(), (n.s, n.e)), k + 4)
            }

            _ => continue,
        };

        // Walk the rest of a dotted path: C.a.b = _ still defines `a`.
        let mut eq = after;
        while toks.get(eq).is_some_and(|n| text(src, n) == ".")
            && toks.get(eq + 1).is_some_and(|n| n.kind == Kind::Ident)
        {
            eq += 2;
        }

        if !assigns(&toks, src, eq) {
            continue;
        }

        defs.push((idx, member.0, member.1));

        // The right side can be a context: a function, or a table of the table.
        if let Some(rhs) = toks.get(eq + 1) {
            if rhs.kind == Kind::Ident && text(src, rhs) == "function" {
                if let Some(end) = block_end(&toks, src, eq + 1) {
                    model.classes[idx].contexts.push((rhs.s, toks[end].e));
                }
            }

            if rhs.kind == Kind::Op && text(src, rhs) == "{" {
                if let Some(close) = close_brace(&toks, src, eq + 1) {
                    model.classes[idx].contexts.push((rhs.s, toks[close].e));
                }
            }
        }
    }

    /*
    Doc privacy: a `@private` comment marks the next definition, when
    nothing but comments and space sits between the two.
    */
    let mut doc_private: Vec<(u32, u32)> = Vec::new(); // spans of marked defs
    let mut doc_lifecycle: Vec<(u32, u32)> = Vec::new();

    for &(cs, ce) in &model.comments {
        let text = &src[cs as usize..ce as usize];
        let marks_private = text.contains("@private");
        let marks_lifecycle = text.contains("@lifecycle");

        if !marks_private && !marks_lifecycle {
            continue;
        }

        let Some((_, _, span)) = defs
            .iter()
            .filter(|(_, _, s)| s.0 >= ce)
            .min_by_key(|(_, _, s)| s.0)
        else {
            continue;
        };

        let gap = &src[ce as usize..span.0 as usize];
        let mut clean = String::new();
        let mut pos = ce;

        for &(gs, ge) in &model.comments {
            if gs >= ce && ge <= span.0 {
                clean.push_str(&src[pos as usize..gs as usize]);
                pos = ge;
            }
        }
        clean.push_str(&src[pos as usize..span.0 as usize]);

        // The gap may hold the def's own head: `function C.` or `C.` etc.
        let head_only = clean
            .trim()
            .trim_start_matches("function")
            .trim()
            .trim_start_matches(|c: char| c.is_alphanumeric() || c == '_')
            .trim()
            .trim_start_matches(['.', ':', '['])
            .trim()
            .is_empty();

        if gap.len() < 200 && head_only {
            if marks_private {
                doc_private.push(*span);
            }

            if marks_lifecycle {
                doc_lifecycle.push(*span);
            }
        }
    }

    let def_spans: std::collections::HashSet<(u32, u32)> =
        defs.iter().map(|(_, _, s)| *s).collect();

    for (idx, name, span) in defs {
        let private = name.starts_with('_') || doc_private.contains(&span);
        let lifecycle = doc_lifecycle.contains(&span);

        model.classes[idx].members.entry(name).or_insert(Member {
            span,
            private,
            lifecycle,
        });
    }

    /*
    Accesses: every member read, write, and call in the file.
    */
    for k in 0..toks.len() {
        let t = &toks[k];

        if t.kind != Kind::Op {
            continue;
        }

        let (name, span) = match text(src, t) {
            "." => {
                // Not `..`, and the next token names a member.
                let glued_dot =
                    |o: Option<&Tok>| o.is_some_and(|n| n.kind == Kind::Op && text(src, n) == ".");

                if glued_dot(toks.get(k + 1)) || (k > 0 && glued_dot(toks.get(k - 1))) {
                    continue;
                }

                let Some(n) = toks.get(k + 1) else { continue };

                if n.kind != Kind::Ident || is_kw(n) {
                    continue;
                }

                (text(src, n).to_string(), (n.s, n.e))
            }

            ":" => {
                let glued_colon =
                    |o: Option<&Tok>| o.is_some_and(|n| n.kind == Kind::Op && text(src, n) == ":");

                if glued_colon(toks.get(k + 1)) || (k > 0 && glued_colon(toks.get(k - 1))) {
                    continue;
                }

                let Some(n) = toks.get(k + 1) else { continue };

                if n.kind != Kind::Ident || is_kw(n) {
                    continue;
                }

                // A method needs its call; `x: Type` annotations have none.
                let calls = toks.get(k + 2).is_some_and(|c| {
                    matches!(c.kind, Kind::Str)
                        || (c.kind == Kind::Op && matches!(text(src, c), "(" | "{"))
                });

                let heads = k > 0 && def_spans.contains(&(n.s, n.e));

                if !calls && !heads {
                    continue;
                }

                (text(src, n).to_string(), (n.s, n.e))
            }

            "[" => {
                let Some(n) = toks.get(k + 1) else { continue };

                if n.kind != Kind::Str || !toks.get(k + 2).is_some_and(|c| text(src, c) == "]") {
                    continue;
                }

                (unquote(src, n).to_string(), (n.s, n.e))
            }

            _ => continue,
        };

        // The receiver: the token before the accessor.
        let Some(p) = k.checked_sub(1).map(|i| &toks[i]) else {
            continue;
        };

        let recv = match p.kind {
            Kind::Ident if !is_kw(p) => {
                let chained = k >= 2
                    && toks[k - 2].kind == Kind::Op
                    && matches!(text(src, &toks[k - 2]), "." | ":");

                match (chained, text(src, p)) {
                    (true, _) => Recv::Complex,
                    (false, "self") => Recv::Zelf,
                    (false, other) => Recv::Named(other.to_string()),
                }
            }

            Kind::Op if matches!(text(src, p), ")" | "]" | "}") => Recv::Complex,
            Kind::Str => Recv::Complex,

            // `{`, `,`, `=`, `(`: a table key or an opening bracket, not an access.
            _ => continue,
        };

        let write_at = match text(src, t) {
            "[" => k + 3,
            _ => k + 2,
        };

        model.accesses.push(Access {
            def_head: def_spans.contains(&span),
            write: assigns(&toks, src, write_at),
            name,
            span,
            recv,
        });
    }

    /*
    The members a context writes join the class: `obj._hp = 0` inside
    `Class.new` declares `_hp` on Class, whatever the receiver is called.
    */
    let mut grown: Vec<(usize, String, (u32, u32))> = Vec::new();

    for access in &model.accesses {
        if !access.write || !access.name.starts_with('_') {
            continue;
        }

        let Some(class) = model.context_at(access.span.0) else {
            continue;
        };

        if !class.members.contains_key(&access.name) {
            let idx = model
                .classes
                .iter()
                .position(|c| std::ptr::eq(c, class))
                .expect("the context class is in the list");

            grown.push((idx, access.name.clone(), access.span));
        }
    }

    for (idx, name, span) in grown {
        // A member a write grew carries no doc comment of its own.
        model.classes[idx].members.entry(name).or_insert(Member {
            span,
            private: true,
            lifecycle: false,
        });
    }

    model
}
