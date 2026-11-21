use super::*;

use std::cell::RefCell;
use std::rc::Rc;

use html5ever::tokenizer::{Doctype, Tag, TagKind, Token as TreeToken, TokenSink, TokenSinkResult};
use html5ever::tree_builder::TreeBuilder;
use html5gum::emitters::callback::{CallbackEmitter, CallbackEvent};
use html5gum::{Emitter, Span, State, Tokenizer};
use markup5ever_rcdom::{Handle, NodeData};

#[path = "tokenizer/tracking_sink.rs"]
mod tracking_sink;
use self::tracking_sink::TrackingSink;

#[derive(Clone, Debug)]
pub(super) enum TokenKind {
    Start {
        name: String,
        range: Range<usize>,
        attrs: std::collections::BTreeMap<String, String>,
    },
    End {
        name: String,
        range: Range<usize>,
    },
    Text(String),
    Comment,
    Error,
    Limit,
}

#[derive(Clone, Debug)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) foreign: bool,
    pub(super) ignored: bool,
}

#[derive(Default)]
struct PendingTag {
    name: String,
    start: usize,
    end: usize,
    attribute_name: Option<String>,
    attrs: std::collections::BTreeMap<String, String>,
}

struct EmitterContext {
    pending: PendingTag,
    active_attribute: Option<String>,
    last_start_state: Option<State>,
    tree_builder: Rc<RefCell<TreeBuilder<Handle, TrackingSink>>>,
    count: usize,
    start_tags: usize,
    select_depth: usize,
}

impl EmitterContext {
    // Builds an HTML tree builder used only to track parser namespace and tokenizer states.
    fn new() -> Self {
        Self {
            pending: PendingTag::default(),
            active_attribute: None,
            last_start_state: None,
            tree_builder: Rc::new(RefCell::new(TreeBuilder::new(
                TrackingSink::default(),
                html5ever::ParseOpts::default().tree_builder,
            ))),
            count: 0,
            start_tags: 0,
            select_depth: 0,
        }
    }

    // Sends one source token to html5ever and maps its requested tokenizer state.
    fn process_tree_token(&self, token: TreeToken) -> Option<State> {
        match self.tree_builder.borrow().process_token(token, 1) {
            TokenSinkResult::Continue => None,
            TokenSinkResult::Script(_) => Some(State::Data),
            TokenSinkResult::Plaintext => Some(State::PlainText),
            TokenSinkResult::RawData(kind) => Some(match kind {
                html5ever::tokenizer::states::RawKind::Rcdata => State::RcData,
                html5ever::tokenizer::states::RawKind::Rawtext => State::RawText,
                html5ever::tokenizer::states::RawKind::ScriptData
                | html5ever::tokenizer::states::RawKind::ScriptDataEscaped(_) => State::ScriptData,
            }),
        }
    }

    // Reads the tree builder's namespace state before the next token is processed.
    fn in_foreign_namespace(&self) -> bool {
        self.tree_builder
            .borrow()
            .adjusted_current_node_present_but_not_in_html_namespace()
    }

    // Clears the sink's latest element marker before processing a start tag.
    fn clear_latest_element(&self) {
        self.tree_builder.borrow().sink.last_created.replace(None);
    }

    // Reads the namespace of the element html5ever actually created for a start tag.
    fn latest_element_is_foreign(&self) -> Option<bool> {
        let builder = self.tree_builder.borrow();
        let node = builder.sink.last_created.borrow().clone()?;
        let NodeData::Element { name, .. } = &node.data else {
            return None;
        };
        Some(name.ns.as_ref() != "http://www.w3.org/1999/xhtml")
    }
}

struct ContextEmitter<F>
where
    F: for<'a> FnMut(CallbackEvent<'a>, Span<usize>) -> Option<Token>,
{
    inner: CallbackEmitter<F, Token, usize>,
    context: Rc<RefCell<EmitterContext>>,
}

