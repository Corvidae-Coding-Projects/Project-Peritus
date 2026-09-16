use super::{Token, TokenKind, tokenize};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DeclarationOwner {
    Module(String),
    Trait(String),
    Impl(Vec<String>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Scope {
    Function,
    Owner(DeclarationOwner),
    UnsupportedImpl,
    Transparent,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TypeBinding {
    Nominal,
    Import(Vec<String>),
    Unsupported,
}

#[derive(Default)]
struct Declarations {
    functions: Vec<(Vec<DeclarationOwner>, usize)>,
    bindings: Vec<(Vec<String>, String, TypeBinding)>,
}

/// Keeps exact same-name declaration ordinals, including declarations on the same line.
/// Impl locators retain their self-type owner; trait declarations retain their own namespace.
pub(crate) fn declaration_paths(source: &str, item: &str) -> Vec<(Vec<DeclarationOwner>, usize)> {
    declarations(source, item).functions
}

pub(crate) fn type_bindings(source: &str, modules: &[String], name: &str) -> Vec<TypeBinding> {
    declarations(source, "")
        .bindings
        .into_iter()
        .filter_map(|(owner, item, binding)| (owner == modules && item == name).then_some(binding))
        .collect()
}

fn declarations(source: &str, item: &str) -> Declarations {
    let tokens = tokenize(source);
    let mut result = Declarations::default();
    let mut scopes = Vec::new();
    let mut pending = None;
    let mut impl_header = None;
    let mut ordinal = 0;
    let mut skip_until = 0;
    for (index, token) in tokens.iter().enumerate() {
        if index < skip_until {
            continue;
        }
        if let Some(end) = non_item_end(&tokens, index) {
            ordinal += tokens[index..end]
                .windows(2)
                .filter(|pair| {
                    pair[0].kind == TokenKind::Identifier("fn")
                        && identifier(Some(&pair[1])) == Some(item)
                })
                .count();
            skip_until = end;
            continue;
        }
        match token.kind {
            TokenKind::Identifier("use") => {
                let end = tokens[index..].iter().position(|token| punct(token, b';'));
                if let Some(end) = end {
                    skip_until = index + end + 1;
                    if let Some(modules) = module_scope(&scopes) {
                        for (name, path) in imports(&tokens[index + 1..index + end], &[]) {
                            result.bindings.push((
                                modules.clone(),
                                name,
                                TypeBinding::Import(path),
                            ));
                        }
                    }
                }
            }
            TokenKind::Identifier("impl") => {
                impl_header = Some(index + 1);
                pending = None;
            }
            TokenKind::Identifier("fn") => {
                if identifier(tokens.get(index + 1)) == Some(item) {
                    if let Some(path) = owner_scope(&scopes) {
                        result.functions.push((path, ordinal));
                    }
                    ordinal += 1;
                }
                pending = Some(Scope::Function);
            }
            TokenKind::Identifier("mod" | "trait") if impl_header.is_none() => {
                pending = identifier(tokens.get(index + 1)).map(|name| {
                    Scope::Owner(if token.kind == TokenKind::Identifier("mod") {
                        DeclarationOwner::Module(name.to_owned())
                    } else {
                        DeclarationOwner::Trait(name.to_owned())
                    })
                });
            }
            TokenKind::Identifier("struct" | "enum" | "union" | "type")
                if impl_header.is_none() =>
            {
                if let (Some(modules), Some(name)) =
                    (module_scope(&scopes), identifier(tokens.get(index + 1)))
                {
                    let binding = if token.kind == TokenKind::Identifier("type") {
                        TypeBinding::Unsupported
                    } else {
                        TypeBinding::Nominal
                    };
                    result.bindings.push((modules, name.to_owned(), binding));
                }
                pending = Some(Scope::Other);
            }
            TokenKind::Punctuation(b'{') => {
                scopes.push(opening_scope(&tokens, index, pending.take(), impl_header.take()));
            }
            TokenKind::Punctuation(b'}') => {
                scopes.pop();
                pending = None;
                impl_header = None;
            }
            TokenKind::Punctuation(b';') => {
                pending = None;
                impl_header = None;
            }
            _ => {}
        }
    }
    result
}

fn opening_scope(
    tokens: &[Token<'_>],
    index: usize,
    pending: Option<Scope>,
    impl_header: Option<usize>,
) -> Scope {
    if let Some(start) = impl_header {
        return impl_owner(&tokens[start..index])
            .map_or(Scope::UnsupportedImpl, |owner| Scope::Owner(DeclarationOwner::Impl(owner)));
    }
    pending.unwrap_or_else(|| {
        if index >= 2
            && punct(&tokens[index - 1], b'!')
            && tokens[index - 2].kind == TokenKind::Identifier("verus")
        {
            Scope::Transparent
        } else {
            Scope::Other
        }
    })
}

fn non_item_end(tokens: &[Token<'_>], index: usize) -> Option<usize> {
    let open = match tokens[index].kind {
        TokenKind::Punctuation(b'#') => {
            let offset =
                if tokens.get(index + 1).is_some_and(|token| punct(token, b'!')) { 2 } else { 1 };
            let open = index + offset;
            if !tokens.get(open).is_some_and(|token| punct(token, b'[')) {
                return None;
            }
            open
        }
        TokenKind::Identifier(name)
            if tokens.get(index + 1).is_some_and(|token| punct(token, b'!')) =>
        {
            let open = index + if name == "macro_rules" { 3 } else { 2 };
            if name == "verus" && tokens.get(open).is_some_and(|token| punct(token, b'{')) {
                return None;
            }
            if !tokens.get(open).is_some_and(|token| {
                matches!(token.kind, TokenKind::Punctuation(b'(' | b'[' | b'{'))
            }) {
                return None;
            }
            open
        }
        _ => return None,
    };
    Some(token_tree_end(tokens, open).unwrap_or(tokens.len()))
}

fn token_tree_end(tokens: &[Token<'_>], open: usize) -> Option<usize> {
    let mut closing = Vec::new();
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            TokenKind::Punctuation(b'(') => closing.push(b')'),
            TokenKind::Punctuation(b'[') => closing.push(b']'),
            TokenKind::Punctuation(b'{') => closing.push(b'}'),
            TokenKind::Punctuation(value @ (b')' | b']' | b'}')) => {
                if closing.pop() != Some(value) {
                    return None;
                }
                if closing.is_empty() {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn owner_scope(scopes: &[Scope]) -> Option<Vec<DeclarationOwner>> {
    let mut path = Vec::new();
    let mut associated = false;
    for scope in scopes {
        match scope {
            Scope::Function | Scope::UnsupportedImpl | Scope::Other => return None,
            Scope::Owner(owner) => {
                if associated {
                    return None;
                }
                associated = !matches!(owner, DeclarationOwner::Module(_));
                path.push(owner.clone());
            }
            Scope::Transparent => {}
        }
    }
    Some(path)
}

fn module_scope(scopes: &[Scope]) -> Option<Vec<String>> {
    owner_scope(scopes)?
        .into_iter()
        .map(|owner| match owner {
            DeclarationOwner::Module(name) => Some(name),
            DeclarationOwner::Trait(_) | DeclarationOwner::Impl(_) => None,
        })
        .collect()
}

fn identifier<'a>(token: Option<&Token<'a>>) -> Option<&'a str> {
    match token?.kind {
        TokenKind::Identifier(name) => Some(name),
        _ => None,
    }
}

fn punct(token: &Token<'_>, value: u8) -> bool {
    token.kind == TokenKind::Punctuation(value)
}

fn path(tokens: &[Token<'_>]) -> Option<Vec<String>> {
    let mut result = Vec::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        result.push(identifier(tokens.get(cursor))?.to_owned());
        cursor += 1;
        if cursor == tokens.len() {
            break;
        }
        if !tokens.get(cursor..cursor + 2)?.iter().all(|token| punct(token, b':')) {
            return None;
        }
        cursor += 2;
        if cursor == tokens.len() {
            return None;
        }
    }
    (!result.is_empty()).then_some(result)
}

fn impl_owner(tokens: &[Token<'_>]) -> Option<Vec<String>> {
    let header_start = if tokens.first().is_some_and(|token| punct(token, b'<')) {
        after_generics(tokens, 0)?
    } else {
        0
    };
    let mut start = header_start;
    let mut cursor = header_start;
    while cursor < tokens.len() {
        if punct(&tokens[cursor], b'<') {
            cursor = after_generics(tokens, cursor)?;
            continue;
        }
        match tokens[cursor].kind {
            TokenKind::Identifier("where") => break,
            TokenKind::Identifier("for") => {
                start = cursor + 1;
                break;
            }
            _ => cursor += 1,
        }
    }
    let end = tokens[start..]
        .iter()
        .position(|token| punct(token, b'<') || token.kind == TokenKind::Identifier("where"))
        .map_or(tokens.len(), |offset| start + offset);
    let owner = path(&tokens[start..end])?;
    // Ordinary trait impl locators retain their self-type owner. The separate compiler gate
    // still requires the exact selected symbol and query before accepting Verus evidence.
    let tail = if tokens.get(end).is_some_and(|token| punct(token, b'<')) {
        after_generics(tokens, end)?
    } else {
        end
    };
    if tail < tokens.len() && tokens[tail].kind != TokenKind::Identifier("where") {
        return None;
    }
    Some(owner)
}

fn after_generics(tokens: &[Token<'_>], start: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if punct(token, b'<') {
            depth += 1;
        }
        if punct(token, b'>') {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index + 1);
            }
        }
    }
    None
}

fn imports(tokens: &[Token<'_>], prefix: &[String]) -> Vec<(String, Vec<String>)> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for index in 0..=tokens.len() {
        if index == tokens.len() || (depth == 0 && punct(&tokens[index], b',')) {
            result.extend(import_tree(&tokens[start..index], prefix));
            start = index + 1;
        } else if punct(&tokens[index], b'{') {
            depth += 1;
        } else if punct(&tokens[index], b'}') {
            depth -= 1;
        }
    }
    result
}

fn import_tree(tokens: &[Token<'_>], prefix: &[String]) -> Vec<(String, Vec<String>)> {
    if let Some(open) = tokens.iter().position(|token| punct(token, b'{')) {
        if !tokens.last().is_some_and(|token| punct(token, b'}')) {
            return Vec::new();
        }
        let mut parent = prefix.to_vec();
        if open > 0 {
            let Some(head) = tokens.get(..open.saturating_sub(2)).and_then(path) else {
                return Vec::new();
            };
            if !tokens[open - 2..open].iter().all(|token| punct(token, b':')) {
                return Vec::new();
            }
            parent.extend(head);
        }
        return imports(&tokens[open + 1..tokens.len() - 1], &parent);
    }
    let split = tokens.iter().position(|token| token.kind == TokenKind::Identifier("as"));
    let Some(tail) = path(&tokens[..split.unwrap_or(tokens.len())]) else { return Vec::new() };
    let mut full = prefix.to_vec();
    if tail != ["self"] {
        full.extend(tail);
    }
    let alias = match split {
        Some(index) if index + 2 == tokens.len() => identifier(tokens.get(index + 1)),
        Some(_) => None,
        None => full.last().map(String::as_str),
    };
    alias.map_or_else(Vec::new, |name| vec![(name.to_owned(), full.clone())])
}
