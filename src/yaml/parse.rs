//! YAML text to a tree of nodes. Aliases are copies; merge keys are applied.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::scalar::{self, Kind};
use super::{Error, Mark};

/// Nesting deeper than this is an error, so no input can overflow the stack.
const MAX_DEPTH: usize = 128;

/// Aliases that copy more nodes than this are an error (alias bombs).
const MAX_ALIAS_NODES: usize = 1_000_000;

/// Aliases that copy more bytes of text than this are an error.
const MAX_ALIAS_BYTES: usize = 64 << 20;

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug)]
pub(super) struct Node {
    pub(super) mark: Mark,
    body: Body,
}

/// A node has its own content, or shares an anchored node. Aliases share
/// the node, so an alias costs no copy.
#[derive(Clone, Debug)]
enum Body {
    Own(NodeKind),
    Shared(Rc<Shared>),
}

/// An anchored node, with the counts that each alias of it adds.
#[derive(Debug)]
struct Shared {
    node: Node,
    size: usize,
    bytes: usize,
    height: usize,
}

#[derive(Clone, Debug)]
pub(super) enum NodeKind {
    Scalar(Scalar),
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>),
}

#[derive(Clone, Debug)]
pub(super) struct Scalar {
    pub(super) text: String,
    pub(super) kind: Kind,
    /// Unquoted and untagged. Only a plain `<<` is a merge key.
    pub(super) plain: bool,
}

impl Node {
    const fn new(mark: Mark, kind: NodeKind) -> Self {
        Self {
            mark,
            body: Body::Own(kind),
        }
    }

    const fn scalar(mark: Mark, text: String, kind: Kind, plain: bool) -> Self {
        Self::new(mark, NodeKind::Scalar(Scalar { text, kind, plain }))
    }

    /// The content, through anchors and aliases.
    pub(super) fn kind(&self) -> &NodeKind {
        match &self.body {
            Body::Own(kind) => kind,
            Body::Shared(shared) => shared.node.kind(),
        }
    }

    /// The pairs of a mapping. A shared mapping gives a copy.
    fn into_pairs(self) -> Option<Vec<(Self, Self)>> {
        match self.body {
            Body::Own(NodeKind::Map(pairs)) => Some(pairs),
            Body::Shared(shared) => match shared.node.kind() {
                NodeKind::Map(pairs) => Some(pairs.clone()),
                _ => None,
            },
            Body::Own(_) => None,
        }
    }

    fn plain(mark: Mark, text: String) -> Self {
        let kind = scalar::resolve_plain(&text);
        Self::scalar(mark, text, kind, true)
    }

    const fn null(mark: Mark) -> Self {
        Self::scalar(mark, String::new(), Kind::Null, true)
    }

    /// The number of nodes, the bytes of text and the height. A shared
    /// node gives its stored counts, so this stops at anchors.
    fn measure(&self) -> (usize, usize, usize) {
        let add = |(n, b, h): (usize, usize, usize), (n2, b2, h2): (usize, usize, usize)| {
            (n + n2, b + b2, h.max(h2))
        };
        match &self.body {
            Body::Shared(shared) => (shared.size, shared.bytes, shared.height),
            Body::Own(NodeKind::Scalar(s)) => (1, s.text.len(), 0),
            Body::Own(NodeKind::Seq(items)) => {
                let (n, b, h) = items.iter().map(Self::measure).fold((0, 0, 0), add);
                (n + 1, b, h + 1)
            }
            Body::Own(NodeKind::Map(pairs)) => {
                let (n, b, h) = pairs
                    .iter()
                    .map(|(k, v)| add(k.measure(), v.measure()))
                    .fold((0, 0, 0), add);
                (n + 1, b, h + 1)
            }
        }
    }
}

/// Parses one document. Line breaks may be `\n`, `\r\n` or `\r`.
pub(super) fn parse(text: &str) -> Result<Node> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut parser = Parser::new(&text);
    if let Some(i) = parser.src.iter().position(|&c| !scalar::printable(c)) {
        parser.advance(i);
        let code = u32::from(parser.src[i]);
        return parser.err(format!("the text has a control character (U+{code:04X})"));
    }
    parser.document()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tag {
    NonSpecific,
    Str,
    Int,
    Float,
    Bool,
    Null,
    Seq,
    Map,
}

impl Tag {
    const fn name(self) -> &'static str {
        match self {
            Self::NonSpecific => "!",
            Self::Str => "!!str",
            Self::Int => "!!int",
            Self::Float => "!!float",
            Self::Bool => "!!bool",
            Self::Null => "!!null",
            Self::Seq => "!!seq",
            Self::Map => "!!map",
        }
    }
}

#[derive(Default)]
struct Props {
    anchor: Option<String>,
    tag: Option<(Tag, Mark)>,
}

impl Props {
    const fn any(&self) -> bool {
        self.anchor.is_some() || self.tag.is_some()
    }

    /// Adds the props of a later line. Two anchors or two tags are an error.
    fn merge(&mut self, more: Self, mark: Mark) -> Result<()> {
        if (self.anchor.is_some() && more.anchor.is_some())
            || (self.tag.is_some() && more.tag.is_some())
        {
            return Err(Error::new(
                "a node can have only one anchor and one tag",
                mark,
            ));
        }
        self.anchor = self.anchor.take().or(more.anchor);
        self.tag = self.tag.take().or(more.tag);
        Ok(())
    }
}

/// How a value on one line started. It decides if it can be a key.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    Plain,
    Quoted,
    Alias,
    Collection,
}

