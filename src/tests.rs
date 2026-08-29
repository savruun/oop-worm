/*!
The scenarios the worm exists for, each as one small file.
*/

use crate::lints::findings;
use crate::lsp::{filter_completions, mark_hover};
use crate::scan::{scan, Recv};
use serde_json::{json, Value};

fn lint_names(src: &str) -> Vec<(String, String)> {
    findings(src, &scan(src))
        .into_iter()
        .map(|f| {
            let at = src[f.span.0 as usize..f.span.1 as usize].to_string();
            (f.lint, at)
        })
        .collect()
}

const CLASS: &str = "\
local Class = {}
Class.__index = Class

function Class.new()
	local self = setmetatable({}, Class)
	self._hp = 10
	return self
end

function Class:damage(other)
	self._hp -= 1
	other._hp -= 1
	return Class._clamp(self._hp)
end

function Class._clamp(hp)
	return hp
end

return Class
";

/// The idioms of a class file raise no private_access at all.
#[test]
fn the_class_idioms_stay_quiet() {
    let noisy: Vec<_> = lint_names(CLASS)
        .into_iter()
        .filter(|(lint, _)| lint == "private_access")
        .collect();

    assert_eq!(noisy, Vec::<(String, String)>::new());
}

/// `__index = Class` on the class receiver never lints, wherever it sits.
#[test]
fn the_index_assignment_is_legal() {
    let found = lint_names("local C = {}\nC.__index = C\nreturn C\n");

    assert!(
        found.iter().all(|(l, _)| l != "private_access"),
        "{found:?}"
    );
}

/// An underscore member poked from module scope is the finding.
#[test]
fn module_scope_private_use_denies() {
    let src = "\
local Class = {}

function Class._helper()
	return 1
end

local x = Class._helper()
return x
";
    let found = lint_names(src);

    assert!(
        found
            .iter()
            .any(|(l, at)| l == "private_access" && at == "_helper"),
        "{found:?}"
    );
}

/// Another file's object never reaches an underscore member.
#[test]
fn a_foreign_object_is_denied() {
    let src = "\
local Enemy = require(script.Parent.Enemy)

local e = Enemy.new()
print(e._hp)
";
    let found = lint_names(src);

    assert!(
        found
            .iter()
            .any(|(l, at)| l == "private_access" && at == "_hp"),
        "{found:?}"
    );
}

/// A table of the table is a context: privates work inside it.
#[test]
fn a_table_of_the_table_reaches_privates() {
    let src = "\
local Class = {}

function Class._step()
	return 1
end

Class.handlers = {
	tick = function()
		return Class._step()
	end,
}

return Class
";
    let found = lint_names(src);

    assert!(
        found.iter().all(|(l, _)| l != "private_access"),
        "{found:?}"
    );
}

/// The constructor table itself is a context too.
#[test]
fn the_constructor_table_is_a_context() {
    let src = "\
local Class = {
	_max = 100,
	limit = function(v)
		return Class._max
	end,
}

return Class
";
    let found = lint_names(src);

    assert!(
        found.iter().all(|(l, _)| l != "private_access"),
        "{found:?}"
    );
}

/// A sibling instance's private resolves inside the class's functions.
#[test]
fn a_sibling_instance_is_reachable_inside() {
    let src = "\
local Vec = {}
Vec.__index = Vec

function Vec.new(x)
	local self = setmetatable({}, Vec)
	self._x = x
	return self
end

function Vec:add(other)
	return Vec.new(self._x + other._x)
end

return Vec
";
    let found = lint_names(src);

    assert!(
        found.iter().all(|(l, _)| l != "private_access"),
        "{found:?}"
    );
}

/// `@private` marks a plain name, and the class receiver stops working outside.
#[test]
fn a_doc_private_member_is_covered() {
    let src = "\
local Store = {}

--- Saves without checks.
--- @private
function Store.rawSave()
	return 1
end

Store.rawSave()
return Store
";
    let found = lint_names(src);

    assert!(
        found
            .iter()
            .any(|(l, at)| l == "private_access" && at == "rawSave"),
        "{found:?}"
    );
}

/// The same call from inside a function of the class stays legal.
#[test]
fn a_doc_private_member_works_inside() {
    let src = "\
local Store = {}

--- @private
function Store.rawSave()
	return 1
end

function Store.save()
	return Store.rawSave()
end

return Store
";
    let found = lint_names(src);

    assert!(
        found.iter().all(|(l, _)| l != "private_access"),
        "{found:?}"
    );
}

/// The colon head warns once per method, naming the class.
#[test]
fn a_colon_method_warns() {
    let found = lint_names(CLASS);

    assert!(
        found
            .iter()
            .any(|(l, at)| l == "implicit_self" && at == "Class:damage"),
        "{found:?}"
    );
}

/// A metatable with no __index warns at the setmetatable site.
#[test]
fn a_metatable_without_index_warns() {
    let src = "\
local Class = {}

function Class.new()
	return setmetatable({}, Class)
end

return Class
";
    let found = lint_names(src);

    assert!(
        found
            .iter()
            .any(|(l, at)| l == "missing_index" && at == "Class"),
        "{found:?}"
    );

    // And setting it satisfies the lint.
    let fixed = lint_names(CLASS);
    assert!(fixed.iter().all(|(l, _)| l != "missing_index"), "{fixed:?}");
}

