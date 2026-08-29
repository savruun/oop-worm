/*!
The respond hooks: private members leave the completion list outside
their class, and hover names a private member as one.

The filter mirrors the lint's allow rules, so the list and the squiggle
never disagree: where the lint stays quiet, the member completes.
*/

use crate::scan::{self, Recv};
use serde_json::Value;

/// The receiver and the cursor's byte offset, read from the request context
fn access_at(text: &str, offset: usize) -> Option<(Recv, usize)> {
    let b = text.as_bytes();
    let mut i = offset.min(b.len());

    // Back over the partial member name the author typed.
    while i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        i -= 1;
    }

    if i == 0 || !(b[i - 1] == b'.' || b[i - 1] == b':') {
        return None;
    }

    let dot = i - 1;

    // `..` and `::` are not member access.
    if dot > 0 && (b[dot - 1] == b'.' || b[dot - 1] == b':') {
        return None;
    }

    let mut j = dot;
    while j > 0 && (b[j - 1].is_ascii_alphanumeric() || b[j - 1] == b'_') {
        j -= 1;
    }

    if j == dot {
        return Some((Recv::Complex, dot));
    }

    let chained = j > 0 && (b[j - 1] == b'.' || b[j - 1] == b':' || b[j - 1] == b')');
    let name = &text[j..dot];

    Some(match (chained, name) {
        (true, _) => (Recv::Complex, dot),
        (false, "self") => (Recv::Zelf, dot),
        (false, other) => (Recv::Named(other.to_string()), dot),
    })
}

/// Report if one completion item reads as private
fn private_item(item: &Value, receiver: Option<&scan::Class>) -> bool {
    let Some(label) = item["label"].as_str() else {
        return false;
    };

    if label.starts_with('_') {
        return true;
    }

    if receiver.is_some_and(|c| c.declares_private(label)) {
        return true;
    }

    let docs = match &item["documentation"] {
        Value::String(s) => s.as_str(),
        other => other["value"].as_str().unwrap_or(""),
    };

    docs.contains("@private")
}

pub fn filter_completions(context: &Value, response: Value) -> Option<Value> {
    let text = context["text"].as_str()?;
    let offset = context["offset"].as_u64()? as usize;
    let (recv, dot) = access_at(text, offset)?;

    if recv == Recv::Zelf {
        return None;
    }

    let model = scan::scan(text);

    let receiver_class = match &recv {
        Recv::Named(name) => model.class_named(name),

        _ => None,
    };

    /*
    Inside a function or a table of the class the receiver names, the
    author is home: everything completes. On any other receiver, the
    cursor's own class context keeps its declared members, so a sibling
    instance completes its privates inside the class's functions.
    */
    let home = receiver_class.is_some_and(|c| c.owns(dot as u32));
    if home {
        return None;
    }

    let context_class = model.context_at(dot as u32);

    let keep = |item: &Value| {
        if !private_item(item, receiver_class) {
            return true;
        }

        let label = item["label"].as_str().unwrap_or("");

        // The defining file keeps its metamethod idioms on the class name.
        if receiver_class.is_some() && label.starts_with("__") {
            return true;
        }

        context_class.is_some_and(|c| c.members.contains_key(label))
    };

    let filtered = match response {
        Value::Array(items) => {
            let kept: Vec<Value> = items.iter().filter(|i| keep(i)).cloned().collect();

            match kept.len() == items.len() {
                true => return None,

                false => Value::Array(kept),
            }
        }

        Value::Object(mut map) => {
            let Some(Value::Array(items)) = map.get("items").cloned() else {
                return None;
            };

            let kept: Vec<Value> = items.iter().filter(|i| keep(i)).cloned().collect();

            if kept.len() == items.len() {
                return None;
            }

            map.insert("items".into(), Value::Array(kept));
            Value::Object(map)
        }

        _ => return None,
    };

    Some(filtered)
}

pub fn mark_hover(context: &Value, response: Value) -> Option<Value> {
    let text = context["text"].as_str()?;
    let offset = context["offset"].as_u64()? as usize;
    let b = text.as_bytes();

    // The ident under the cursor, and the accessor before it.
    let mut s = offset.min(b.len());
    while s > 0 && (b[s - 1].is_ascii_alphanumeric() || b[s - 1] == b'_') {
        s -= 1;
    }

    let mut e = offset.min(b.len());
    while e < b.len() && (b[e].is_ascii_alphanumeric() || b[e] == b'_') {
        e += 1;
    }

    let name = &text[s..e];
    let accessed = s > 0 && (b[s - 1] == b'.' || b[s - 1] == b':');

    if !accessed || !name.starts_with('_') {
        return None;
    }

    let mut response = response;
    let value = response["contents"]["value"].as_str()?;

    let owner = scan::scan(text)
        .classes
        .iter()
        .find(|c| c.declares_private(name))
        .map(|c| format!(" of `{}`", c.name))
        .unwrap_or_default();

    let marked = format!("{value}\n\n---\nprivate by convention{owner}");
    response["contents"]["value"] = Value::String(marked);

    Some(response)
}