#[derive(Clone, Copy)]
struct State {
    pos: usize,
    line: usize,
    line_start: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Chomp {
    Strip,
    Clip,
    Keep,
}

struct Parser {
    src: Vec<char>,
    pos: usize,
    line: usize,
    line_start: usize,
    depth: usize,
    yaml_directive: bool,
    copied_nodes: usize,
    copied_bytes: usize,
    anchors: HashMap<String, Rc<Shared>>,
}

impl Parser {
    fn new(text: &str) -> Self {
        Self {
            src: text.chars().collect(),
            pos: 0,
            line: 1,
            line_start: 0,
            depth: 0,
            yaml_directive: false,
            copied_nodes: 0,
            copied_bytes: 0,
            anchors: HashMap::new(),
        }
    }

    // -- Cursor ------------------------------------------------------------

    fn peek(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.src.get(self.pos + n).copied()
    }

    fn bump(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.line_start = self.pos;
            }
        }
    }

    fn advance(&mut self, n: usize) {
        for _ in 0..n {
            self.bump();
        }
    }

    const fn column(&self) -> usize {
        self.pos - self.line_start
    }

    const fn mark(&self) -> Mark {
        Mark {
            line: self.line,
            column: self.column() + 1,
        }
    }

    fn err<T>(&self, msg: impl Into<String>) -> Result<T> {
        Err(Error::new(msg, self.mark()))
    }

    const fn state(&self) -> State {
        State {
            pos: self.pos,
            line: self.line,
            line_start: self.line_start,
        }
    }

    const fn restore(&mut self, s: State) {
        self.pos = s.pos;
        self.line = s.line;
        self.line_start = s.line_start;
    }

    fn starts_with(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(i, c)| self.peek_at(i) == Some(c))
    }

    /// White space, a line break or the end at `pos + n`.
    fn ws_or_end(&self, n: usize) -> bool {
        matches!(self.peek_at(n), None | Some(' ' | '\t' | '\n'))
    }

    /// Like [`Self::ws_or_end`], or a flow indicator.
    fn flow_end(&self, n: usize) -> bool {
        self.ws_or_end(n) || matches!(self.peek_at(n), Some(',' | '[' | ']' | '{' | '}'))
    }

    fn skip_blanks(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.bump();
        }
    }

    /// A `#` at the start of a line or after white space.
    fn at_comment(&self) -> bool {
        self.peek() == Some('#')
            && (self.pos == 0 || matches!(self.src[self.pos - 1], ' ' | '\t' | '\n'))
    }

    fn skip_to_line_end(&mut self) {
        while !matches!(self.peek(), None | Some('\n')) {
            self.bump();
        }
    }

    fn at_line_end(&self) -> bool {
        matches!(self.peek(), None | Some('\n')) || self.at_comment()
    }

    /// Skips blanks and a comment, then the line break.
    fn end_of_line(&mut self) -> Result<()> {
        self.skip_blanks();
        if self.at_comment() {
            self.skip_to_line_end();
        }
        match self.peek() {
            None => Ok(()),
            Some('\n') => {
                self.bump();
                Ok(())
            }
            Some(c) => self.err(format!("unexpected `{c}` after a value")),
        }
    }

    /// `---` or `...` at column 0, then white space or the end.
    fn marker_here(&self) -> bool {
        self.column() == 0
            && (self.starts_with("---") || self.starts_with("..."))
            && self.ws_or_end(3)
    }

    /// From a line start: skips empty and comment lines. Returns the
    /// indentation of the next line, with the position at its start, or
    /// `None` at the end.
    fn skip_blank_lines(&mut self) -> Result<Option<usize>> {
        loop {
            let start = self.pos;
            let mut i = start;
            while self.src.get(i) == Some(&' ') {
                i += 1;
            }
            let mut j = i;
            while matches!(self.src.get(j), Some(' ' | '\t')) {
                j += 1;
            }
            match self.src.get(j) {
                None => {
                    self.advance(j - start);
                    return Ok(None);
                }
                Some('\n' | '#') => {
                    self.advance(j - start);
                    self.skip_to_line_end();
                    self.bump();
                }
                Some(_) => return Ok(Some(i - start)),
            }
        }
    }

    /// Like [`Self::skip_blank_lines`], but a document marker is the end.
    fn next_content_line(&mut self) -> Result<Option<usize>> {
        Ok(self
            .skip_blank_lines()?
            .filter(|&n| n > 0 || !self.marker_here()))
    }

    /// White space before the position has a tab. A block collection can
    /// not start after a tab.
    fn tab_before(&self) -> bool {
        self.src[self.line_start..self.pos]
            .iter()
            .rev()
            .take_while(|&&c| c == ' ' || c == '\t')
            .any(|&c| c == '\t')
    }

    /// The line has only blanks from `pos + n` to its end.
    fn rest_blank(&self, n: usize) -> bool {
        self.src[self.pos + n..]
            .iter()
            .take_while(|&&c| c != '\n')
            .all(|&c| c == ' ' || c == '\t')
    }

    /// Errors at a tab where an entry of a block collection must start.
    fn no_tab(&self) -> Result<()> {
        if self.peek() == Some('\t') {
            return self.err("a tab character can not indent a line");
        }
        Ok(())
    }

    /// Like [`Self::tab_before`], at an earlier position on this line.
    fn tab_before_at(&self, mark: Mark) -> bool {
        mark.line == self.line
            && self.src[self.line_start..self.line_start + mark.column - 1]
                .iter()
                .rev()
                .take_while(|&&c| c == ' ' || c == '\t')
                .any(|&c| c == '\t')
    }

    fn seq_entry_at(&self, n: usize) -> bool {
        self.peek_at(n) == Some('-') && self.ws_or_end(n + 1)
    }

    fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.err(format!("nesting depth exceeds {MAX_DEPTH}"));
        }
        Ok(())
    }

    const fn leave(&mut self) {
        self.depth -= 1;
    }

    // -- Document ----------------------------------------------------------

    fn document(&mut self) -> Result<Node> {
        let mut directives = false;
        let node = loop {
            match self.skip_blank_lines()? {
                None if directives => return self.err("a directive needs `---` after it"),
                None => return Ok(Node::null(self.mark())),
                Some(0) if self.peek() == Some('%') => {
                    self.directive()?;
                    directives = true;
                }
                Some(0) if self.marker_here() && self.starts_with("---") => {
                    self.advance(3);
                    break self.block_node(None, false, false)?;
                }
                Some(0) if directives => return self.err("a directive needs `---` after it"),
                Some(0) if self.marker_here() => {
                    let mark = self.mark();
                    self.advance(3);
                    self.end_of_line()?;
                    break Node::null(mark);
                }
                Some(_) if directives => return self.err("a directive needs `---` after it"),
                Some(_) => break self.block_node(None, true, false)?,
            }
        };
        loop {
            match self.skip_blank_lines()? {
                None => return Ok(node),
                Some(0) if self.marker_here() && self.starts_with("...") => {
                    self.advance(3);
                    self.end_of_line()?;
                }
                Some(0) if self.marker_here() || self.peek() == Some('%') => {
                    return self.err("a file can have only one document");
                }
                Some(n) => {
                    self.advance(n);
                    return self.err("unexpected content after the document");
                }
            }
        }
    }

    fn directive(&mut self) -> Result<()> {
        let mark = self.mark();
        self.bump();
        let mut name = String::new();
        while let Some(c) = self.peek().filter(|c| !matches!(c, ' ' | '\t' | '\n')) {
            name.push(c);
            self.bump();
        }
        match name.as_str() {
            "YAML" => {
                if self.yaml_directive {
                    return Err(Error::new("a second `%YAML` directive", mark));
                }
                self.yaml_directive = true;
                self.skip_blanks();
                let version: String = std::iter::from_fn(|| {
                    let c = self.peek().filter(|c| c.is_ascii_digit() || *c == '.')?;
                    self.bump();
                    Some(c)
                })
                .collect();
                let valid = version
                    .split_once('.')
                    .is_some_and(|(a, b)| scalar::is_int(a) && scalar::is_int(b));
                if !valid || !self.ws_or_end(0) {
                    return self.err("`%YAML` needs a version such as `1.2`");
                }
                self.end_of_line()
            }
            "TAG" => Err(Error::new("`%TAG` directives are not supported", mark)),
            // The spec says to ignore a reserved directive.
            _ => {
                self.skip_to_line_end();
                self.bump();
                Ok(())
            }
        }
    }

    // -- Block context -----------------------------------------------------

    /// A node after `-`, `key:`, `---` or at a line start. `parent` is the
    /// indentation of the parent collection, `None` at the top. `compact`
    /// lets a collection start on this line. `map_value` lets a sequence
    /// start at the parent's indentation on the next line. Ends at a line
    /// start.
    fn block_node(
        &mut self,
        parent: Option<usize>,
        compact: bool,
        map_value: bool,
    ) -> Result<Node> {
        self.skip_blanks();
        let start = self.mark();
        // `props` apply to the node. `line` are the props on the line of the
        // content: an implicit key after them is an error.
        let mut props = Props::default();
        let mut line = self.properties(false)?;
        let mut compact = compact;
        while self.at_line_end() {
            self.end_of_line()?;
            match self.next_content_line()? {
                Some(n)
                    if parent.is_none_or(|p| n > p)
                        || (map_value && parent == Some(n) && self.seq_entry_at(n)) =>
                {
                    self.advance(n);
                    self.skip_blanks();
                    compact = true;
                    props.merge(line, self.mark())?;
                    line = self.properties(false)?;
                }
                _ => {
                    props.merge(line, start)?;
                    return self.finish_props(props, Node::null(start));
                }
            }
        }
        if line.any() && self.peek() == Some('*') {
            return self.err("an alias can not have an anchor or tag");
        }
        let col = self.column();
        let mark = self.mark();
        let node = self.block_content(col, parent, compact, line.any())?;
        props.merge(line, mark)?;
        self.finish_props(props, node)
    }

    fn block_content(
        &mut self,
        col: usize,
        parent: Option<usize>,
        compact: bool,
        has_props: bool,
    ) -> Result<Node> {
        match self.peek() {
            Some('-') if self.ws_or_end(1) => {
                if !compact || has_props {
                    return self.err("a block sequence can not start on this line");
                }
                if self.tab_before() {
                    return self.err("a tab character can not indent a block sequence");
                }
                return self.block_seq(col);
            }
            Some('?') if self.ws_or_end(1) => {
                return self.err("explicit keys (`? `) are not supported");
            }
            Some('|' | '>') => return self.block_scalar(parent),
            _ => {}
        }
        let line = self.line;
        let (node, item) = self.inline_item(false)?;
        self.skip_blanks();
        if self.line == line && self.peek() == Some(':') && self.ws_or_end(1) {
            if !compact {
                return self.err("a mapping value is not allowed here");
            }
            if has_props {
                return Err(Error::new(
                    "an anchor or tag before an implicit key is not supported",
                    node.mark,
                ));
            }
            check_key(&node, item)?;
            if self.tab_before_at(node.mark) {
                return Err(Error::new(
                    "a tab character can not indent a block mapping",
                    node.mark,
                ));
            }
            return self.block_map(col, node);
        }
        if item == Item::Plain && matches!(self.peek(), None | Some('\n')) {
            return self.plain_continue(node, parent);
        }
        self.end_of_line()?;
        Ok(node)
    }

    fn block_seq(&mut self, indent: usize) -> Result<Node> {
        self.enter()?;
        let mark = self.mark();
        let mut items = Vec::new();
        loop {
            self.bump();
            items.push(self.block_node(Some(indent), true, false)?);
            match self.next_content_line()? {
                Some(n) if n == indent && self.seq_entry_at(n) => self.advance(n),
                Some(n) if n == indent && self.peek_at(n) == Some('\t') => {
                    self.advance(n);
                    return self.err("a tab character can not indent a line");
                }
                Some(n) if n > indent => {
                    self.advance(n);
                    return self.err("bad indentation of a sequence entry");
                }
                _ => break,
            }
        }
        self.leave();
        Ok(Node::new(mark, NodeKind::Seq(items)))
    }

    /// A block mapping. The position is at the `:` after `first_key`.
    fn block_map(&mut self, indent: usize, first_key: Node) -> Result<Node> {
        self.enter()?;
        let mark = first_key.mark;
        let mut map = MapBuilder::default();
        let mut key = first_key;
        loop {
            self.bump();
            let value = self.block_node(Some(indent), false, true)?;
            map.insert(key, value)?;
            match self.next_content_line()? {
                Some(n) if n == indent => {
                    self.advance(n);
                    self.no_tab()?;
                    key = self.block_key()?;
                }
                Some(n) if n > indent => {
                    self.advance(n);
                    return self.err("bad indentation of a mapping entry");
                }
                _ => break,
            }
        }
        self.leave();
        Ok(Node::new(mark, NodeKind::Map(map.finish()?)))
    }

    /// A key after the first one of a block mapping. Ends at the `:`.
    fn block_key(&mut self) -> Result<Node> {
        match self.peek() {
            Some('-') if self.ws_or_end(1) => {
                return self.err("expected a mapping key, found a sequence entry");
            }
            Some('?') if self.ws_or_end(1) => {
                return self.err("explicit keys (`? `) are not supported");
            }
            Some('&' | '!') => {
                return self.err("an anchor or tag before an implicit key is not supported");
            }
            Some('|' | '>') => return self.err("a block scalar can not be a mapping key"),
            _ => {}
        }
        let line = self.line;
        let (node, item) = self.inline_item(false)?;
        self.skip_blanks();
        if self.line != line || self.peek() != Some(':') || !self.ws_or_end(1) {
            return self.err("expected `:` after a mapping key");
        }
        check_key(&node, item)?;
        Ok(node)
    }

    /// One value that starts on this line: a flow collection, a quoted
    /// scalar, an alias or the first line of a plain scalar.
    fn inline_item(&mut self, flow: bool) -> Result<(Node, Item)> {
        let mark = self.mark();
        match self.peek() {
            Some('[') => Ok((self.flow_seq()?, Item::Collection)),
            Some('{') => Ok((self.flow_map()?, Item::Collection)),
            Some('\'') => Ok((self.single_quoted()?, Item::Quoted)),
            Some('"') => Ok((self.double_quoted()?, Item::Quoted)),
            Some('*') => Ok((self.alias()?, Item::Alias)),
            Some(_) if self.plain_start(flow) => {
                let node = if flow {
                    self.plain_flow()
                } else {
                    Node::plain(mark, self.plain_line(false))
                };
                Ok((node, Item::Plain))
            }
            Some(':') if self.flow_end(1) => self.err("an empty mapping key is not supported"),
            Some(c) => self.err(format!("`{c}` can not start a value")),
            None => Ok((Node::null(mark), Item::Plain)),
        }
    }

    fn plain_start(&self, flow: bool) -> bool {
        match self.peek() {
            Some('-' | '?' | ':') => {
                !(if flow {
                    self.flow_end(1)
                } else {
                    self.ws_or_end(1)
                })
            }
            Some(c) => !matches!(
                c,
                ',' | '['
                    | ']'
                    | '{'
                    | '}'
                    | '#'
                    | '&'
                    | '*'
                    | '!'
                    | '|'
                    | '>'
                    | '\''
                    | '"'
                    | '%'
                    | '@'
                    | '`'
                    | ' '
                    | '\t'
                    | '\n'
            ),
            None => false,
        }
    }

    /// The rest of a plain scalar on this line, without trailing blanks.
    fn plain_line(&mut self, flow: bool) -> String {
        let mut text = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => break,
                Some(':') if self.ws_or_end(1) || (flow && self.flow_end(1)) => break,
                Some('#') if self.at_comment() => break,
                Some(',' | '[' | ']' | '{' | '}') if flow => break,
                Some(c) => {
                    text.push(c);
                    self.bump();
                }
            }
        }
        let kept = text.trim_end_matches([' ', '\t']).len();
        text.truncate(kept);
        text
    }

    /// Folds the lines of a block plain scalar that continue `first`.
    fn plain_continue(&mut self, first: Node, parent: Option<usize>) -> Result<Node> {
        let mark = first.mark;
        let mut text = match first.kind() {
            NodeKind::Scalar(s) => s.text.clone(),
            _ => return Ok(first),
        };
        while self.peek() == Some('\n') {
            self.bump();
            let mut empties = 0;
            let indent = loop {
                let spaces = self.src[self.pos..]
                    .iter()
                    .take_while(|&&c| c == ' ')
                    .count();
                let rest = self.src[self.pos + spaces..]
                    .iter()
                    .position(|&c| !matches!(c, ' ' | '\t'));
                match rest.map(|r| self.src[self.pos + spaces + r]) {
                    None | Some('#') => break None,
                    Some('\n') => {
                        self.skip_to_line_end();
                        self.bump();
                        empties += 1;
                    }
                    Some(_) => break Some(spaces),
                }
            };
            let Some(n) = indent.filter(|&n| parent.is_none_or(|p| n > p)) else {
                break;
            };
            if n == 0 && self.marker_here() {
                break;
            }
            self.skip_blanks();
            let segment = self.plain_line(false);
            if empties == 0 {
                text.push(' ');
            } else {
                text.extend(std::iter::repeat_n('\n', empties));
            }
            text.push_str(&segment);
            if self.peek() == Some(':') {
                return self.err("a mapping value is not allowed in a multi-line plain scalar");
            }
            if self.at_comment() {
                self.end_of_line()?;
                break;
            }
        }
        Ok(Node::plain(mark, text))
    }

    fn block_scalar(&mut self, parent: Option<usize>) -> Result<Node> {
        let mark = self.mark();
        let literal = self.peek() == Some('|');
        self.bump();
        let mut chomp = None;
        let mut explicit = None;
        for _ in 0..2 {
            match self.peek() {
                Some('-') if chomp.is_none() => chomp = Some(Chomp::Strip),
                Some('+') if chomp.is_none() => chomp = Some(Chomp::Keep),
                Some(c @ '1'..='9') if explicit.is_none() => explicit = c.to_digit(10),
                _ => break,
            }
            self.bump();
        }
        if !self.ws_or_end(0) {
            return self.err("invalid block scalar header");
        }
        self.end_of_line()?;
        let min = parent.map_or(0, |p| p + 1);
        let mut indent = explicit.map(|d| parent.unwrap_or(0) + d as usize);
        let mut lines: Vec<Option<String>> = Vec::new();
        let mut leading: Option<(usize, Mark)> = None;
        let mut final_break = true;
        while self.peek().is_some() {
            let spaces = self.src[self.pos..]
                .iter()
                .take_while(|&&c| c == ' ')
                .count();
            let after = self.src.get(self.pos + spaces).copied();
            let spaces_only = matches!(after, None | Some('\n'));
            let n = if let Some(n) = indent {
                n
            } else {
                if spaces_only {
                    if leading.is_none_or(|(max, _)| spaces > max) {
                        leading = Some((spaces, self.mark()));
                    }
                    self.skip_to_line_end();
                    if self.peek() == Some('\n') {
                        lines.push(None);
                    }
                    self.bump();
                    continue;
                }
                if spaces < min && after == Some('\t') && self.rest_blank(spaces) {
                    self.advance(spaces);
                    return self.err("a tab character can not indent a block scalar line");
                }
                if spaces < min || (spaces == 0 && self.marker_here()) {
                    break;
                }
                if let Some((_, at)) = leading.filter(|&(max, _)| max > spaces) {
                    return Err(Error::new(
                        "an empty line has more spaces than the first line of the block scalar",
                        at,
                    ));
                }
                indent = Some(spaces);
                spaces
            };
            if spaces == 0 && self.marker_here() {
                break;
            }
            let line = if spaces >= n {
                self.advance(n);
                let start = self.pos;
                self.skip_to_line_end();
                let text: String = self.src[start..self.pos].iter().collect();
                (!text.is_empty()).then_some(text)
            } else if spaces_only {
                self.skip_to_line_end();
                None
            } else if self.rest_blank(spaces) {
                self.advance(spaces);
                return self.err("a tab character can not indent a block scalar line");
            } else {
                break;
            };
            // At the end of the input a line has no break: an empty line is
            // then not a line, and the last text line keeps no break.
            let broke = self.peek() == Some('\n');
            if broke || line.is_some() {
                lines.push(line);
                final_break = broke;
            }
            self.bump();
        }
        let text = block_text(&lines, literal, chomp.unwrap_or(Chomp::Clip), final_break);
        Ok(Node::scalar(mark, text, Kind::Str, false))
    }

    // -- Flow context ------------------------------------------------------

    fn skip_flow_ws(&mut self) -> Result<()> {
        loop {
            if self.column() == 0 && self.marker_here() {
                return self.err("a document marker can not be in a flow collection");
            }
            match self.peek() {
                Some(' ' | '\t' | '\n') => self.bump(),
                Some('#') if self.at_comment() => self.skip_to_line_end(),
                _ => return Ok(()),
            }
        }
    }

    fn flow_node(&mut self) -> Result<(Node, Item)> {
        let start = self.mark();
        let props = self.properties(true)?;
        if props.any() {
            self.skip_flow_ws()?;
        }
        if props.any() && self.peek() == Some('*') {
            return self.err("an alias can not have an anchor or tag");
        }
        let (node, item) =
            if props.any() && matches!(self.peek(), None | Some(',' | ']' | '}' | ':')) {
                (Node::null(start), Item::Plain)
            } else {
                self.inline_item(true)?
            };
        Ok((self.finish_props(props, node)?, item))
    }

    fn flow_seq(&mut self) -> Result<Node> {
        let mark = self.mark();
        self.enter()?;
        self.bump();
        let mut items = Vec::new();
        loop {
            self.skip_flow_ws()?;
            match self.peek() {
                Some(']') => {
                    self.bump();
                    break;
                }
                None => return self.err("unterminated flow sequence: expected `]`"),
                _ => {}
            }
            let (item, _) = self.flow_node()?;
            self.skip_flow_ws()?;
            if self.peek() == Some(':') {
                return self.err("a key-value pair in a flow sequence is not supported");
            }
            items.push(item);
            match self.peek() {
                Some(',') => self.bump(),
                Some(']') => {}
                None => return self.err("unterminated flow sequence: expected `]`"),
                Some(c) => return self.err(format!("expected `,` or `]`, found `{c}`")),
            }
        }
        self.leave();
        Ok(Node::new(mark, NodeKind::Seq(items)))
    }

    fn flow_map(&mut self) -> Result<Node> {
        let mark = self.mark();
        self.enter()?;
        self.bump();
        let mut map = MapBuilder::default();
        loop {
            self.skip_flow_ws()?;
            match self.peek() {
                Some('}') => {
                    self.bump();
                    break;
                }
                None => return self.err("unterminated flow mapping: expected `}`"),
                Some('?') if self.flow_end(1) => {
                    return self.err("explicit keys (`? `) are not supported");
                }
                _ => {}
            }
            let (key, item) = self.flow_node()?;
            check_key(&key, item)?;
            self.skip_flow_ws()?;
            let value = if self.peek() == Some(':') {
                self.bump();
                self.skip_flow_ws()?;
                if matches!(self.peek(), Some(',' | '}')) {
                    Node::null(self.mark())
                } else {
                    self.flow_node()?.0
                }
            } else {
                Node::null(self.mark())
            };
            map.insert(key, value)?;
            self.skip_flow_ws()?;
            match self.peek() {
                Some(',') => self.bump(),
                Some('}') => {}
                None => return self.err("unterminated flow mapping: expected `}`"),
                Some(c) => return self.err(format!("expected `,` or `}}`, found `{c}`")),
            }
        }
        self.leave();
        Ok(Node::new(mark, NodeKind::Map(map.finish()?)))
    }

    /// A plain scalar in a flow collection. It can go on to the next lines.
    fn plain_flow(&mut self) -> Node {
        let mark = self.mark();
        let mut text = self.plain_line(true);
        while self.peek() == Some('\n') {
            let save = self.state();
            self.bump();
            let mut empties = 0;
            loop {
                self.skip_blanks();
                if self.peek() != Some('\n') {
                    break;
                }
                self.bump();
                empties += 1;
            }
            let stop = match self.peek() {
                None | Some(',' | '[' | ']' | '{' | '}' | '#') => true,
                Some(':') => self.flow_end(1),
                Some(_) => self.marker_here(),
            };
            if stop {
                self.restore(save);
                break;
            }
            let segment = self.plain_line(true);
            if empties == 0 {
                text.push(' ');
            } else {
                text.extend(std::iter::repeat_n('\n', empties));
            }
            text.push_str(&segment);
        }
        Node::plain(mark, text)
    }

    // -- Scalars -----------------------------------------------------------

    /// After a line break in a quoted scalar: skips the indentation and the
    /// empty lines. Returns the number of empty lines.
    fn skip_folded_lines(&mut self) -> Result<usize> {
        let mut empties = 0;
        loop {
            if self.marker_here() {
                return self.err("a document marker can not be in a quoted scalar");
            }
            self.skip_blanks();
            if self.peek() != Some('\n') {
                return Ok(empties);
            }
            self.bump();
            empties += 1;
        }
    }

    /// Folds a line break in a quoted scalar. `kept` is the length of
    /// `text` that trailing-blank removal must not touch (escapes).
    fn fold_break(&mut self, text: &mut String, kept: usize) -> Result<()> {
        let end = text[kept..].trim_end_matches([' ', '\t']).len();
        text.truncate(kept + end);
        self.bump();
        let empties = self.skip_folded_lines()?;
        if empties == 0 {
            text.push(' ');
        } else {
            text.extend(std::iter::repeat_n('\n', empties));
        }
        Ok(())
    }

    fn single_quoted(&mut self) -> Result<Node> {
        let mark = self.mark();
        self.bump();
        let mut text = String::new();
        let mut kept = 0;
        loop {
            match self.peek() {
                None => return self.err("unterminated quoted scalar"),
                Some('\'') if self.peek_at(1) == Some('\'') => {
                    text.push('\'');
                    self.advance(2);
                }
                Some('\'') => {
                    self.bump();
                    break;
                }
                Some('\n') => {
                    self.fold_break(&mut text, kept)?;
                    kept = text.len();
                }
                Some(c) => {
                    text.push(c);
                    self.bump();
                }
            }
        }
        Ok(Node::scalar(mark, text, Kind::Str, false))
    }

    fn double_quoted(&mut self) -> Result<Node> {
        let mark = self.mark();
        self.bump();
        let mut text = String::new();
        let mut kept = 0;
        loop {
            match self.peek() {
                None => return self.err("unterminated quoted scalar"),
                Some('"') => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    self.escape(&mut text)?;
                    kept = text.len();
                }
                Some('\n') => {
                    self.fold_break(&mut text, kept)?;
                    kept = text.len();
                }
                Some(c) => {
                    text.push(c);
                    self.bump();
                }
            }
        }
        Ok(Node::scalar(mark, text, Kind::Str, false))
    }

    fn escape(&mut self, text: &mut String) -> Result<()> {
        let mark = self.mark();
        self.bump();
        let Some(c) = self.peek() else {
            return self.err("unterminated quoted scalar");
        };
        self.bump();
        let ch = match c {
            '\n' => {
                let empties = self.skip_folded_lines()?;
                text.extend(std::iter::repeat_n('\n', empties));
                return Ok(());
            }
            '0' => '\0',
            'a' => '\u{7}',
            'b' => '\u{8}',
            't' | '\t' => '\t',
            'n' => '\n',
            'v' => '\u{b}',
            'f' => '\u{c}',
            'r' => '\r',
            'e' => '\u{1b}',
            ' ' => ' ',
            '"' => '"',
            '/' => '/',
            '\\' => '\\',
            'N' => '\u{85}',
            '_' => '\u{a0}',
            'L' => '\u{2028}',
            'P' => '\u{2029}',
            'x' | 'u' | 'U' => {
                let len = match c {
                    'x' => 2,
                    'u' => 4,
                    _ => 8,
                };
                let digits: String = (0..len).filter_map(|i| self.peek_at(i)).collect();
                let code = (digits.len() == len && digits.chars().all(|d| d.is_ascii_hexdigit()))
                    .then(|| u32::from_str_radix(&digits, 16).ok())
                    .flatten()
                    .and_then(char::from_u32);
                let Some(ch) = code else {
                    return Err(Error::new(format!("invalid escape `\\{c}{digits}`"), mark));
                };
                self.advance(len);
                ch
            }
            _ => return Err(Error::new(format!("unknown escape `\\{c}`"), mark)),
        };
        text.push(ch);
        Ok(())
    }

    // -- Anchors, aliases, tags --------------------------------------------

    fn anchor_name(&mut self) -> Result<String> {
        let mut name = String::new();
        while let Some(c) = self.peek().filter(|_| !self.flow_end(0)) {
            name.push(c);
            self.bump();
        }
        if name.is_empty() {
            return self.err("an anchor or alias needs a name");
        }
        Ok(name)
    }

    fn alias(&mut self) -> Result<Node> {
        let mark = self.mark();
        self.bump();
        let name = self.anchor_name()?;
        let Some(shared) = self.anchors.get(&name) else {
            return Err(Error::new(format!("unknown anchor `{name}`"), mark));
        };
        if self.depth + shared.height > MAX_DEPTH {
            return Err(Error::new(
                format!("nesting depth exceeds {MAX_DEPTH}"),
                mark,
            ));
        }
        self.copied_nodes += shared.size;
        self.copied_bytes += shared.bytes;
        if self.copied_nodes > MAX_ALIAS_NODES || self.copied_bytes > MAX_ALIAS_BYTES {
            return Err(Error::new(
                format!(
                    "aliases copy more than {MAX_ALIAS_NODES} nodes or {} MiB of text",
                    MAX_ALIAS_BYTES >> 20
                ),
                mark,
            ));
        }
        Ok(Node {
            mark,
            body: Body::Shared(Rc::clone(shared)),
        })
    }

    /// An anchor and a tag, in any order, each followed by white space.
    fn properties(&mut self, flow: bool) -> Result<Props> {
        let mut props = Props::default();
        loop {
            match self.peek() {
                Some('&') if props.anchor.is_none() => {
                    self.bump();
                    props.anchor = Some(self.anchor_name()?);
                }
                Some('!') if props.tag.is_none() => {
                    let mark = self.mark();
                    props.tag = Some((self.tag()?, mark));
                }
                _ => return Ok(props),
            }
            if !(if flow {
                self.flow_end(0)
            } else {
                self.ws_or_end(0)
            }) {
                return self.err("expected white space after an anchor or tag");
            }
            self.skip_blanks();
        }
    }

    fn tag(&mut self) -> Result<Tag> {
        let mark = self.mark();
        self.bump();
        let verbatim = self.peek() == Some('<');
        let mut name = String::new();
        while let Some(c) = self.peek().filter(|_| !self.flow_end(0)) {
            name.push(c);
            self.bump();
        }
        let tag = match name.as_str() {
            "" => Some(Tag::NonSpecific),
            "!str" => Some(Tag::Str),
            "!int" => Some(Tag::Int),
            "!float" => Some(Tag::Float),
            "!bool" => Some(Tag::Bool),
            "!null" => Some(Tag::Null),
            "!seq" => Some(Tag::Seq),
            "!map" => Some(Tag::Map),
            _ => None,
        };
        match tag {
            Some(tag) if !verbatim => Ok(tag),
            _ => Err(Error::new(format!("unsupported tag `!{name}`"), mark)),
        }
    }

    fn finish_props(&mut self, props: Props, mut node: Node) -> Result<Node> {
        if let Some((tag, mark)) = props.tag {
            apply_tag(&mut node, tag, mark)?;
        }
        let Some(name) = props.anchor else {
            return Ok(node);
        };
        let (size, bytes, height) = node.measure();
        let mark = node.mark;
        let shared = Rc::new(Shared {
            node,
            size,
            bytes,
            height,
        });
        self.anchors.insert(name, Rc::clone(&shared));
        Ok(Node {
            mark,
            body: Body::Shared(shared),
        })
    }
}