/// A private nothing reads is dead, and a metamethod never is.
#[test]
fn an_unused_private_warns() {
    let src = "\
local Class = {}
Class.__index = Class

function Class._forgotten()
	return 1
end

return Class
";
    let found = lint_names(src);

    assert!(
        found
            .iter()
            .any(|(l, at)| l == "unused_private" && at == "_forgotten"),
        "{found:?}"
    );
    assert!(found.iter().all(|(_, at)| at != "__index"), "{found:?}");
}

fn items(labels: &[&str]) -> Value {
    json!(labels
        .iter()
        .map(|l| json!({ "label": l, "kind": 5 }))
        .collect::<Vec<_>>())
}

fn kept(context: &Value, response: Value) -> Vec<String> {
    let out = filter_completions(context, response.clone()).unwrap_or(response);

    out.as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap().to_string())
        .collect()
}

/// Outside the class, the dot hides underscore members; inside, it shows them.
#[test]
fn completions_hide_privates_outside() {
    let src = "\
local Class = {}

function Class._helper()
	return 1
end

function Class.run()
	return Class.
end

local x = Class.
";
    // Outside: the trailing `Class.` at the bottom.
    let outside = (src.len() - 1) as u64;
    let context = json!({ "path": "/p/a.luau", "text": src, "offset": outside });

    assert_eq!(
        kept(&context, items(&["run", "_helper"])),
        vec!["run".to_string()]
    );

    // Inside `Class.run`: the `Class.` before the second end.
    let inside = (src.find("Class.\nend").unwrap() + 6) as u64;
    let context = json!({ "path": "/p/a.luau", "text": src, "offset": inside });

    assert_eq!(
        kept(&context, items(&["run", "_helper"])),
        vec!["run".to_string(), "_helper".to_string()]
    );
}

/// `self.` keeps everything, wherever it appears.
#[test]
fn completions_keep_self_whole() {
    let src = "local x = self.\n";
    let context = json!({ "path": "/p/a.luau", "text": src, "offset": (src.len() - 1) as u64 });

    assert_eq!(
        kept(&context, items(&["_hp", "damage"])),
        vec!["_hp".to_string(), "damage".to_string()]
    );
}

/// A `@private` doc on an item hides it, even with a public name.
#[test]
fn completions_read_the_docs() {
    let src = "local o = require(x)\nlocal y = o.\n";
    let context = json!({ "path": "/p/a.luau", "text": src, "offset": (src.len() - 1) as u64 });

    let response = json!([
        { "label": "save", "kind": 3 },
        { "label": "rawSave", "kind": 3, "documentation": { "kind": "markdown", "value": "Saves.\n@private" } },
    ]);

    assert_eq!(kept(&context, response), vec!["save".to_string()]);
}

/// The dunder members stay offered on the class name in its own file.
#[test]
fn completions_keep_dunders_on_the_class() {
    let src = "local Class = {}\nClass.__index = Class\nlocal m = Class.\n";
    let context = json!({ "path": "/p/a.luau", "text": src, "offset": (src.len() - 1) as u64 });

    assert_eq!(
        kept(&context, items(&["__index", "_secret", "new"])),
        vec!["__index".to_string(), "new".to_string()]
    );
}

/// A fully typed private hovers, and the card says it is private.
#[test]
fn hover_marks_a_private() {
    let src = "\
local Class = {}

function Class._helper()
	return 1
end

local x = Class._helper
";
    let at = (src.rfind("_helper").unwrap() + 2) as u64;
    let context = json!({ "path": "/p/a.luau", "text": src, "offset": at });
    let response = json!({ "contents": { "kind": "markdown", "value": "```luau\nfunction Class._helper(): number\n```" } });

    let marked = mark_hover(&context, response).expect("the hover gains a note");
    let value = marked["contents"]["value"].as_str().unwrap();

    assert!(
        value.contains("private by convention of `Class`"),
        "{value}"
    );
    assert!(value.contains("function Class._helper"), "{value}");
}

/// The receiver reader agrees with the scanner's classification.
#[test]
fn the_receiver_reader_matches() {
    let model = scan("local a = foo.bar\nlocal b = a.b._x\nlocal c = self._y\n");

    let recvs: Vec<Recv> = model.accesses.iter().map(|a| a.recv.clone()).collect();

    assert!(recvs.contains(&Recv::Named("foo".to_string())), "{recvs:?}");
    assert!(recvs.contains(&Recv::Complex), "{recvs:?}");
    assert!(recvs.contains(&Recv::Zelf), "{recvs:?}");
}

/// The constructor write teaches the class its instance members.
#[test]
fn the_constructor_write_grows_the_class() {
    let src = "\
local Class = {}
Class.__index = Class

function Class.new()
	local self = setmetatable({}, Class)
	self._hp = 10
	return self
end

return Class
";
    let model = scan(src);
    let class = model.class_named("Class").expect("Class is found");

    assert!(
        class.declares_private("_hp"),
        "members: {:?}",
        class.members.keys().collect::<Vec<_>>()
    );
}
