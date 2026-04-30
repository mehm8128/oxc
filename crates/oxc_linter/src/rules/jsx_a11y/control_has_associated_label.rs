use std::ops::Deref;

use oxc_ast::{
    AstKind,
    ast::{JSXAttributeItem, JSXAttributeValue, JSXChild, JSXElement},
};
use oxc_diagnostics::OxcDiagnostic;
use oxc_macros::declare_oxc_lint;
use oxc_span::Span;
use oxc_str::CompactStr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AstNode,
    context::LintContext,
    globals::HTML_TAG,
    rule::Rule,
    utils::{
        get_element_type, get_jsx_attribute_name, get_string_literal_prop_value, has_jsx_prop,
        is_hidden_from_screen_reader, is_interactive_element, is_interactive_role,
        is_react_component_name,
    },
};

fn control_has_associated_label_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("A control must be associated with a text label.")
        .with_help(
            "Add a text label to the control element. This can be done by adding text content, an `aria-label` attribute, or an `aria-labelledby` attribute.",
        )
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct ControlHasAssociatedLabel(Box<ControlHasAssociatedLabelConfig>);

/// Elements that are always ignored (cannot reliably determine label source).
const DEFAULT_IGNORE_ELEMENTS: [&str; 1] = ["link"];

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ControlHasAssociatedLabelConfig {
    /// Maximum depth to search for an accessible label within the element.
    /// Defaults to `2`.
    depth: u8,
    /// Additional attributes to check for accessible label text.
    label_attributes: Vec<CompactStr>,
    /// Custom JSX components to be treated as interactive controls.
    control_components: Vec<CompactStr>,
    /// Elements to ignore (in addition to the default ignore list).
    /// Defaults to `["audio", "canvas", "embed", "input", "textarea", "tr", "video"]`.
    ignore_elements: Vec<CompactStr>,
    /// Interactive roles to ignore.
    /// Defaults to `["grid", "listbox", "menu", "menubar", "radiogroup", "row", "tablist", "toolbar", "tree", "treegrid"]`.
    ignore_roles: Vec<CompactStr>,
}

impl Deref for ControlHasAssociatedLabel {
    type Target = ControlHasAssociatedLabelConfig;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for ControlHasAssociatedLabelConfig {
    fn default() -> Self {
        Self {
            depth: 2,
            label_attributes: vec![],
            control_components: vec![],
            ignore_elements: vec![
                "audio".into(),
                "canvas".into(),
                "embed".into(),
                "input".into(),
                "textarea".into(),
                "tr".into(),
                "video".into(),
            ],
            ignore_roles: vec![
                "grid".into(),
                "listbox".into(),
                "menu".into(),
                "menubar".into(),
                "radiogroup".into(),
                "row".into(),
                "tablist".into(),
                "toolbar".into(),
                "tree".into(),
                "treegrid".into(),
            ],
        }
    }
}

declare_oxc_lint!(
    /// ### What it does
    ///
    /// Enforce that a control (an interactive element) has a text label.
    ///
    /// ### Why is this bad?
    ///
    /// An interactive element (such as a `<button>`) without an accessible
    /// text label makes it difficult or impossible for users of assistive
    /// technologies to understand the purpose of the control.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```jsx
    /// <button />
    /// <input type="text" />
    /// <a href="/path" />
    /// <th />
    /// <div role="button" />
    /// <div role="checkbox" />
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```jsx
    /// <button>Save</button>
    /// <button aria-label="Save" />
    /// <label>Name <input type="text" /></label>
    /// <a href="/path">Learn more</a>
    /// <th>Column Header</th>
    /// <div role="button">Submit</div>
    /// <div role="checkbox" aria-labelledby="label_id" />
    /// ```
    ControlHasAssociatedLabel,
    jsx_a11y,
    correctness,
    config = ControlHasAssociatedLabelConfig,
    version = "next",
);

impl Rule for ControlHasAssociatedLabel {
    fn from_configuration(value: serde_json::Value) -> Result<Self, serde_json::error::Error> {
        let mut config = ControlHasAssociatedLabelConfig::default();

        let Some(options) = value.get(0) else {
            return Ok(Self(Box::new(config)));
        };

        if let Some(depth) = options.get("depth").and_then(serde_json::Value::as_u64) {
            config.depth = std::cmp::min(depth, 25).try_into().unwrap();
        }

        if let Some(label_attributes) =
            options.get("labelAttributes").and_then(serde_json::Value::as_array)
        {
            config.label_attributes =
                label_attributes.iter().filter_map(|v| v.as_str().map(CompactStr::from)).collect();
        }

        if let Some(control_components) =
            options.get("controlComponents").and_then(serde_json::Value::as_array)
        {
            config.control_components = control_components
                .iter()
                .filter_map(|v| v.as_str().map(CompactStr::from))
                .collect();
        }

        if let Some(ignore_elements) =
            options.get("ignoreElements").and_then(serde_json::Value::as_array)
        {
            config.ignore_elements =
                ignore_elements.iter().filter_map(|v| v.as_str().map(CompactStr::from)).collect();
        }

        if let Some(ignore_roles) = options.get("ignoreRoles").and_then(serde_json::Value::as_array)
        {
            config.ignore_roles =
                ignore_roles.iter().filter_map(|v| v.as_str().map(CompactStr::from)).collect();
        }

        Ok(Self(Box::new(config)))
    }

    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::JSXElement(element) = node.kind() else {
            return;
        };

