//! HTML5 parsing without script execution; iterative traversal handles deep documents.

use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{Value, json};

pub struct Extracted {
    pub title: String,
    pub text: String,
    pub links: Vec<Value>,
    pub scripts: bool,
}

pub fn html(input: &str, final_url: &url::Url) -> Extracted {
    let dom = parse_document(RcDom::default(), html5ever::ParseOpts::default()).one(input);
    let mut result =
        Extracted { title: String::new(), text: String::new(), links: Vec::new(), scripts: false };
    let mut stack: Vec<(Handle, bool, bool)> = vec![(dom.document.clone(), false, false)];
    let mut base = final_url.clone();
    while let Some((node, exiting, title)) = stack.pop() {
        match &node.data {
            NodeData::Text { contents } if !exiting => {
                let text = contents.borrow();
                if title {
                    result.title.push_str(&text);
                } else {
                    result.text.push_str(&text);
                }
            }
            NodeData::Element { name, attrs, .. } => {
                let name = name.local.as_ref();
                if name == "script" {
                    result.scripts = true;
                }
                if matches!(name, "script" | "style" | "noscript" | "template" | "svg") {
                    continue;
                }
                if name == "head" && !exiting {
                    result.scripts |= node.children.borrow().iter().any(|child| matches!(&child.data, NodeData::Element { name, .. } if name.local.as_ref() == "script"));
                    // Keep title/base metadata but omit head text from the body.
                    for child in node.children.borrow().iter().rev() {
                        if matches!(&child.data, NodeData::Element { name, .. } if matches!(name.local.as_ref(), "title" | "base"))
                        {
                            stack.push((child.clone(), false, true));
                        }
                    }
                    continue;
                }
                let block = matches!(
                    name,
                    "p" | "div"
                        | "section"
                        | "article"
                        | "main"
                        | "header"
                        | "footer"
                        | "nav"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "li"
                        | "tr"
                        | "pre"
                        | "blockquote"
                        | "br"
                );
                if block && !result.text.ends_with('\n') {
                    result.text.push('\n');
                }
                if exiting {
                    continue;
                }
                for attr in attrs.borrow().iter() {
                    if attr.name.local.as_ref() == "href"
                        && let Ok(target) = base.join(&attr.value)
                    {
                        if name == "base" && matches!(target.scheme(), "http" | "https") {
                            base = target;
                        } else if name == "a"
                            && matches!(target.scheme(), "http" | "https")
                            && result.links.len() < 500
                        {
                            result
                                .links
                                .push(json!({"url": target.as_str(), "text": label(&node)}));
                        }
                    }
                }
                stack.push((node.clone(), true, title));
                for child in node.children.borrow().iter().rev() {
                    stack.push((child.clone(), false, title || name == "title"));
                }
            }
            _ if !exiting => {
                for child in node.children.borrow().iter().rev() {
                    stack.push((child.clone(), false, title));
                }
            }
            _ => {}
        }
    }
    // RcDom nodes clear descendant child lists when their final owner drops.
    // Keep the document owner alive until every queued descendant has been visited.
    drop(dom);
    normalize(&mut result);
    result
}

fn normalize(result: &mut Extracted) {
    // Preserve preformatted indentation and line content; remove only excess blank lines.
    let mut text = String::new();
    let mut blank = true;
    for line in result.text.lines() {
        if line.trim().is_empty() {
            if !blank {
                text.push('\n');
            }
            blank = true;
        } else {
            text.push_str(line.trim_end());
            text.push('\n');
            blank = false;
        }
    }
    text.trim_matches('\n').clone_into(&mut result.text);
    result.title = result.title.split_whitespace().collect::<Vec<_>>().join(" ");
}

fn label(node: &Handle) -> String {
    let mut text = String::new();
    let mut stack = vec![node.clone()];
    while let Some(node) = stack.pop() {
        if let NodeData::Text { contents } = &node.data {
            text.extend(contents.borrow().chars().take(200));
        }
        if text.len() > 500 {
            break;
        }
        stack.extend(node.children.borrow().iter().rev().cloned());
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
