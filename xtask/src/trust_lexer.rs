use super::construct::Construct;
use tokenizer::tokenize;

#[path = "trust_lexer/tokenizer.rs"]
mod tokenizer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Occurrence {
    pub(super) construct: Construct,
    pub(super) line: usize,
    pub(super) nested_item_scope: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenKind<'source> {
    Identifier(&'source str),
    Punctuation(u8),
    AllowInlineAir,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Token<'source> {
    kind: TokenKind<'source>,
    line: usize,
}

pub(super) fn scan(source: &str) -> Vec<Occurrence> {
    let tokens = tokenize(source);
    let mut occurrences = Vec::new();
    let mut scope = ScopeTracker::default();

    for (index, token) in tokens.iter().enumerate() {
        if let Some(construct) = prohibited_import_construct(&tokens, index) {
            occurrences.push(Occurrence {
                construct,
                line: token.line,
                nested_item_scope: scope.has_nested_item(),
            });
            continue;
        }
        if inside_use_statement(&tokens, index) {
            continue;
        }
        let construct = match token.kind {
            TokenKind::Identifier(identifier) => {
                call_construct(&tokens, index).or_else(|| identifier_construct(identifier))
            }
            TokenKind::AllowInlineAir => Some(Construct::AllowInlineAir),
            TokenKind::Punctuation(_) => None,
        };
        if let Some(construct) = construct {
            occurrences.push(Occurrence {
                construct,
                line: token.line,
                nested_item_scope: scope.has_nested_item(),
            });
        }

        if let Some(construct) = attribute_construct(&tokens, index) {
            occurrences.push(Occurrence {
                construct,
                line: token.line,
                nested_item_scope: scope.has_nested_item(),
            });
        }
        scope.observe(token);
    }

    occurrences
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScopeKind {
    Function,
    NestedItem,
    Other,
}

#[derive(Default)]
struct ScopeTracker {
    pending: Option<ScopeKind>,
    stack: Vec<ScopeKind>,
}

impl ScopeTracker {
    fn observe(&mut self, token: &Token<'_>) {
        match token.kind {
            TokenKind::Identifier("fn") => self.pending = Some(ScopeKind::Function),
            TokenKind::Identifier("mod" | "impl" | "trait" | "struct" | "enum" | "union") => {
                self.pending = Some(ScopeKind::NestedItem);
            }
            TokenKind::Punctuation(b'{') => {
                self.stack.push(self.pending.take().unwrap_or(ScopeKind::Other));
            }
            TokenKind::Punctuation(b'}') => {
                self.stack.pop();
                self.pending = None;
            }
            TokenKind::Punctuation(b';') => self.pending = None,
            _ => {}
        }
    }

    fn has_nested_item(&self) -> bool {
        self.stack.contains(&ScopeKind::NestedItem)
    }
}

fn identifier_construct(identifier: &str) -> Option<Construct> {
    match identifier {
        "assume" => Some(Construct::Assume),
        "assume_" => Some(Construct::BuiltinAssume),
        "admit" => Some(Construct::Admit),
        "axiom" => Some(Construct::Axiom),
        "assume_specification" => Some(Construct::AssumeSpecification),
        "exec_spec_unverified" => Some(Construct::ExecSpecUnverified),
        "inline_air_stmt" => Some(Construct::InlineAirStatement),
        concat!("allow_", "inline_air") => Some(Construct::AllowInlineAir),
        _ => None,
    }
}

fn call_construct(tokens: &[Token<'_>], index: usize) -> Option<Construct> {
    let TokenKind::Identifier(method) = tokens[index].kind else { return None };
    if !matches!(method, "assume_new" | "assume_new_fallback") {
        return None;
    }
    let owner = tokens[..index].iter().rev().take(16).find_map(|token| match token.kind {
        TokenKind::Identifier("Ghost") => Some("Ghost"),
        TokenKind::Identifier("Tracked") => Some("Tracked"),
        TokenKind::Punctuation(b';' | b'{' | b'}' | b'=') => Some(""),
        _ => None,
    });
    match (owner, method) {
        (Some("Ghost"), "assume_new") => Some(Construct::GhostAssumeNew),
        (Some("Ghost"), "assume_new_fallback") => Some(Construct::GhostAssumeNewFallback),
        (Some("Tracked"), "assume_new") => Some(Construct::TrackedAssumeNew),
        (Some("Tracked"), "assume_new_fallback") => Some(Construct::TrackedAssumeNewFallback),
        (_, "assume_new") => Some(Construct::AssumeNew),
        (_, "assume_new_fallback") => Some(Construct::AssumeNewFallback),
        _ => unreachable!("method spelling was checked above"),
    }
}

fn prohibited_import_construct(tokens: &[Token<'_>], index: usize) -> Option<Construct> {
    let TokenKind::Identifier(identifier) = tokens[index].kind else { return None };
    let in_use = inside_use_statement(tokens, index);
    let in_type_alias = inside_type_alias(tokens, index);
    let exact_operation =
        matches!(identifier, "assume" | "assume_" | "admit" | "assume_new" | "assume_new_fallback");
    let aliasable_owner = matches!(identifier, "Ghost" | "Tracked");
    let statement = statement_tokens(tokens, index);
    let glob = statement.iter().any(|token| matches!(token.kind, TokenKind::Punctuation(b'*')));
    let public = statement.iter().any(|token| matches!(token.kind, TokenKind::Identifier("pub")));
    let trusted_glob =
        (identifier == "pervasive" && glob) || (identifier == "prelude" && glob && public);
    ((in_use && (exact_operation || aliasable_owner || trusted_glob))
        || (in_type_alias && aliasable_owner))
        .then_some(Construct::ProhibitedTrustedImport)
}

fn inside_use_statement(tokens: &[Token<'_>], index: usize) -> bool {
    tokens[..index]
        .iter()
        .rev()
        .take_while(|token| !matches!(token.kind, TokenKind::Punctuation(b';')))
        .any(|token| matches!(token.kind, TokenKind::Identifier("use")))
}

fn inside_type_alias(tokens: &[Token<'_>], index: usize) -> bool {
    let statement = statement_tokens(tokens, index);
    statement.iter().any(|token| matches!(token.kind, TokenKind::Identifier("type")))
        && statement.iter().any(|token| matches!(token.kind, TokenKind::Punctuation(b'=')))
}

fn statement_tokens<'a>(tokens: &'a [Token<'a>], index: usize) -> &'a [Token<'a>] {
    let start = tokens[..index]
        .iter()
        .rposition(|token| matches!(token.kind, TokenKind::Punctuation(b';')))
        .map_or(0, |position| position + 1);
    let end = tokens[index..]
        .iter()
        .position(|token| matches!(token.kind, TokenKind::Punctuation(b';')))
        .map_or(tokens.len(), |offset| index + offset);
    &tokens[start..end]
}

fn attribute_construct(tokens: &[Token<'_>], index: usize) -> Option<Construct> {
    let TokenKind::Identifier(identifier) = tokens[index].kind else { return None };
    if !inside_attribute(tokens, index) {
        return None;
    }
    match identifier {
        "external" => Some(Construct::External),
        "external_body" => Some(Construct::ExternalBody),
        "external_fn_specification" => Some(Construct::ExternalFunctionSpecification),
        "external_type_specification" => Some(Construct::ExternalTypeSpecification),
        "external_trait_specification" => Some(Construct::ExternalTraitSpecification),
        "external_trait_extension" => Some(Construct::ExternalTraitExtension),
        "external_trait_private_bound" => Some(Construct::ExternalTraitPrivateBound),
        "external_derive" => Some(Construct::ExternalDerive),
        "external_trait_blanket" => Some(Construct::ExternalTraitBlanket),
        "trusted" if path_prefix_is(tokens, index, "verus") => Some(Construct::Trusted),
        "assume_termination" if path_prefix_is(tokens, index, "verifier") => {
            Some(Construct::AssumeTermination)
        }
        "exec_allows_no_decreases_clause" if path_prefix_is(tokens, index, "verifier") => {
            Some(Construct::ExecAllowsNoDecreases)
        }
        _ => None,
    }
}

fn path_prefix_is(tokens: &[Token<'_>], index: usize, expected: &str) -> bool {
    index >= 3
        && matches!(tokens[index - 3].kind, TokenKind::Identifier(prefix) if prefix == expected)
        && matches!(tokens[index - 2].kind, TokenKind::Punctuation(b':'))
        && matches!(tokens[index - 1].kind, TokenKind::Punctuation(b':'))
}

fn inside_attribute(tokens: &[Token<'_>], index: usize) -> bool {
    let mut depth = 0;
    for cursor in (0..index).rev() {
        match tokens[cursor].kind {
            TokenKind::Punctuation(b']') => depth += 1,
            TokenKind::Punctuation(b'[') if depth == 0 => {
                let hash = cursor.checked_sub(1).filter(|position| {
                    matches!(tokens[*position].kind, TokenKind::Punctuation(b'#'))
                });
                let inner_hash = cursor.checked_sub(2).filter(|position| {
                    matches!(tokens[*position].kind, TokenKind::Punctuation(b'#'))
                        && matches!(tokens[cursor - 1].kind, TokenKind::Punctuation(b'!'))
                });
                return hash.is_some() || inner_hash.is_some();
            }
            TokenKind::Punctuation(b'[') => depth -= 1,
            _ => {}
        }
    }
    false
}
