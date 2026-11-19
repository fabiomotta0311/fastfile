/// Tamanho máximo aceito para uma linha de cabeçalho, em bytes.
pub const MAX_LINE: usize = 1024;

/// Lê "Chave: valor". A chave é normalizada em minúsculas; o valor perde espaços das pontas.
pub fn parse_header(line: &str) -> Option<(String, String)> {
    if line.len() > MAX_LINE {
        return None;
    }
    let (k, v) = line.split_once(':')?;
    let k = k.trim();
    if k.is_empty() {
        return None;
    }
    Some((k.to_ascii_lowercase(), v.trim().to_string()))
}