impl<F> html5gum::ForwardingEmitter for ContextEmitter<F>
where
    F: for<'a> FnMut(CallbackEvent<'a>, Span<usize>) -> Option<Token>,
{
    type Token = Token;

    /// Returns the wrapped emitter used by html5gum.
    fn inner(&mut self) -> &mut impl Emitter<Token = Self::Token> {
        &mut self.inner
    }

    // Applies raw-text states selected by html5ever's tree builder.
    fn emit_current_tag(&mut self) -> Option<State> {
        let _ = self.inner.emit_current_tag();
        self.context.borrow_mut().last_start_state.take()
    }

    // Lets html5ever decide whether the current node enables CDATA tokenization.
    fn adjusted_current_node_present_but_not_in_html_namespace(&mut self) -> bool {
        self.context.borrow().in_foreign_namespace()
    }

    // Finishes the tree builder after the tokenizer has consumed the source.
    fn emit_eof(&mut self) {
        self.inner.emit_eof();
        let builder = self.context.borrow().tree_builder.clone();
        let _ = builder.borrow().process_token(TreeToken::EOFToken, 1);
        builder.borrow().end();
    }
}

// Names that can change the table grid when the tree builder accepts them.
fn is_table_structure_tag(name: &str) -> bool {
    matches!(
        name,
        "table" | "caption" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th"
    )
}

// Emits source-spanned tokens and switches tokenizer states with tag context.
pub(super) fn tokenize(text: &str) -> Result<Vec<Token>, HtmlError> {
    tokenize_with_token_limit(text, MAX_HTML_TOKENS)
}

