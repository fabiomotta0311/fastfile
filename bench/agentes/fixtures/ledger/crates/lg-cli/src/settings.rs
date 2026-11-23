//! Configuração efetiva.
//!
//! Precedência, da mais fraca para a mais forte:
//! 1. padrões embutidos ([`Settings::default`]);
//! 2. arquivo `ledger.conf` (seções `[report]`, `[import]`, `[rules]`);
//! 3. opções da linha de comando.
//!
//! Uma opção dada na linha de comando sempre vence o arquivo.

use lg_core::{currency, AccountId, Currency, Error, Result, RoundingMode};
use lg_fmt::Locale;
use lg_parse::Config;
use lg_report::FiscalCalendar;

use crate::args::Flags;

#[derive(Debug, Clone)]
pub struct Settings {
    pub locale: Locale,
    pub currency: &'static Currency,
    pub calendar: FiscalCalendar,
    pub rounding: RoundingMode,
    pub account: AccountId,
    pub negate_import: bool,
    pub rules_file: Option<String>,
    pub use_builtin_rules: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            locale: Locale::by_tag("pt-BR").expect("pt-BR na tabela"),
            currency: currency::brl(),
            calendar: FiscalCalendar::default(),
            rounding: RoundingMode::HalfEven,
            account: AccountId::parse("ativo:banco").expect("conta padrão válida"),
            negate_import: false,
            rules_file: None,
            use_builtin_rules: true,
        }
    }
}

/// Primeiro valor presente na ordem dada.
fn pick(candidates: [Option<String>; 2]) -> Option<String> {
    let [a, b] = candidates;
    a.or(b)
}

impl Settings {
    pub fn resolve(file: &Config, flags: &Flags) -> Result<Settings> {
        let mut s = Settings::default();
        let from_file = |k: &str| file.get(k).map(str::to_string);

        if let Some(tag) = pick([flags.locale.clone(), from_file("report.locale")]) {
            s.locale = Locale::by_tag(&tag).ok_or_else(|| Error::Config(format!("localidade desconhecida: {tag}")))?;
        }
        if let Some(code) = pick([flags.currency.clone(), from_file("report.currency")]) {
            s.currency = currency::require(&code)?;
        }
        let fiscal = match flags.fiscal_start {
            Some(n) => Some(n),
            None => file.get_u8("report.fiscal_start")?,
        };
        if let Some(n) = fiscal {
            s.calendar = FiscalCalendar::new(n).ok_or_else(|| Error::Config(format!("fiscal_start fora de 1..12: {n}")))?;
        }
        if let Some(m) = pick([flags.rounding.clone(), from_file("report.rounding")]) {
            s.rounding = RoundingMode::parse(&m).ok_or_else(|| Error::Config(format!("arredondamento desconhecido: {m}")))?;
        }
        if let Some(a) = pick([flags.account.clone(), from_file("import.account")]) {
            s.account = AccountId::parse(&a)?;
        }
        if let Some(b) = file.get_bool("import.negate")? {
            s.negate_import = b;
        }
        s.rules_file = pick([flags.rules_file.clone(), from_file("rules.file")]);
        if let Some(b) = file.get_bool("rules.builtin")? {
            s.use_builtin_rules = b;
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_without_file_or_flags() {
        let s = Settings::resolve(&Config::default(), &Flags::default()).unwrap();
        assert_eq!(s.locale.tag, "pt-BR");
        assert_eq!(s.currency.code, "BRL");
        assert_eq!(s.calendar.start_month(), 1);
        assert!(s.use_builtin_rules);
    }

    #[test]
    fn file_overrides_defaults() {
        let file = Config::parse("[report]\ncurrency = USD\nfiscal_start = 7\nrounding = half-up\n[import]\naccount = passivo:cartao\nnegate = sim\n").unwrap();
        let s = Settings::resolve(&file, &Flags::default()).unwrap();
        assert_eq!(s.currency.code, "USD");
        assert_eq!(s.calendar.start_month(), 7);
        assert_eq!(s.rounding, RoundingMode::HalfUp);
        assert_eq!(s.account.as_str(), "passivo:cartao");
        assert!(s.negate_import);
    }

    #[test]
    fn flags_override_file() {
        let file = Config::parse("[report]\ncurrency = USD\nfiscal_start = 7\n").unwrap();
        let flags = Flags { currency: Some("EUR".into()), fiscal_start: Some(4), ..Flags::default() };
        let s = Settings::resolve(&file, &flags).unwrap();
        assert_eq!(s.currency.code, "EUR");
        assert_eq!(s.calendar.start_month(), 4);
    }

    #[test]
    fn invalid_values_are_errors() {
        let bad = |text: &str| Settings::resolve(&Config::parse(text).unwrap(), &Flags::default()).is_err();
        assert!(bad("[report]\nlocale = xx-YY\n"));
        assert!(bad("[report]\ncurrency = ZZZ\n"));
        assert!(bad("[report]\nfiscal_start = 13\n"));
        assert!(bad("[report]\nrounding = magico\n"));
    }
}
