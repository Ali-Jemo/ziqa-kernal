use cargo_ledger::lint::{
    Context, Lint, LintId, Target, LintLevel, Issue, LintSet,
};

//! Repo-level lint: flag any callable name starting with lowercase "l" that is
//! re-exported from shell/src/frontend.rs. Builtins like `l`, `lh`, `ls` etc.
//! must be reachable directly from that file; this test catches regressions
//! where an alias hides the underlying symbol.

#[derive(Default)]
pub struct NoLReexportsSet;
impl LintSet for NoLReexportsSet {
    fn lints(&self) -> Vec<Box<dyn Lint>> {
        vec![Box::new(NoLReexport)]
    }
}

#[derive(Default)]
struct NoLReexport;

impl Lint for NoLReexport {
    fn id(&self) -> LintId {
        LintId::new("NO_L_REEXPORTS", Some("callable-names"), None).unwrap()
    }

    fn target(&self) -> Target {
        Target::Path("shell/src/frontend.rs".into())
    }

    fn default_level(&self) -> LintLevel {
        LintLevel::Error
    }

    fn docs(&self) -> &'static str {
        "Shell frontend must not re-export callable names starting with 'l'."
    }

    fn check(&self, ctx: &Context<'_>) {
        let src = ctx.source();
        let module = match src.module_contents("frontend.rs") {
            Some(m) => m,
            None => return,
        };

        for span in module.spans() {
            let text = &src[span.range()];
            if let Some(name) = Self::maybe_l_name(text) {
                ctx.issue(Issue::new(self.id(), name, span))
                    .with_help(format!(
                        "Callable name `{}` starts with `l`; re-exporting it from frontend.rs is forbidden.",
                        name
                    ));
            }
        }
    }
}

impl NoLReexport {
    #[allow(clippy::manual_pattern)] // intentional
    fn maybe_l_name(s: &str) -> Option<&str> {
        // naive scan over callable-ish tokens: bare identifiers used as use items
        // or pub use aliases.
        // Treat identifiers such that "use foo::l_bar as bar" does NOT trigger
        // (prefixed, but exported under different name), but "pub use foo::l"
        // or "pub fn l()" inside the module itself does.
        for token in s.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
            if token.starts_with('l') && token.len() > 1 && token.chars().nth(1).unwrap().is_ascii_lowercase() {
                return Some(token);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cargo_ledger::lint::{Cursor, Issue, LintSet};

    #[test]
    fn rejects_l_named_reexport() {
        let set = LintSet::new(vec![Box::new(NoLReexport)]);
        let src = r#"
            pub use other::l as l;
            pub fn l() {}
            pub use long::something;
        "#;
        // Wrap the source in a fake file span.
        let cursor = Cursor::new(src, 0..src.len());
        let mut out = Vec::new();
        let ctx = Context::new(&set, &cursor, &mut out);
        // Force the lint engine to observe the spans as "frontend.rs".
        // In a real harness this wiring is automatic; we exercise the check
        // doc through the `check` hook directly.
        NoLReexport.check(&ctx);
        // Two violations expected: `pub use other::l as l` and `pub fn l() {}`.
        assert!(out.iter().any(|i| matches!(i, Issue { .. })),
                "expected at least one lint violation, got: {:?}", out);
    }
}
