/*!
The oop worm: privacy conventions and class ergonomics for Luau.

The worm reads plain Luau, so it claims no extension. Its lints run over
every `.luau` file through `lints_luau`, and over the Luau shadow of a
claimed file when the claiming worm shares. Its respond hooks drop
private members from completion lists outside their class and mark them
on hover. Everything rests on one lexical model in `scan`.
*/

use oop_worm::{lints, lsp, scan};

use larvae_worm::native::{Handler, Settings, serve};
use larvae_worm::wire::Lint;

fn main() {
    serve(OopWorm::default())
}

struct OopWorm {
    /// `[worms.oop.config] hide_private_completions`, read once at init
    hide_private_completions: bool,
}

impl Default for OopWorm {
    fn default() -> Self {
        Self {
            hide_private_completions: true,
        }
    }
}

impl Handler for OopWorm {
    fn init(&mut self, config: &str, _rules: &str, _settings: &Settings) -> Result<(), String> {
        // The host sends the config table as text; either spelling parses.
        let parsed: serde_json::Value = serde_json::from_str(config)
            .or_else(|_| {
                config
                    .parse::<toml::Table>()
                    .map_err(|e| e.to_string())
                    .and_then(|t| serde_json::to_value(t).map_err(|e| e.to_string()))
            })
            .unwrap_or_default();

        if let Some(on) = parsed["hide_private_completions"].as_bool() {
            self.hide_private_completions = on;
        }

        Ok(())
    }

    fn lint(&mut self, source: &str) -> Result<Lint, String> {
        let model = scan::scan(source);

        Ok(Lint {
            findings: lints::findings(source, &model),
            comments: model.comments.clone(),
            luau: None,
        })
    }

    fn lsp_respond(
        &mut self,
        kind: &str,
        context: &str,
        response: &str,
    ) -> Result<Option<serde_json::Value>, String> {
        let context: serde_json::Value = serde_json::from_str(context).unwrap_or_default();
        let response: serde_json::Value = serde_json::from_str(response).unwrap_or_default();

        Ok(match kind {
            "completions" if self.hide_private_completions => {
                lsp::filter_completions(&context, response)
            }

            "hover" => lsp::mark_hover(&context, response),

            _ => None,
        })
    }

    fn manifest(&self) -> Option<&'static str> {
        Some(include_str!("../worm.toml"))
    }
}
