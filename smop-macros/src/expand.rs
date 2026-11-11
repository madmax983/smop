//! Writes the tokens that `cmd!` expands to.
//!
//! The output names only `$crate` paths and `::core::compile_error!`, so a
//! script needs no dependency on this crate.

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

use crate::validate::{Arg, Part, Template};

/// The number of `&` before a value wrapper. Each impl of one method is
/// for a different number, and method lookup tries the largest number first.
const ARG_REFS: usize = 3;
const SPLAT_REFS: usize = 8;

/// Keywords of some editions only. A local with such a name is a raw
/// identifier in the later editions.
const EDITION_KEYWORDS: [&str; 5] = ["async", "await", "dyn", "gen", "try"];

/// Writes `::core::compile_error!("cmd!: <message>")` at `span`.
///
/// The tokens have the hygiene of this crate. So `::core` resolves by the
/// rules of this edition, also when the caller is an edition 2015 crate.
pub fn error(span: Span, message: &str) -> TokenStream {
    let span = Span::mixed_site().located_at(span);
    let mut out = Tokens::new(span);
    out.colons();
    out.ident("core");
    out.colons();
    out.ident("compile_error");
    out.punct('!');
    let mut text = Tokens::new(span);
    text.literal(&format!("cmd!: {message}"));
    out.group(Delimiter::Parenthesis, text);
    out.stream
}

/// Writes the builder calls for a template.
///
/// `krate` is the `$crate` token of `smop`. `literal` is the span of the
/// template. `listed` holds the names of the `name = value` arguments.
pub fn template(
    krate: &TokenTree,
    literal: Span,
    template: &Template,
    listed: &[Ident],
) -> TokenStream {
    let expand = Expand {
        krate,
        literal,
        // Rustc reports an error at the template. The hygiene is that of
        // a macro, so lints for user code do not check these tokens.
        span: Span::mixed_site().located_at(literal),
        listed,
    };

    let has_text_only = |parts: &[Part]| parts.iter().all(|part| matches!(part, Part::Text(_)));
    let has_placeholder = !has_text_only(&template.program)
        || template.args.iter().any(|arg| match arg {
            Arg::Word(parts) => !has_text_only(parts),
            Arg::Splat(_) => true,
        });

    let mut body = expand.tokens();
    if has_placeholder {
        expand.import(&mut body);
    }

    expand.krate_path(&mut body, &["sh", "cmd"]);
    body.group(Delimiter::Parenthesis, expand.word(&template.program));
    for arg in &template.args {
        body.punct('.');
        match arg {
            Arg::Word(parts) => {
                body.ident("arg");
                body.group(Delimiter::Parenthesis, expand.word(parts));
            }
            Arg::Splat(name) => {
                body.ident("args");
                body.group(Delimiter::Parenthesis, expand.splat(name));
            }
        }
    }

    let mut out = expand.tokens();
    out.group(Delimiter::Brace, body);
    out.stream
}

/// The values that each part of an expansion needs.
struct Expand<'a> {
    krate: &'a TokenTree,
    literal: Span,
    span: Span,
    listed: &'a [Ident],
}