fn apply_tag(node: &mut Node, tag: Tag, mark: Mark) -> Result<()> {
    let bad = |what: &str| {
        Err(Error::new(
            format!("`{}` can not tag {what}", tag.name()),
            mark,
        ))
    };
    let Body::Own(kind) = &mut node.body else {
        return bad("an alias");
    };
    match kind {
        NodeKind::Seq(_) => match tag {
            Tag::NonSpecific | Tag::Seq => Ok(()),
            _ => bad("a sequence"),
        },
        NodeKind::Map(_) => match tag {
            Tag::NonSpecific | Tag::Map => Ok(()),
            _ => bad("a mapping"),
        },
        NodeKind::Scalar(s) => {
            let kind = match tag {
                Tag::NonSpecific | Tag::Str => Some(Kind::Str),
                Tag::Int => scalar::is_int(&s.text).then_some(Kind::Int),
                Tag::Float => {
                    (scalar::is_int(&s.text) || scalar::is_float(&s.text)).then_some(Kind::Float)
                }
                Tag::Bool => scalar::parse_bool(&s.text).map(Kind::Bool),
                Tag::Null => scalar::is_null(&s.text).then_some(Kind::Null),
                Tag::Seq | Tag::Map => return bad("a scalar"),
            };
            let Some(kind) = kind else {
                return bad(&format!("`{}`", s.text));
            };
            s.kind = kind;
            s.plain = false;
            Ok(())
        }
    }
}

