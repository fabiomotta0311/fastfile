use ws_text::header::{parse_header, MAX_LINE};

#[test]
fn le_cabecalho_simples() {
    assert_eq!(
        parse_header("Host:example"),
        Some(("host".to_string(), "example".to_string()))
    );
}

#[test]
fn rejeita_linha_acima_do_limite() {
    let line = format!("k:{}", "x".repeat(MAX_LINE));
    assert_eq!(parse_header(&line), None);
}
