//! Tabelas de texto para o terminal.
//!
//! As larguras são medidas em caracteres (não em bytes), porque descrições de extratos
//! trazem acentos e símbolos de moeda fora do ASCII. Caracteres de largura dupla (CJK)
//! não são tratados.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub struct Table {
    headers: Vec<String>,
    aligns: Vec<Align>,
    rows: Vec<Vec<String>>,
    /// Largura máxima de uma coluna; textos maiores são cortados com "…".
    pub max_width: Option<usize>,
    pub separator: &'static str,
}

fn display_width(s: &str) -> usize {
    s.chars().count()
}

fn truncate(s: &str, max: usize) -> String {
    if display_width(s) <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn pad(s: &str, width: usize, align: Align) -> String {
    let fill = " ".repeat(width.saturating_sub(display_width(s)));
    match align {
        Align::Left => format!("{s}{fill}"),
        Align::Right => format!("{fill}{s}"),
    }
}

impl Table {
    pub fn new(headers: &[&str]) -> Table {
        Table {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            aligns: vec![Align::Left; headers.len()],
            rows: Vec::new(),
            max_width: None,
            separator: "  ",
        }
    }

    pub fn align(mut self, column: usize, align: Align) -> Table {
        self.aligns[column] = align;
        self
    }

    pub fn push_row<S: AsRef<str>>(&mut self, cells: &[S]) {
        assert_eq!(cells.len(), self.headers.len(), "linha com número de colunas diferente do cabeçalho");
        self.rows.push(cells.iter().map(|c| c.as_ref().to_string()).collect());
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    fn cell(&self, s: &str) -> String {
        match self.max_width {
            Some(m) => truncate(s, m),
            None => s.to_string(),
        }
    }

    fn widths(&self) -> Vec<usize> {
        let mut w: Vec<usize> = self.headers.iter().map(|h| display_width(&self.cell(h))).collect();
        for row in &self.rows {
            for (i, c) in row.iter().enumerate() {
                w[i] = w[i].max(display_width(&self.cell(c)));
            }
        }
        w
    }

    /// Renderiza com uma linha de traços abaixo do cabeçalho. Sem espaços no fim das linhas.
    pub fn render(&self) -> String {
        let widths = self.widths();
        let mut out = String::new();
        let line = |cells: &[String], out: &mut String| {
            let parts: Vec<String> = cells
                .iter()
                .enumerate()
                .map(|(i, c)| pad(&self.cell(c), widths[i], self.aligns[i]))
                .collect();
            out.push_str(parts.join(self.separator).trim_end());
            out.push('\n');
        };
        line(&self.headers, &mut out);
        let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        out.push_str(&rule.join(self.separator));
        out.push('\n');
        for row in &self.rows {
            line(row, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_aligned_columns() {
        let mut t = Table::new(&["conta", "valor"]).align(1, Align::Right);
        t.push_row(&["ativo:banco", "10.00"]);
        t.push_row(&["despesa", "1,234.50"]);
        assert_eq!(
            t.render(),
            "conta           valor\n-----------  --------\nativo:banco     10.00\ndespesa      1,234.50\n"
        );
    }

    #[test]
    fn truncates_long_cells() {
        let mut t = Table::new(&["descricao"]);
        t.max_width = Some(6);
        t.push_row(&["SUPERMERCADO"]);
        assert_eq!(t.render(), "descr…\n------\nSUPER…\n");
    }

    #[test]
    fn wrong_column_count_is_rejected() {
        let r = std::panic::catch_unwind(|| {
            let mut t = Table::new(&["a", "b"]);
            t.push_row(&["só uma"]);
        });
        assert!(r.is_err());
    }

    #[test]
    fn empty_table_has_header_only() {
        let t = Table::new(&["a"]);
        assert!(t.is_empty());
        assert_eq!(t.render(), "a\n-\n");
    }
}