fn check_key(node: &Node, item: Item) -> Result<()> {
    match item {
        Item::Collection => Err(Error::new(
            "a collection as a mapping key is not supported",
            node.mark,
        )),
        Item::Alias => Err(Error::new(
            "an alias as a mapping key is not supported",
            node.mark,
        )),
        Item::Plain | Item::Quoted => Ok(()),
    }
}

/// The value of a block scalar from its lines (`None` for an empty line).
/// `final_break` is false if the last line ends at the end of the input.
fn block_text(lines: &[Option<String>], literal: bool, chomp: Chomp, final_break: bool) -> String {
    let content = lines.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
    let trailing = lines.len() - content;
    let last_break = content > 0 && (trailing > 0 || final_break);
    let mut out = String::new();
    let mut prev_more: Option<bool> = None;
    let mut empties = 0;
    for line in &lines[..content] {
        let Some(text) = line else {
            empties += 1;
            continue;
        };
        let more = text.starts_with([' ', '\t']);
        let breaks = match prev_more {
            Some(prev) if literal || prev || more => empties + 1,
            Some(_) if empties == 0 => {
                out.push(' ');
                0
            }
            _ => empties,
        };
        out.extend(std::iter::repeat_n('\n', breaks));
        out.push_str(text);
        prev_more = Some(more);
        empties = 0;
    }
    match chomp {
        Chomp::Clip if last_break => out.push('\n'),
        Chomp::Keep => {
            let breaks = trailing + usize::from(last_break);
            out.extend(std::iter::repeat_n('\n', breaks));
        }
        Chomp::Strip | Chomp::Clip => {}
    }
    out
}

