//! Divisão de registros CSV (RFC 4180 simplificado).
//!
//! Campos podem vir entre aspas duplas; dentro delas o separador é literal e `""`
//! representa uma aspa. Quebras de linha dentro de aspas não são suportadas: os
//! extratos dos bancos atendidos não as usam.

use lg_core::{Error, Result};

/// Separadores aceitos na detecção automática, em ordem de preferência.
pub const SEPARATORS: [char; 3] = [';', ',', '\t'];

/// Escolhe o separador que mais aparece fora de aspas no cabeçalho.
pub fn detect_separator(header: &str) -> char {
    let mut best = (0usize, SEPARATORS[0]);
    for sep in SEPARATORS {
        let mut in_quotes = false;
        let mut count = 0;
        for c in header.chars() {
            match c {
                '"' => in_quotes = !in_quotes,
                c if c == sep && !in_quotes => count += 1,
                _ => {}
            }
        }
        if count > best.0 {
            best = (count, sep);
        }
    }
    best.1
}

/// Divide uma linha em campos. `line_no` só entra na mensagem de erro.
pub fn split_record(line: &str, sep: char, line_no: usize) -> Result<Vec<String>> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut field_started_quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
            continue;
        }
        match c {
            '"' if cur.is_empty() && !field_started_quoted => {
                in_quotes = true;
                field_started_quoted = true;
            }
            '"' => {
                return Err(Error::Parse { line: line_no, message: "aspas no meio de um campo".into() });
            }
            c if c == sep => {
                fields.push(std::mem::take(&mut cur));
                field_started_quoted = false;
            }
            c => {
                if field_started_quoted {
                    return Err(Error::Parse { line: line_no, message: "texto depois das aspas de fechamento".into() });
                }
                cur.push(c);
            }
        }
    }
    if in_quotes {
        return Err(Error::Parse { line: line_no, message: "aspas sem fechamento".into() });
    }
    fields.push(cur);
    Ok(fields)
}

/// Escreve um campo com aspas quando necessário (inverso de [`split_record`]).
pub fn quote_field(field: &str, sep: char) -> String {
    if field.contains(sep) || field.contains('"') || field.starts_with(' ') || field.ends_with(' ') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_fields() {
        assert_eq!(split_record("a;b;c", ';', 1).unwrap(), vec!["a", "b", "c"]);
        assert_eq!(split_record("a;;c;", ';', 1).unwrap(), vec!["a", "", "c", ""]);
    }

    #[test]
    fn quoted_separator_is_literal() {
        assert_eq!(split_record("\"a;b\";c", ';', 1).unwrap(), vec!["a;b", "c"]);
        assert_eq!(split_record("x,\"1,50\"", ',', 1).unwrap(), vec!["x", "1,50"]);
    }

    #[test]
    fn malformed_quotes_are_errors() {
        assert!(split_record("\"abc", ';', 3).is_err());
        assert!(split_record("ab\"c;d", ';', 3).is_err());
        assert!(split_record("\"ab\"c;d", ';', 3).is_err());
    }

    #[test]
    fn detects_separator() {
        assert_eq!(detect_separator("data;descricao;valor"), ';');
        assert_eq!(detect_separator("data,descricao,valor"), ',');
        assert_eq!(detect_separator("data\tdescricao\tvalor"), '\t');
        assert_eq!(detect_separator("\"a;b\",c,d"), ',');
    }

    #[test]
    fn quote_field_only_when_needed() {
        assert_eq!(quote_field("abc", ';'), "abc");
        assert_eq!(quote_field("a;b", ';'), "\"a;b\"");
        assert_eq!(quote_field(" x", ';'), "\" x\"");
    }
}
