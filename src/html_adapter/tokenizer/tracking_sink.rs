use std::borrow::Cow;
use std::cell::RefCell;

use html5ever::interface::tree_builder::{
    ElementFlags, NextParserState, NodeOrText, QuirksMode, TreeSink,
};
use html5ever::{Attribute, ExpandedName, QualName};
use markup5ever_rcdom::{Handle, RcDom};

#[derive(Default)]
pub(super) struct TrackingSink {
    dom: RcDom,
    pub(super) last_created: RefCell<Option<Handle>>,
}

impl TreeSink for TrackingSink {
    type Handle = Handle;
    type Output = RcDom;
    type ElemName<'a> = ExpandedName<'a>;

    /// Returns the completed DOM tree.
    fn finish(self) -> Self::Output {
        self.dom
    }

    /// Stores a parser error in the DOM.
    fn parse_error(&self, message: Cow<'static, str>) {
        self.dom.parse_error(message);
    }

    /// Returns the root document node.
    fn get_document(&self) -> Self::Handle {
        self.dom.get_document()
    }

    /// Returns the expanded name of a DOM node.
    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        self.dom.elem_name(target)
    }

    /// Creates an element and records it for namespace checks.
    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> Self::Handle {
        let node = self.dom.create_element(name, attrs, flags);
        self.last_created.replace(Some(node.clone()));
        node
    }

    /// Creates a comment node in the DOM.
    fn create_comment(&self, text: html5ever::tendril::StrTendril) -> Self::Handle {
        self.dom.create_comment(text)
    }

    /// Creates a processing instruction node in the DOM.
    fn create_pi(
        &self,
        target: html5ever::tendril::StrTendril,
        data: html5ever::tendril::StrTendril,
    ) -> Self::Handle {
        self.dom.create_pi(target, data)
    }

    /// Adds a child node to its parent.
    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        self.dom.append(parent, child);
    }

    /// Adds a child using html5ever's parent and sibling rules.
    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        previous: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        self.dom
            .append_based_on_parent_node(element, previous, child);
    }

    /// Adds the parsed doctype to the document.
    fn append_doctype_to_document(
        &self,
        name: html5ever::tendril::StrTendril,
        public_id: html5ever::tendril::StrTendril,
        system_id: html5ever::tendril::StrTendril,
    ) {
        self.dom
            .append_doctype_to_document(name, public_id, system_id);
    }

    /// Marks a script node as already started.
    fn mark_script_already_started(&self, node: &Self::Handle) {
        self.dom.mark_script_already_started(node);
    }

    /// Moves the tree builder back to the given node.
    fn pop(&self, node: &Self::Handle) {
        self.dom.pop(node);
    }

    /// Returns the content fragment owned by a template node.
    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        self.dom.get_template_contents(target)
    }

    /// Checks whether two handles refer to the same DOM node.
    fn same_node(&self, left: &Self::Handle, right: &Self::Handle) -> bool {
        self.dom.same_node(left, right)
    }

    /// Sets the document's HTML quirks mode.
    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.dom.set_quirks_mode(mode);
    }

    /// Places a new node before its sibling.
    fn append_before_sibling(&self, sibling: &Self::Handle, new_node: NodeOrText<Self::Handle>) {
        self.dom.append_before_sibling(sibling, new_node);
    }

    /// Adds only attributes that the target does not already have.
    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        self.dom.add_attrs_if_missing(target, attrs);
    }

    /// Links form controls to their owning form node.
    fn associate_with_form(
        &self,
        target: &Self::Handle,
        form: &Self::Handle,
        nodes: (&Self::Handle, Option<&Self::Handle>),
    ) {
        self.dom.associate_with_form(target, form, nodes);
    }

    /// Removes a node from its parent.
    fn remove_from_parent(&self, target: &Self::Handle) {
        self.dom.remove_from_parent(target);
    }

    /// Moves a node's children under a new parent.
    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        self.dom.reparent_children(node, new_parent);
    }

    /// Checks whether a MathML annotation node allows HTML integration.
    fn is_mathml_annotation_xml_integration_point(&self, handle: &Self::Handle) -> bool {
        self.dom.is_mathml_annotation_xml_integration_point(handle)
    }

    /// Updates the current source line used by parser errors.
    fn set_current_line(&self, line_number: u64) {
        self.dom.set_current_line(line_number);
    }

    /// Completes script handling and returns the tokenizer's next state.
    fn complete_script(&self, node: &Self::Handle) -> NextParserState {
        self.dom.complete_script(node)
    }

    /// Checks whether the parent accepts a declarative shadow root.
    fn allow_declarative_shadow_roots(&self, parent: &Self::Handle) -> bool {
        self.dom.allow_declarative_shadow_roots(parent)
    }

    /// Attaches a declarative shadow root to its host node.
    fn attach_declarative_shadow(
        &self,
        location: &Self::Handle,
        attrs: Vec<Attribute>,
    ) -> Result<(), String> {
        self.dom.attach_declarative_shadow(location, attrs)
    }
}