// Uses a smaller callback limit for focused tests without allocating a huge source.
pub(super) fn tokenize_with_token_limit(
    text: &str,
    token_limit: usize,
) -> Result<Vec<Token>, HtmlError> {
    let context = Rc::new(RefCell::new(EmitterContext::new()));
    let callback_context = Rc::clone(&context);
    let callback = move |event: CallbackEvent<'_>, span: Span<usize>| -> Option<Token> {
        let mut state = callback_context.borrow_mut();
        state.count += 1;
        if state.count > token_limit || state.start_tags > MAX_HTML_CELLS {
            return Some(Token {
                kind: TokenKind::Limit,
                foreign: state.in_foreign_namespace(),
                ignored: false,
            });
        }
        match event {
            CallbackEvent::OpenStartTag { name } => {
                state.pending = PendingTag {
                    name: String::from_utf8_lossy(name).to_ascii_lowercase(),
                    start: span.start,
                    end: span.end,
                    ..PendingTag::default()
                };
                state.active_attribute = None;
                None
            }
            CallbackEvent::AttributeName { name } => {
                let name = String::from_utf8_lossy(name).to_ascii_lowercase();
                if state.pending.attrs.contains_key(&name) {
                    state.pending.attribute_name = None;
                    state.active_attribute = None;
                } else {
                    state.pending.attrs.insert(name.clone(), String::new());
                    state.pending.attribute_name = Some(name.clone());
                    state.active_attribute = Some(name);
                }
                None
            }
            CallbackEvent::AttributeValue { value } => {
                if let Some(name) = state.active_attribute.take() {
                    let target = state.pending.attrs.entry(name).or_default();
                    target.push_str(&String::from_utf8_lossy(value));
                }
                None
            }
            CallbackEvent::CloseStartTag { self_closing } => {
                let mut tag = std::mem::take(&mut state.pending);
                tag.end = span.end;
                tag.attribute_name = None;
                let name = tag.name.clone();
                let was_foreign = state.in_foreign_namespace();
                let attrs = tag
                    .attrs
                    .iter()
                    .map(|(name, value)| html5ever::Attribute {
                        name: html5ever::QualName::new(
                            None,
                            Default::default(),
                            name.clone().into(),
                        ),
                        value: value.clone().into(),
                    })
                    .collect();
                let tree_token = TreeToken::TagToken(Tag {
                    kind: TagKind::StartTag,
                    name: name.clone().into(),
                    self_closing,
                    attrs,
                });
                if state.start_tags >= MAX_HTML_CELLS {
                    return Some(Token {
                        kind: TokenKind::Limit,
                        foreign: was_foreign,
                        ignored: false,
                    });
                }
                state.clear_latest_element();
                state.start_tags += 1;
                state.last_start_state = state.process_tree_token(tree_token);
                let was_in_select = state.select_depth > 0;
                let created_element = state.latest_element_is_foreign();
                let is_foreign = created_element.unwrap_or(was_foreign);
                let ignored =
                    created_element.is_none() || (was_in_select && is_table_structure_tag(&name));
                if name == "select" && !was_in_select && created_element.is_some() {
                    state.select_depth = state.select_depth.saturating_add(1);
                }
                Some(Token {
                    kind: TokenKind::Start {
                        name: name.clone(),
                        range: tag.start..tag.end,
                        attrs: tag.attrs,
                    },
                    foreign: is_foreign,
                    ignored,
                })
            }
            CallbackEvent::EndTag { name } => {
                let name = String::from_utf8_lossy(name).to_ascii_lowercase();
                let was_foreign = state.in_foreign_namespace();
                let was_in_select = state.select_depth > 0;
                state.process_tree_token(TreeToken::TagToken(Tag {
                    kind: TagKind::EndTag,
                    name: name.clone().into(),
                    self_closing: false,
                    attrs: Vec::new(),
                }));
                if name == "select" && was_in_select {
                    state.select_depth = state.select_depth.saturating_sub(1);
                }
                let ignored = was_in_select && is_table_structure_tag(&name);
                Some(Token {
                    kind: TokenKind::End {
                        name,
                        range: span.start..span.end,
                    },
                    foreign: was_foreign,
                    ignored,
                })
            }
            CallbackEvent::String { value } => {
                let foreign = state.in_foreign_namespace();
                let text = String::from_utf8_lossy(value).into_owned();
                state.process_tree_token(TreeToken::CharacterTokens(text.clone().into()));
                Some(Token {
                    kind: TokenKind::Text(text),
                    foreign,
                    ignored: false,
                })
            }
            CallbackEvent::Comment { value } => {
                let foreign = state.in_foreign_namespace();
                state.process_tree_token(TreeToken::CommentToken(
                    String::from_utf8_lossy(value).into_owned().into(),
                ));
                Some(Token {
                    kind: TokenKind::Comment,
                    foreign,
                    ignored: false,
                })
            }
            CallbackEvent::Doctype {
                name,
                public_identifier,
                system_identifier,
                force_quirks,
            } => {
                let foreign = state.in_foreign_namespace();
                state.process_tree_token(TreeToken::DoctypeToken(Doctype {
                    name: Some(String::from_utf8_lossy(name).into_owned().into()),
                    public_id: public_identifier
                        .map(|value| String::from_utf8_lossy(value).into_owned().into()),
                    system_id: system_identifier
                        .map(|value| String::from_utf8_lossy(value).into_owned().into()),
                    force_quirks,
                }));
                Some(Token {
                    kind: TokenKind::Comment,
                    foreign,
                    ignored: false,
                })
            }
            CallbackEvent::Error(error) => {
                let foreign = state.in_foreign_namespace();
                state.process_tree_token(TreeToken::ParseError(error.as_str().into()));
                Some(Token {
                    kind: TokenKind::Error,
                    foreign,
                    ignored: false,
                })
            }
        }
    };
    let emitter = ContextEmitter {
        inner: CallbackEmitter::new(callback),
        context: Rc::clone(&context),
    };
    let tokenizer = Tokenizer::new_with_emitter(text, emitter);
    let mut tokens = Vec::new();
    for result in tokenizer {
        let token = result.map_err(|_| HtmlError::OutputMismatch)?;
        if matches!(token.kind, TokenKind::Limit) {
            return Err(HtmlError::ResourceLimit);
        }
        tokens.push(token);
    }
    Ok(tokens)
}