        let element_type = get_element_type(ctx, &element.opening_element);

        if DEFAULT_IGNORE_ELEMENTS.contains(&element_type.as_ref())
            || self.ignore_elements.iter().any(|e| e.as_str() == element_type.as_ref())
        {
            return;
        }

        let role =
            has_jsx_prop(&element.opening_element, "role").and_then(get_string_literal_prop_value);
        if let Some(role) = role
            && self.ignore_roles.iter().any(|r| r.as_str() == role)
        {
            return;
        }

        if is_hidden_from_screen_reader(ctx, &element.opening_element) {
            return;
        }

        let is_dom_element = HTML_TAG.contains(element_type.as_ref());
        let is_interactive_el = is_interactive_element(&element_type, &element.opening_element);
        let is_interactive_role_el = role.is_some_and(is_interactive_role);
        let is_control_component =
            self.control_components.iter().any(|c| c.as_str() == element_type.as_ref());

        if !(is_interactive_el || is_dom_element && is_interactive_role_el || is_control_component)
        {
            return;
        }

        if !self.may_have_accessible_label(element, ctx) {
            ctx.diagnostic(control_has_associated_label_diagnostic(element.opening_element.span));
        }
    }
}

impl ControlHasAssociatedLabel {
    fn may_have_accessible_label<'a>(
        &self,
        element: &JSXElement<'a>,
        ctx: &LintContext<'a>,
    ) -> bool {
        if self.has_labelling_prop(&element.opening_element.attributes) {
            return true;
        }

        for child in &element.children {
            if self.check_child_for_label(child, 1, ctx) {
                return true;
            }
        }

        false
    }

    fn has_labelling_prop(&self, attributes: &[JSXAttributeItem<'_>]) -> bool {
        let labelling_props: &[&str] = &["alt", "aria-label", "aria-labelledby"];

        attributes.iter().any(|attribute| match attribute {
            JSXAttributeItem::SpreadAttribute(_) => true,
            JSXAttributeItem::Attribute(attr) => {
                let attr_name = get_jsx_attribute_name(&attr.name);
                let is_labelling = labelling_props.iter().any(|p| *p == attr_name.as_ref())
                    || self.label_attributes.iter().any(|p| p.as_str() == attr_name.as_ref());
                if !is_labelling {
                    return false;
                }

                match &attr.value {
                    None => false,
                    Some(JSXAttributeValue::StringLiteral(s)) => {
                        !s.value.as_str().trim().is_empty()
                    }
                    Some(_) => true,
                }
            }
        })
    }

    fn check_child_for_label<'a>(
        &self,
        node: &JSXChild<'a>,
        depth: u8,
        ctx: &LintContext<'a>,
    ) -> bool {
        if depth > self.depth {
            return false;
        }

        match node {
            JSXChild::ExpressionContainer(_) => true,
            JSXChild::Text(text) => !text.value.as_str().trim().is_empty(),
            JSXChild::Element(element) => {
                if self.has_labelling_prop(&element.opening_element.attributes) {
                    return true;
                }

                if element.children.is_empty() {
                    let name = get_element_type(ctx, &element.opening_element);
                    if is_react_component_name(&name)
                        && !self.control_components.iter().any(|c| c.as_str() == name.as_ref())
                    {
                        return true;
                    }
                }

                for child in &element.children {
                    if self.check_child_for_label(child, depth + 1, ctx) {
                        return true;
                    }
                }

                false
            }
            JSXChild::Fragment(fragment) => {
                for child in &fragment.children {
                    if self.check_child_for_label(child, depth + 1, ctx) {
                        return true;
                    }
                }
                false
            }
            JSXChild::Spread(_) => false,
        }
    }
}