impl Expand<'_> {
    fn tokens(&self) -> Tokens {
        Tokens::new(self.span)
    }

    /// Writes `$crate::a::b`.
    fn krate_path(&self, out: &mut Tokens, path: &[&str]) {
        out.stream.extend([self.krate.clone()]);
        for segment in path {
            out.colons();
            out.ident(segment);
        }
    }

    /// Writes `$crate::__private::<name>`.
    fn private(&self, out: &mut Tokens, name: &str) {
        self.krate_path(out, &["__private", name]);
    }

    /// Writes `use $crate::__private::traits::*;`.
    ///
    /// Each placeholder uses the `Value` trait of that module, so the
    /// import is never unused. The module holds only traits, and a trait
    /// name cannot hide a local of the caller.
    fn import(&self, out: &mut Tokens) {
        out.ident("use");
        self.private(out, "traits");
        out.colons();
        out.punct('*');
        out.punct(';');
    }

    /// Writes the expression for one word.
    fn word(&self, parts: &[Part]) -> Tokens {
        let mut out = self.tokens();
        match parts {
            [] => out.literal(""),
            [Part::Text(text)] => out.literal(text),
            [Part::Var(name)] => self.value(&mut out, name),
            parts => {
                // `word(&[&"--out=", &<value>])`
                let mut items = self.tokens();
                for part in parts {
                    items.punct('&');
                    match part {
                        Part::Text(text) => items.literal(text),
                        Part::Var(name) => self.value(&mut items, name),
                    }
                    items.punct(',');
                }
                let mut list = self.tokens();
                list.punct('&');
                list.group(Delimiter::Bracket, items);
                self.private(&mut out, "word");
                out.group(Delimiter::Parenthesis, list);
            }
        }
        out
    }

    /// Writes `(&&&name.__smop_cmd_value()).__smop_cmd_arg()`.
    fn value(&self, out: &mut Tokens, name: &str) {
        self.dispatch(out, name, "__smop_cmd_value", ARG_REFS, "__smop_cmd_arg");
    }

    /// Writes `(&&&&&&&&name.__smop_cmd_list()).__smop_cmd_splat()`.
    fn splat(&self, name: &str) -> Tokens {
        let mut out = self.tokens();
        self.dispatch(
            &mut out,
            name,
            "__smop_cmd_list",
            SPLAT_REFS,
            "__smop_cmd_splat",
        );
        out
    }

    /// Writes the two method calls that convert the value of a name.
    ///
    /// The first call is on the value itself. That call is an error when
    /// the type of the value is not yet known. So the second call never
    /// selects an impl for an unknown type.
    fn dispatch(&self, out: &mut Tokens, name: &str, wrap: &str, refs: usize, convert: &str) {
        let mut receiver = self.tokens();
        for _ in 0..refs {
            receiver.punct('&');
        }
        self.place(&mut receiver, name);
        receiver.punct('.');
        receiver.ident(wrap);
        receiver.group(Delimiter::Parenthesis, self.tokens());

        out.group(Delimiter::Parenthesis, receiver);
        out.punct('.');
        out.ident(convert);
        out.group(Delimiter::Parenthesis, self.tokens());
    }

    /// Writes the expression for the value of a template name.
    ///
    /// A listed name is the token that the caller wrote, so it finds the
    /// binding of the wrapper. That binding is a reference, so the
    /// expression is `(*name)`. A captured name gets the span of the
    /// template, so it finds a local at the call site.
    fn place(&self, out: &mut Tokens, name: &str) {
        let listed = self
            .listed
            .iter()
            .find(|listed| plain(&listed.to_string()) == name);
        if let Some(listed) = listed {
            let mut deref = self.tokens();
            deref.punct('*');
            deref.stream.extend([TokenTree::Ident(listed.clone())]);
            out.group(Delimiter::Parenthesis, deref);
        } else {
            let captured = if EDITION_KEYWORDS.contains(&name) {
                Ident::new_raw(name, self.literal)
            } else {
                Ident::new(name, self.literal)
            };
            out.stream.extend([TokenTree::Ident(captured)]);
        }
    }
}

/// Removes the `r#` of a raw identifier.
pub fn plain(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

/// A token stream in which each new token gets the same span.
struct Tokens {
    stream: TokenStream,
    span: Span,
}

impl Tokens {
    fn new(span: Span) -> Self {
        Self {
            stream: TokenStream::new(),
            span,
        }
    }

    fn push(&mut self, mut token: TokenTree) {
        token.set_span(self.span);
        self.stream.extend([token]);
    }

    fn ident(&mut self, name: &str) {
        self.push(Ident::new(name, self.span).into());
    }

    fn punct(&mut self, c: char) {
        self.push(Punct::new(c, Spacing::Alone).into());
    }

    fn colons(&mut self) {
        self.push(Punct::new(':', Spacing::Joint).into());
        self.push(Punct::new(':', Spacing::Alone).into());
    }

    fn literal(&mut self, text: &str) {
        self.push(Literal::string(text).into());
    }

    fn group(&mut self, delimiter: Delimiter, inner: Self) {
        self.push(Group::new(delimiter, inner.stream).into());
    }
}