/// The identity of a key, for duplicate checks and merges.
#[derive(Hash, PartialEq, Eq)]
enum KeyId {
    Null,
    Bool(bool),
    Int(i128),
    Float(u64),
    Text(String),
}

impl KeyId {
    fn of(node: &Node) -> Self {
        let NodeKind::Scalar(s) = node.kind() else {
            return Self::Text(String::new());
        };
        match s.kind {
            Kind::Null => Self::Null,
            Kind::Bool(b) => Self::Bool(b),
            Kind::Int => {
                scalar::parse_int(&s.text).map_or_else(|| Self::Text(s.text.clone()), Self::Int)
            }
            Kind::Float => {
                // `+ 0.0` makes -0.0 the same key as 0.0.
                Self::Float((scalar::parse_float(&s.text).unwrap_or(0.0) + 0.0).to_bits())
            }
            Kind::Str => Self::Text(s.text.clone()),
        }
    }
}

enum Entry {
    Pair(Node, Node),
    Merge(Node),
}

/// The text of a scalar key. Keys with the same text are duplicates too:
/// `1` and `"1"` are both `"1"` in a map with `String` keys.
fn key_text(node: &Node) -> &str {
    match node.kind() {
        NodeKind::Scalar(s) => &s.text,
        _ => "",
    }
}

