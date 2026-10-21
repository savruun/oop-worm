/*!
The oop worm: privacy conventions and class ergonomics for Luau.

The worm reads plain Luau, so it claims no extension. Its lints run over
every `.luau` file through `lints_luau`, and over the Luau shadow of a
claimed file when the claiming worm shares. Everything rests on one
lexical model in `scan`.
*/

use oop_worm::{lints, scan};

use larvae_worm::native::{Handler, serve};
use larvae_worm::wire::Lint;

fn main() {
    serve(OopWorm)
}

struct OopWorm;

impl Handler for OopWorm {
    fn lint(&mut self, source: &str) -> Result<Lint, String> {
        let model = scan::scan(source);

        Ok(Lint {
            findings: lints::findings(source, &model),
            comments: model.comments.clone(),
            luau: None,
        })
    }

    fn manifest(&self) -> Option<&'static str> {
        Some(include_str!("../worm.toml"))
    }
}
