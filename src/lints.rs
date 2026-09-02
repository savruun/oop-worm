/*!
The four lints, over the model of one file.

`private_access` is the reason the worm exists: an underscore or
`@private` member used outside a function or a table of its class. The
receiver rules keep the idioms alive: `self` reaches everything,
`Class.__index = Class` stays legal in the defining file, and a sibling
instance's privates stay reachable inside the class's own functions.
*/

use crate::scan::{Model, Recv};
use larvae_worm::wire::Finding;

pub fn findings(src: &str, model: &Model) -> Vec<Finding> {
    let mut out = Vec::new();

    private_access(model, &mut out);
    implicit_self(src, model, &mut out);
    missing_index(model, &mut out);
    unused_private(model, &mut out);

    out.sort_by_key(|f| f.span.0);
    out
}

fn private_access(model: &Model, out: &mut Vec<Finding>) {
    for access in &model.accesses {
        if access.def_head {
            continue;
        }

        // The class this receiver names in this file, when it names one.
        let receiver_class = match &access.recv {
            Recv::Named(name) => model.class_named(name),

            _ => None,
        };

        let doc_private = receiver_class
            .is_some_and(|c| c.declares_private(&access.name) && !access.name.starts_with('_'));

        if !access.name.starts_with('_') && !doc_private {
            continue;
        }

        if access.recv == Recv::Zelf {
            continue;
        }

        /*
        The global containers hold globals, not the privates of a class.
        `_G._BLINK` is how generated code namespaces itself, and an
        Instance child can carry any name its author gave it.
        */
        if let Recv::Named(name) = &access.recv
            && matches!(
                name.as_str(),
                "_G" | "shared" | "game" | "script" | "workspace" | "plugin"
            )
        {
            continue;
        }

        /*
        The defining file owns its metamethods: `Class.__index = Class`
        is the constructor idiom, and it lives at module scope by
        design. Single underscore members and `@private` ones stay
        behind the context rule even on the class receiver, because a
        module scope call of a private helper is exactly what the lint
        is for.
        */
        if receiver_class.is_some() && access.name.starts_with("__") {
            continue;
        }

        // Inside a function or a table of a class that declares the member.
        if model
            .context_at(access.span.0)
            .is_some_and(|c| c.members.contains_key(&access.name))
        {
            continue;
        }

        let owner: Vec<&str> = model
            .classes
            .iter()
            .filter(|c| c.declares_private(&access.name))
            .map(|c| c.name.as_str())
            .collect();

        let (message, help) = match owner.as_slice() {
            [one] => (
                format!(
                    "`{}` is private to `{one}`, and this is outside `{one}`'s own functions",
                    access.name
                ),
                format!(
                    "use it from a function of `{one}`, or drop the underscore to make it public"
                ),
            ),

            _ => (
                format!(
                    "`{}` reads as private, and this is outside any function of its table",
                    access.name
                ),
                "an underscore name belongs to the table that defines it".to_string(),
            ),
        };

        out.push(Finding::new("private_access", access.span, message).with_help(help));
    }
}

fn implicit_self(src: &str, model: &Model, out: &mut Vec<Finding>) {
    for &(idx, span) in &model.colon_defs {
        let class = &model.classes[idx].name;
        let head = &src[span.0 as usize..span.1 as usize];

        out.push(
            Finding::new(
                "implicit_self",
                span,
                format!("the colon leaves self untyped in `function {head}`"),
            )
            .with_help(format!(
                "write `function {class}.{}(self: typeof({class}))` so self keeps the class type",
                head.split(':').next_back().unwrap_or("m")
            )),
        );
    }
}

fn missing_index(model: &Model, out: &mut Vec<Finding>) {
    let mut said: Vec<usize> = Vec::new();

    for &(idx, span) in &model.setmeta {
        if said.contains(&idx) {
            continue;
        }

        let class = &model.classes[idx];

        if class.members.contains_key("__index") {
            continue;
        }

        said.push(idx);
        out.push(
            Finding::new(
                "missing_index",
                span,
                format!(
                    "`{}` becomes a metatable here, and `{}.__index` is never set",
                    class.name, class.name
                ),
            )
            .with_help(format!(
                "add `{}.__index = {}` so method calls on the instances resolve",
                class.name, class.name
            )),
        );
    }
}

fn unused_private(model: &Model, out: &mut Vec<Finding>) {
    // Two classes can learn one member from one write; one finding is enough.
    let mut said: Vec<(u32, u32)> = Vec::new();

    for class in &model.classes {
        for (name, member) in &class.members {
            /*
            A member marked `@lifecycle` is called from outside this
            file: a framework calls the methods it names, and the file
            that declares one never does. Reporting it as unused tells
            the author to delete the method the framework needs.
            */
            if !member.private
                || member.lifecycle
                || name.starts_with("__")
                || said.contains(&member.span)
            {
                continue;
            }

            let read = model
                .accesses
                .iter()
                .any(|a| a.name == *name && !a.write && !a.def_head && a.span != member.span);

            if read {
                continue;
            }

            said.push(member.span);
            out.push(
                Finding::new(
                    "unused_private",
                    member.span,
                    format!(
                        "`{name}` is private to `{}` and nothing in this file uses it",
                        class.name
                    ),
                )
                .with_help("outside files must not use a private member, so this one can go"),
            );
        }
    }
}