#[test]
fn test() {
    use crate::tester::Tester;

    let pass = vec![
        // Custom Control Components
        (
            r"<CustomControl><span><span>Save</span></span></CustomControl>",
            Some(serde_json::json!([{ "depth": 3, "controlComponents": ["CustomControl"] }])),
            None,
        ),
        (
            r#"<CustomControl><span><span label="Save"></span></span></CustomControl>"#,
            Some(
                serde_json::json!([{ "depth": 3, "controlComponents": ["CustomControl"], "labelAttributes": ["label"] }]),
            ),
            None,
        ),
        (
            r"<CustomControl>Save</CustomControl>",
            None,
            Some(
                serde_json::json!({ "settings": { "jsx-a11y": { "components": { "CustomControl": "button" } } } }),
            ),
        ),
        // Interactive Elements
        (r"<button>Save</button>", None, None),
        (r"<button><span>Save</span></button>", None, None),
        (
            r"<button><span><span>Save</span></span></button>",
            Some(serde_json::json!([{ "depth": 3 }])),
            None,
        ),
        (
            r"<button><span><span><span><span><span><span><span><span>Save</span></span></span></span></span></span></span></span></button>",
            Some(serde_json::json!([{ "depth": 9 }])),
            None,
        ),
        (r#"<button><img alt="Save" /></button>"#, None, None),
        (r#"<button aria-label="Save" />"#, None, None),
        (r#"<button><span aria-label="Save" /></button>"#, None, None),
        (r#"<button aria-labelledby="js_1" />"#, None, None),
        (r#"<button><span aria-labelledby="js_1" /></button>"#, None, None),
        (r"<button>{sureWhyNot}</button>", None, None),
        (
            r#"<button><span><span label="Save"></span></span></button>"#,
            Some(serde_json::json!([{ "depth": 3, "labelAttributes": ["label"] }])),
            None,
        ),
        (r##"<a href="#">Save</a>"##, None, None),
        (r##"<area href="#">Save</area>"##, None, None),
        (r"<link>Save</link>", None, None),
        (r"<menuitem>Save</menuitem>", None, None),
        (r"<option>Save</option>", None, None),
        (r"<th>Save</th>", None, None),
        // Interactive Roles
        (r#"<div role="button">Save</div>"#, None, None),
        (r#"<div role="checkbox">Save</div>"#, None, None),
        (r#"<div role="columnheader">Save</div>"#, None, None),
        (r#"<div role="combobox">Save</div>"#, None, None),
        (r#"<div role="gridcell">Save</div>"#, None, None),
        (r#"<div role="link">Save</div>"#, None, None),
        (r#"<div role="menuitem">Save</div>"#, None, None),
        (r#"<div role="menuitemcheckbox">Save</div>"#, None, None),
        (r#"<div role="menuitemradio">Save</div>"#, None, None),
        (r#"<div role="option">Save</div>"#, None, None),
        (r#"<div role="progressbar">Save</div>"#, None, None),
        (r#"<div role="radio">Save</div>"#, None, None),
        (r#"<div role="rowheader">Save</div>"#, None, None),
        (r#"<div role="searchbox">Save</div>"#, None, None),
        (r#"<div role="slider">Save</div>"#, None, None),
        (r#"<div role="spinbutton">Save</div>"#, None, None),
        (r#"<div role="switch">Save</div>"#, None, None),
        (r#"<div role="tab">Save</div>"#, None, None),
        (r#"<div role="textbox">Save</div>"#, None, None),
        (r#"<div role="treeitem">Save</div>"#, None, None),
        (r#"<div role="button" aria-label="Save" />"#, None, None),
        (r#"<div role="checkbox" aria-label="Save" />"#, None, None),
        (r#"<div role="columnheader" aria-label="Save" />"#, None, None),
        (r#"<div role="combobox" aria-label="Save" />"#, None, None),
        (r#"<div role="gridcell" aria-label="Save" />"#, None, None),
        (r#"<div role="link" aria-label="Save" />"#, None, None),
        (r#"<div role="menuitem" aria-label="Save" />"#, None, None),
        (r#"<div role="menuitemcheckbox" aria-label="Save" />"#, None, None),
        (r#"<div role="menuitemradio" aria-label="Save" />"#, None, None),
        (r#"<div role="option" aria-label="Save" />"#, None, None),
        (r#"<div role="progressbar" aria-label="Save" />"#, None, None),
        (r#"<div role="radio" aria-label="Save" />"#, None, None),
        (r#"<div role="rowheader" aria-label="Save" />"#, None, None),
        (r#"<div role="searchbox" aria-label="Save" />"#, None, None),
        (r#"<div role="slider" aria-label="Save" />"#, None, None),
        (r#"<div role="spinbutton" aria-label="Save" />"#, None, None),
        (r#"<div role="switch" aria-label="Save" />"#, None, None),
        (r#"<div role="tab" aria-label="Save" />"#, None, None),
        (r#"<div role="textbox" aria-label="Save" />"#, None, None),
        (r#"<div role="treeitem" aria-label="Save" />"#, None, None),
        (r#"<div role="button" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="checkbox" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="columnheader" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="combobox" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="gridcell" aria-labelledby="Save" />"#, None, None),
        (r#"<div role="link" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="menuitem" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="menuitemcheckbox" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="menuitemradio" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="option" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="progressbar" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="radio" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="rowheader" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="searchbox" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="slider" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="spinbutton" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="switch" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="tab" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="textbox" aria-labelledby="js_1" />"#, None, None),
        (r#"<div role="treeitem" aria-labelledby="js_1" />"#, None, None),
        // Non-interactive Elements
        (r"<abbr />", None, None),
        (r"<article />", None, None),
        (r"<blockquote />", None, None),
        (r"<br />", None, None),
        (r"<caption />", None, None),
        (r"<dd />", None, None),
        (r"<dfn />", None, None),
        (r"<dialog />", None, None),
        (r"<dir />", None, None),
        (r"<dl />", None, None),
        (r"<dt />", None, None),
        (r"<fieldset />", None, None),
        (r"<figcaption />", None, None),
        (r"<figure />", None, None),
        (r"<footer />", None, None),
        (r"<form />", None, None),
        (r"<frame />", None, None),
        (r"<h1 />", None, None),
        (r"<h2 />", None, None),
        (r"<h3 />", None, None),
        (r"<h4 />", None, None),
        (r"<h5 />", None, None),
        (r"<h6 />", None, None),
        (r"<hr />", None, None),
        (r"<img />", None, None),
        (r"<legend />", None, None),
        (r"<li />", None, None),
        (r"<link />", None, None),
        (r"<main />", None, None),
        (r"<mark />", None, None),
        (r"<marquee />", None, None),
        (r"<menu />", None, None),
        (r"<meter />", None, None),
        (r"<nav />", None, None),
        (r"<ol />", None, None),
        (r"<p />", None, None),
        (r"<pre />", None, None),
        (r"<progress />", None, None),
        (r"<ruby />", None, None),
        (r"<section />", None, None),
        (r"<table />", None, None),
        (r"<tbody />", None, None),
        (r"<tfoot />", None, None),
        (r"<thead />", None, None),
        (r"<time />", None, None),
        (r"<ul />", None, None),
        // Non-interactive Roles
        (r#"<div role="alert" />"#, None, None),
        (r#"<div role="alertdialog" />"#, None, None),
        (r#"<div role="application" />"#, None, None),
        (r#"<div role="article" />"#, None, None),
        (r#"<div role="banner" />"#, None, None),
        (r#"<div role="cell" />"#, None, None),
        (r#"<div role="complementary" />"#, None, None),
        (r#"<div role="contentinfo" />"#, None, None),
        (r#"<div role="definition" />"#, None, None),
        (r#"<div role="dialog" />"#, None, None),
        (r#"<div role="directory" />"#, None, None),
        (r#"<div role="document" />"#, None, None),
        (r#"<div role="feed" />"#, None, None),
        (r#"<div role="figure" />"#, None, None),
        (r#"<div role="form" />"#, None, None),
        (r#"<div role="group" />"#, None, None),
        (r#"<div role="heading" />"#, None, None),
        (r#"<div role="img" />"#, None, None),
        (r#"<div role="list" />"#, None, None),
        (r#"<div role="listitem" />"#, None, None),
        (r#"<div role="log" />"#, None, None),
        (r#"<div role="main" />"#, None, None),
        (r#"<div role="marquee" />"#, None, None),
        (r#"<div role="math" />"#, None, None),
        (r#"<div role="navigation" />"#, None, None),
        (r#"<div role="none" />"#, None, None),
        (r#"<div role="note" />"#, None, None),
        (r#"<div role="presentation" />"#, None, None),
        (r#"<div role="progressbar" />"#, None, None),
        (r#"<div role="region" />"#, None, None),
        (r#"<div role="rowgroup" />"#, None, None),
        (r#"<div role="search" />"#, None, None),
        (r#"<div role="status" />"#, None, None),
        (r#"<div role="table" />"#, None, None),
        (r#"<div role="tabpanel" />"#, None, None),
        (r#"<div role="term" />"#, None, None),
        (r#"<div role="timer" />"#, None, None),
        (r#"<div role="tooltip" />"#, None, None),
        // Via Config - Inputs (might get a label from a wrapping label element)
        (r"<input />", None, None),
        (r#"<input type="button" />"#, None, None),
        (r#"<input type="checkbox" />"#, None, None),
        (r#"<input type="color" />"#, None, None),
        (r#"<input type="date" />"#, None, None),
        (r#"<input type="datetime" />"#, None, None),
        (r#"<input type="email" />"#, None, None),
        (r#"<input type="file" />"#, None, None),
        (r#"<input type="hidden" />"#, None, None),
        (r#"<input type="hidden" name="bot-field"/>"#, None, None),
        (r#"<input type="hidden" name="form-name" value="Contact Form"/>"#, None, None),
        (r#"<input type="image" />"#, None, None),
        (r#"<input type="month" />"#, None, None),
        (r#"<input type="number" />"#, None, None),
        (r#"<input type="password" />"#, None, None),
        (r#"<input type="radio" />"#, None, None),
        (r#"<input type="range" />"#, None, None),
        (r#"<input type="reset" />"#, None, None),
        (r#"<input type="search" />"#, None, None),
        (r#"<input type="submit" />"#, None, None),
        (r#"<input type="tel" />"#, None, None),
        (r#"<input type="text" />"#, None, None),
        (r#"<label>Foo <input type="text" /></label>"#, None, None),
        (
            r#"<input name={field.name} id="foo" type="text" value={field.value} disabled={isDisabled} onChange={changeText(field.onChange, field.name)} onBlur={field.onBlur} />"#,
            None,
            None,
        ),
        (r#"<input type="time" />"#, None, None),
        (r#"<input type="url" />"#, None, None),
        (r#"<input type="week" />"#, None, None),
        // Marginal interactive elements
        (r"<audio />", None, None),
        (r"<canvas />", None, None),
        (r"<embed />", None, None),
        (r"<textarea />", None, None),
        (r"<tr />", None, None),
        (r"<video />", None, None),
        // Interactive roles to ignore
        (r#"<div role="grid" />"#, None, None),
        (r#"<div role="listbox" />"#, None, None),
        (r#"<div role="menu" />"#, None, None),
        (r#"<div role="menubar" />"#, None, None),
        (r#"<div role="radiogroup" />"#, None, None),
        (r#"<div role="row" />"#, None, None),
        (r#"<div role="tablist" />"#, None, None),
        (r#"<div role="toolbar" />"#, None, None),
        (r#"<div role="tree" />"#, None, None),
        (r#"<div role="treegrid" />"#, None, None),
    ];

    let fail = vec![
        (r"<button />", None, None),
        (r"<button><span /></button>", None, None),
        (r"<button><img /></button>", None, None),
        (r#"<button><span title="This is not a real label" /></button>"#, None, None),
        (
            r"<button><span><span><span>Save</span></span></span></button>",
            Some(serde_json::json!([{ "depth": 3 }])),
            None,
        ),
        (
            r"<CustomControl><span><span></span></span></CustomControl>",
            Some(serde_json::json!([{ "depth": 3, "controlComponents": ["CustomControl"] }])),
            None,
        ),
        (
            r"<CustomControl></CustomControl>",
            None,
            Some(
                serde_json::json!({ "settings": { "jsx-a11y": { "components": { "CustomControl": "button" } } } }),
            ),
        ),
        (r##"<a href="#" />"##, None, None),
        (r##"<area href="#" />"##, None, None),
        (r"<menuitem />", None, None),
        (r"<option />", None, None),
        (r"<th />", None, None),
        (r"<td />", None, None),
        // Interactive Roles
        (r#"<div role="button" />"#, None, None),
        (r#"<div role="checkbox" />"#, None, None),
        (r#"<div role="columnheader" />"#, None, None),
        (r#"<div role="combobox" />"#, None, None),
        (r#"<div role="link" />"#, None, None),
        (r#"<div role="gridcell" />"#, None, None),
        (r#"<div role="menuitem" />"#, None, None),
        (r#"<div role="menuitemcheckbox" />"#, None, None),
        (r#"<div role="menuitemradio" />"#, None, None),
        (r#"<div role="option" />"#, None, None),
        (r#"<div role="radio" />"#, None, None),
        (r#"<div role="rowheader" />"#, None, None),
        (r#"<div role="scrollbar" />"#, None, None),
        (r#"<div role="searchbox" />"#, None, None),
        (r#"<div role="slider" />"#, None, None),
        (r#"<div role="spinbutton" />"#, None, None),
        (r#"<div role="switch" />"#, None, None),
        (r#"<div role="tab" />"#, None, None),
        (r#"<div role="textbox" />"#, None, None),
        (r"<details />", None, None),
        (r"<iframe />", None, None),
        (r"<label />", None, None),
        (r#"<div role="separator" />"#, None, None),
    ];

    Tester::new(ControlHasAssociatedLabel::NAME, ControlHasAssociatedLabel::PLUGIN, pass, fail)
        .test_and_snapshot();
}
