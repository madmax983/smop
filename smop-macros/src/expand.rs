//! Writes the tokens that `cmd!` expands to.
//!
//! The output names only `$crate` paths and `::core::compile_error!`, so a
//! script needs no dependency on this crate.

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

use crate::validate::{Arg, Part, Template};

/// The traits that select how a `{name}` value becomes an argument.
const ARG_TRAITS: [&str; 2] = ["ArgValue", "ArgOther"];

/// The traits that select how a `{name...}` value becomes arguments.
const SPLAT_TRAITS: [&str; 3] = ["SplatPath", "SplatValue", "SplatOther"];

/// The number of `&` before a value wrapper. Each trait of one method is
/// for a different number, and the largest number wins.
const ARG_REFS: usize = 3;
const SPLAT_REFS: usize = 6;

/// Writes `::core::compile_error!("cmd!: <message>")` at `span`.
pub fn error(span: Span, message: &str) -> TokenStream {
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

    let words =
        || std::iter::once(&template.program).chain(template.args.iter().filter_map(Arg::word));
    let has_value = words().flatten().any(|part| matches!(part, Part::Var(_)));
    let has_splat = template.args.iter().any(|arg| matches!(arg, Arg::Splat(_)));

    let mut body = expand.tokens();
    if has_value {
        expand.import(&mut body, &ARG_TRAITS);
    }
    if has_splat {
        expand.import(&mut body, &SPLAT_TRAITS);
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

    /// Writes `#[allow(unused_imports)] use $crate::__private::<name> as _;`
    /// for each name.
    ///
    /// Method lookup uses only one trait of a group for a given type, so
    /// the other imports are not used.
    fn import(&self, out: &mut Tokens, traits: &[&str]) {
        for name in traits {
            let mut lint = self.tokens();
            lint.ident("unused_imports");
            let mut attribute = self.tokens();
            attribute.ident("allow");
            attribute.group(Delimiter::Parenthesis, lint);
            out.punct('#');
            out.group(Delimiter::Bracket, attribute);
            out.ident("use");
            self.private(out, name);
            out.ident("as");
            out.ident("_");
            out.punct(';');
        }
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

    /// Writes `(&&&$crate::__private::Arg(&name)).__smop_cmd_arg()`.
    fn value(&self, out: &mut Tokens, name: &str) {
        self.dispatch(out, name, "Arg", ARG_REFS, "__smop_cmd_arg");
    }

    /// Writes `(&&&&&&$crate::__private::Splat(&name)).__smop_cmd_splat()`.
    fn splat(&self, name: &str) -> Tokens {
        let mut out = self.tokens();
        self.dispatch(&mut out, name, "Splat", SPLAT_REFS, "__smop_cmd_splat");
        out
    }

    fn dispatch(&self, out: &mut Tokens, name: &str, wrapper: &str, refs: usize, method: &str) {
        let mut borrowed = self.tokens();
        borrowed.punct('&');
        borrowed.stream.extend([TokenTree::Ident(self.name(name))]);

        let mut receiver = self.tokens();
        for _ in 0..refs {
            receiver.punct('&');
        }
        self.private(&mut receiver, wrapper);
        receiver.group(Delimiter::Parenthesis, borrowed);

        out.group(Delimiter::Parenthesis, receiver);
        out.punct('.');
        out.ident(method);
        out.group(Delimiter::Parenthesis, self.tokens());
    }

    /// Makes the identifier for a template name.
    ///
    /// A listed name is the token that the caller wrote, so it finds the
    /// binding of the wrapper. A captured name gets the span of the
    /// template, so it finds a local at the call site.
    fn name(&self, name: &str) -> Ident {
        self.listed
            .iter()
            .find(|listed| listed.to_string() == name)
            .cloned()
            .unwrap_or_else(|| Ident::new(name, self.literal))
    }
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
