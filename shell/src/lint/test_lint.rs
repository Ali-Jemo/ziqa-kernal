use cargo_ledger::lint::{Target, LintSet, LintId, lint};

struct PrefixLint;
impl lint::Lint for PrefixLint {
    fn id(&self) -> LintId { LintId::of_const("PREFIX_L") }
    fn target(&self) -> Target { Target::new_static() }

    fn check(&self, ctx: &lint::Context<'_>) {
        // Reject any callable name starting with "l" in shell/mod.rs.
        todo!()
    }
}

fn main() {}
