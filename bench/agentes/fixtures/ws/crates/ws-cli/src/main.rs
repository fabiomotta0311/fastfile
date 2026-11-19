// parse_header é definido em ws-text; aqui só é chamado.
fn main() {
    let linhas: Vec<String> = std::env::args().skip(1).collect();
    let refs: Vec<&str> = linhas.iter().map(String::as_str).collect();
    println!("{}", ws_cli::report::summarize(&refs));
    let _ = ws_text::header::parse_header("a: b");
    println!("use parse_header para cabeçalhos");
}