#[derive(Default)]
struct MapBuilder {
    entries: Vec<Entry>,
    keys: HashSet<KeyId>,
    texts: HashSet<String>,
}

impl MapBuilder {
    /// Adds a key if no key has its value or its text.
    fn add_key(keys: &mut HashSet<KeyId>, texts: &mut HashSet<String>, key: &Node) -> bool {
        let id = KeyId::of(key);
        let text = key_text(key);
        if keys.contains(&id) || texts.contains(text) {
            return false;
        }
        keys.insert(id);
        texts.insert(text.to_string());
        true
    }

    fn insert(&mut self, key: Node, value: Node) -> Result<()> {
        if matches!(key.kind(), NodeKind::Scalar(s) if s.plain && s.text == "<<") {
            self.entries.push(Entry::Merge(value));
            return Ok(());
        }
        if !Self::add_key(&mut self.keys, &mut self.texts, &key) {
            return Err(Error::new(
                format!("duplicate key `{}`", key_text(&key)),
                key.mark,
            ));
        }
        self.entries.push(Entry::Pair(key, value));
        Ok(())
    }

    /// The pairs in order. Merged keys go at the place of `<<`. A key in the
    /// mapping wins over a merged key; an earlier merged mapping wins over a
    /// later one.
    fn finish(self) -> Result<Vec<(Node, Node)>> {
        let (mut keys, mut texts) = (self.keys, self.texts);
        let mut out = Vec::new();
        for entry in self.entries {
            match entry {
                Entry::Pair(k, v) => out.push((k, v)),
                Entry::Merge(value) => {
                    for (k, v) in merge_sources(value)?.into_iter().flatten() {
                        if Self::add_key(&mut keys, &mut texts, &k) {
                            out.push((k, v));
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

/// The mappings of a `<<` value: one mapping, or a sequence of them.
fn merge_sources(value: Node) -> Result<Vec<Vec<(Node, Node)>>> {
    let bad = |mark| {
        Error::new(
            "a merge key (`<<`) needs a mapping or a sequence of mappings",
            mark,
        )
    };
    let mark = value.mark;
    if let NodeKind::Seq(items) = value.kind() {
        return items
            .iter()
            .map(|n| n.clone().into_pairs().ok_or_else(|| bad(n.mark)))
            .collect();
    }
    value
        .into_pairs()
        .map(|pairs| vec![pairs])
        .ok_or_else(|| bad(mark))
}
