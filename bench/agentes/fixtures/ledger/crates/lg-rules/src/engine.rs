//! Escolha da regra vencedora.
//!
//! Quando várias regras casam com a mesma descrição, vence, nesta ordem:
//! 1. a de maior `priority`;
//! 2. em empate, a mais específica (mais caracteres literais no padrão);
//! 3. persistindo o empate, a definida primeiro.
//!
//! Transações que já chegam com categoria (coluna `categoria` do extrato) são mantidas,
//! a menos que `override_existing` esteja ligado.

use std::cmp::Ordering;

use lg_core::Txn;

use crate::pattern::Pattern;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub pattern: Pattern,
    pub category: String,
    pub priority: i32,
}

impl Rule {
    pub fn new(pattern: &str, category: &str) -> Rule {
        Rule { pattern: Pattern::parse(pattern), category: category.to_string(), priority: 0 }
    }

    pub fn with_priority(mut self, priority: i32) -> Rule {
        self.priority = priority;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct Engine {
    rules: Vec<Rule>,
    pub override_existing: bool,
}

/// Resultado detalhado, útil para `ledger explain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision<'a> {
    pub winner: &'a Rule,
    pub index: usize,
    pub candidates: Vec<usize>,
}

impl Engine {
    pub fn new(rules: Vec<Rule>) -> Engine {
        Engine { rules, override_existing: false }
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn push(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    pub fn extend(&mut self, rules: impl IntoIterator<Item = Rule>) {
        self.rules.extend(rules);
    }

    /// Compara duas regras candidatas: `Greater` significa que `a` vence `b`.
    fn rank(a: (usize, &Rule), b: (usize, &Rule)) -> Ordering {
        a.1.priority
            .cmp(&b.1.priority)
            .then_with(|| a.1.pattern.specificity().cmp(&b.1.pattern.specificity()))
            .then_with(|| b.0.cmp(&a.0))
    }

    pub fn decide(&self, description: &str) -> Option<Decision<'_>> {
        let candidates: Vec<usize> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.pattern.matches(description))
            .map(|(i, _)| i)
            .collect();
        let best = candidates
            .iter()
            .copied()
            .max_by(|&a, &b| Engine::rank((a, &self.rules[a]), (b, &self.rules[b])))?;
        Some(Decision { winner: &self.rules[best], index: best, candidates })
    }

    pub fn categorize(&self, description: &str) -> Option<&str> {
        self.decide(description).map(|d| d.winner.category.as_str())
    }

    /// Aplica a categorização a um lote; devolve quantas transações mudaram.
    pub fn apply(&self, txns: &mut [Txn]) -> usize {
        let mut changed = 0;
        for t in txns.iter_mut() {
            if t.category.is_some() && !self.override_existing {
                continue;
            }
            if let Some(cat) = self.categorize(&t.description) {
                if t.category.as_deref() != Some(cat) {
                    t.category = Some(cat.to_string());
                    changed += 1;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lg_core::{AccountId, Date, Money};

    #[test]
    fn higher_priority_wins() {
        let e = Engine::new(vec![
            Rule::new("POSTO*", "transporte"),
            Rule::new("POSTO SHELL*", "transporte:combustivel").with_priority(-1),
            Rule::new("*SHELL*", "combustivel").with_priority(5),
        ]);
        assert_eq!(e.categorize("POSTO SHELL 123"), Some("combustivel"));
    }

    #[test]
    fn first_defined_wins_full_tie() {
        let e = Engine::new(vec![Rule::new("AB*", "primeira"), Rule::new("A*B", "segunda")]);
        assert_eq!(e.categorize("AB B"), Some("primeira"));
        let d = e.decide("AB B").unwrap();
        assert_eq!(d.index, 0);
        assert_eq!(d.candidates, vec![0, 1]);
    }

    #[test]
    fn no_match_is_none() {
        let e = Engine::new(vec![Rule::new("X*", "x")]);
        assert_eq!(e.categorize("Y"), None);
        assert!(Engine::default().decide("qualquer").is_none());
    }

    #[test]
    fn apply_respects_existing_categories() {
        let acc = AccountId::parse("ativo:banco").unwrap();
        let d = Date::ymd(2024, 1, 1);
        let mut txns = vec![
            Txn::new(d, acc.clone(), "NETFLIX", Money::of(-3990, "BRL")),
            Txn::new(d, acc.clone(), "NETFLIX", Money::of(-3990, "BRL")).with_category("manual"),
        ];
        let mut e = Engine::new(vec![Rule::new("NETFLIX", "assinaturas")]);
        assert_eq!(e.apply(&mut txns), 1);
        assert_eq!(txns[1].category.as_deref(), Some("manual"));
        e.override_existing = true;
        assert_eq!(e.apply(&mut txns), 1);
        assert_eq!(txns[1].category.as_deref(), Some("assinaturas"));
    }
}
