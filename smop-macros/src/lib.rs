//! Procedural macros for `smop`.
//!
//! Do not use this crate directly. Enable the `macros` feature of `smop`
//! and call `smop::cmd!`.
//!
//! The crate has no dependencies. It uses only the `proc_macro` API.
//!
//! The work is in four steps, one module for each:
//!
//! 1. `decode` gets the value of the template literal.
//! 2. `lex` splits the value into words.
//! 3. `validate` applies the grammar and gives a `Template`.
//! 4. `expand` writes the builder calls for the template.
//!
//! The first three steps are plain functions over `&str`, so their tests
//! need no `proc_macro` types.

use proc_macro::{Delimiter, Ident, Literal, Span, TokenStream, TokenTree};

mod decode;
mod expand;
mod lex;
mod validate;

/// The implementation of `smop::cmd!`. Call that macro, not this one.
///
/// The input is `$crate`, the template literal, then each listed name.
#[doc(hidden)]
#[proc_macro]
pub fn __cmd(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter();
    let (Some(krate), Some(template)) = (tokens.next(), tokens.next()) else {
        return expand::error(Span::call_site(), "call `smop::cmd!`, not `__cmd!`");
    };
    let mut listed = Vec::new();
    for token in tokens {
        match token {
            TokenTree::Ident(name) => listed.push(name),
            other => return expand::error(other.span(), "call `smop::cmd!`, not `__cmd!`"),
        }
    }

    let group_span = template.span();
    let Some(literal) = literal_of(template) else {
        return expand::error(group_span, "the template must be a string literal");
    };
    // The span of the literal is at the call site. The span of a group
    // around it is in the macro that forwards it.
    let span = literal.span();
    match parse(&literal, &listed) {
        Ok(template) => expand::template(&krate, span, &template, &listed),
        Err(message) => expand::error(span, &message),
    }
}

/// Gets the literal from the template token.
///
/// A literal that a `macro_rules!` macro forwards is in a group with no
/// delimiter. That group can be in one more such group for each macro.
fn literal_of(token: TokenTree) -> Option<Literal> {
    match token {
        TokenTree::Literal(literal) => Some(literal),
        TokenTree::Group(group) if group.delimiter() == Delimiter::None => {
            let mut inner = group.stream().into_iter();
            let only = inner.next()?;
            inner.next().is_none().then(|| literal_of(only))?
        }
        _ => None,
    }
}

/// Changes the source text of the template literal into a valid template.
fn parse(literal: &Literal, listed: &[Ident]) -> Result<validate::Template, String> {
    let names: Vec<String> = listed.iter().map(ToString::to_string).collect();
    let names: Vec<&str> = names.iter().map(|name| expand::plain(name)).collect();
    let value = decode::decode(&literal.to_string())?;
    validate::validate(lex::lex(&value)?, &names)
}
