//! Regras padrão (geradas a partir de `docs/regras-padrao.csv`).
//!
//! Valem quando o usuário não define as próprias. Prioridades positivas só onde um
//! padrão genérico (`*SHELL*`) precisa vencer um prefixo (`POSTO*`).

use crate::engine::Rule;

/// (padrão, categoria, prioridade)
pub const BUILTIN: &[(&str, &str, i32)] = &[
    ("PADARIA*", "alimentacao:padaria", 0),
    ("PANIFICADORA*", "alimentacao:padaria", 0),
    ("CONFEITARIA*", "alimentacao:padaria", 0),
    ("*IFOOD*", "alimentacao:delivery", 5),
    ("RAPPI*", "alimentacao:delivery", 0),
    ("ZE DELIVERY*", "alimentacao:delivery", 0),
    ("RESTAURANTE*", "alimentacao:restaurante", 0),
    ("CHURRASCARIA*", "alimentacao:restaurante", 0),
    ("PIZZARIA*", "alimentacao:restaurante", 0),
    ("LANCHONETE*", "alimentacao:restaurante", 0),
    ("BAR E RESTAURANTE*", "alimentacao:restaurante", 1),
    ("CAFETERIA*", "alimentacao:cafe", 0),
    ("STARBUCKS*", "alimentacao:cafe", 0),
    ("MCDONALDS*", "alimentacao:fast-food", 0),
    ("BURGER KING*", "alimentacao:fast-food", 0),
    ("SUBWAY*", "alimentacao:fast-food", 0),
    ("HABIBS*", "alimentacao:fast-food", 0),
    ("OUTBACK*", "alimentacao:restaurante", 0),
    ("MERCADO*", "mercado", 0),
    ("SUPERMERCADO*", "mercado", 0),
    ("HIPERMERCADO*", "mercado", 0),
    ("ATACADAO*", "mercado", 0),
    ("ASSAI*", "mercado", 0),
    ("CARREFOUR*", "mercado", 0),
    ("PAO DE ACUCAR*", "mercado", 0),
    ("EXTRA HIPER*", "mercado", 0),
    ("HORTIFRUTI*", "mercado:hortifruti", 0),
    ("SACOLAO*", "mercado:hortifruti", 0),
    ("ACOUGUE*", "mercado:acougue", 0),
    ("EMPORIO*", "mercado", 0),
    ("MINIMERCADO*", "mercado", 0),
    ("QUITANDA*", "mercado:hortifruti", 0),
    ("MERCADO LIVRE*", "compras:online", 0),
    ("MERCADOLIVRE*", "compras:online", 0),
    ("AMAZON*", "compras:online", 0),
    ("AMAZON PRIME*", "assinaturas:video", 2),
    ("SHOPEE*", "compras:online", 0),
    ("ALIEXPRESS*", "compras:online", 0),
    ("MAGALU*", "compras:online", 0),
    ("MAGAZINE LUIZA*", "compras:loja", 0),
    ("AMERICANAS*", "compras:loja", 0),
    ("CASAS BAHIA*", "compras:loja", 0),
    ("KABUM*", "compras:online", 0),
    ("SHEIN*", "compras:online", 0),
    ("NETSHOES*", "compras:online", 0),
    ("DAFITI*", "compras:online", 0),
    ("TEMU*", "compras:online", 0),
    ("UBER*", "transporte:app", 0),
    ("UBER EATS*", "alimentacao:delivery", 1),
    ("99APP*", "transporte:app", 0),
    ("99 TAXI*", "transporte:app", 0),
    ("CABIFY*", "transporte:app", 0),
    ("POSTO*", "transporte:combustivel", 0),
    ("AUTO POSTO*", "transporte:combustivel", 0),
    ("*SHELL*", "transporte:combustivel", 1),
    ("*IPIRANGA*", "transporte:combustivel", 1),
    ("*PETROBRAS*", "transporte:combustivel", 1),
    ("ESTACIONAMENTO*", "transporte:estacionamento", 0),
    ("ESTAPAR*", "transporte:estacionamento", 0),
    ("SEM PARAR*", "transporte:pedagio", 0),
    ("CONECTCAR*", "transporte:pedagio", 0),
    ("VELOE*", "transporte:pedagio", 0),
    ("METRO*", "transporte:publico", 0),
    ("SPTRANS*", "transporte:publico", 0),
    ("BILHETE UNICO*", "transporte:publico", 0),
    ("LATAM*", "viagem:aereo", 0),
    ("GOL LINHAS*", "viagem:aereo", 0),
    ("AZUL LINHAS*", "viagem:aereo", 0),
    ("BOOKING*", "viagem:hospedagem", 0),
    ("AIRBNB*", "viagem:hospedagem", 0),
    ("HOTEL*", "viagem:hospedagem", 0),
    ("NETFLIX*", "assinaturas:video", 0),
    ("SPOTIFY*", "assinaturas:musica", 0),
    ("DEEZER*", "assinaturas:musica", 0),
    ("DISNEY PLUS*", "assinaturas:video", 0),
    ("DISNEY+*", "assinaturas:video", 0),
    ("HBO MAX*", "assinaturas:video", 0),
    ("MAX.COM*", "assinaturas:video", 0),
    ("GLOBOPLAY*", "assinaturas:video", 0),
    ("YOUTUBE PREMIUM*", "assinaturas:video", 0),
    ("APPLE.COM/BILL*", "assinaturas:apps", 0),
    ("GOOGLE *", "assinaturas:apps", 0),
    ("MICROSOFT*", "assinaturas:software", 0),
    ("ADOBE*", "assinaturas:software", 0),
    ("DROPBOX*", "assinaturas:software", 0),
    ("GITHUB*", "assinaturas:software", 0),
    ("DROGARIA*", "saude:farmacia", 0),
    ("FARMACIA*", "saude:farmacia", 0),
    ("*DROGASIL*", "saude:farmacia", 1),
    ("*RAIA*", "saude:farmacia", 1),
    ("PAGUE MENOS*", "saude:farmacia", 0),
    ("LABORATORIO*", "saude:exames", 0),
    ("CLINICA*", "saude:consultas", 0),
    ("HOSPITAL*", "saude:hospital", 0),
    ("UNIMED*", "saude:plano", 0),
    ("AMIL*", "saude:plano", 0),
    ("SULAMERICA SAUDE*", "saude:plano", 0),
    ("ODONTO*", "saude:dentista", 0),
    ("SMART FIT*", "saude:academia", 0),
    ("BLUEFIT*", "saude:academia", 0),
    ("GYMPASS*", "saude:academia", 0),
    ("ENEL*", "casa:energia", 0),
    ("CPFL*", "casa:energia", 0),
    ("LIGHT SA*", "casa:energia", 0),
    ("CEMIG*", "casa:energia", 0),
    ("SABESP*", "casa:agua", 0),
    ("COPASA*", "casa:agua", 0),
    ("COMGAS*", "casa:gas", 0),
    ("ULTRAGAZ*", "casa:gas", 0),
    ("VIVO*", "casa:telefonia", 0),
    ("CLARO*", "casa:telefonia", 0),
    ("TIM *", "casa:telefonia", 0),
    ("OI FIBRA*", "casa:internet", 0),
    ("CONDOMINIO*", "casa:condominio", 0),
    ("ALUGUEL*", "casa:aluguel", 0),
    ("IPTU*", "impostos:iptu", 0),
    ("LEROY MERLIN*", "casa:manutencao", 0),
    ("TELHANORTE*", "casa:manutencao", 0),
    ("C&C*", "casa:manutencao", 0),
    ("TOK STOK*", "casa:moveis", 0),
    ("ETNA*", "casa:moveis", 0),
    ("CAMICADO*", "casa:utilidades", 0),
    ("ESCOLA*", "educacao:escola", 0),
    ("COLEGIO*", "educacao:escola", 0),
    ("FACULDADE*", "educacao:faculdade", 0),
    ("UNIVERSIDADE*", "educacao:faculdade", 0),
    ("UDEMY*", "educacao:cursos", 0),
    ("ALURA*", "educacao:cursos", 0),
    ("COURSERA*", "educacao:cursos", 0),
    ("LIVRARIA*", "educacao:livros", 0),
    ("SARAIVA*", "educacao:livros", 0),
    ("PIX RECEBIDO*", "receita:pix", 0),
    ("PIX ENVIADO*", "transferencias:pix", 0),
    ("TED RECEBIDA*", "receita:transferencia", 0),
    ("TED ENVIADA*", "transferencias:ted", 0),
    ("DOC ENVIADO*", "transferencias:doc", 0),
    ("SALARIO*", "receita:salario", 0),
    ("PAGTO SALARIO*", "receita:salario", 0),
    ("RENDIMENTO*", "receita:rendimentos", 0),
    ("RESGATE*", "investimentos:resgate", 0),
    ("APLICACAO*", "investimentos:aplicacao", 0),
    ("TARIFA*", "financeiro:tarifas", 0),
    ("IOF*", "impostos:iof", 0),
    ("JUROS*", "financeiro:juros", 0),
    ("ANUIDADE*", "financeiro:tarifas", 0),
    ("PAGTO FATURA*", "transferencias:cartao", 0),
    ("ESTORNO*", "estornos", 3),
    ("CASHBACK*", "receita:cashback", 0),
    ("DARF*", "impostos:federal", 0),
    ("GPS INSS*", "impostos:inss", 0),
    ("IPVA*", "impostos:ipva", 0),
    ("DETRAN*", "transporte:taxas", 0),
    ("CINEMA*", "lazer:cinema", 0),
    ("CINEMARK*", "lazer:cinema", 0),
    ("INGRESSO.COM*", "lazer:eventos", 0),
    ("SYMPLA*", "lazer:eventos", 0),
    ("STEAM*", "lazer:jogos", 0),
    ("PLAYSTATION*", "lazer:jogos", 0),
    ("NINTENDO*", "lazer:jogos", 0),
    ("XBOX*", "lazer:jogos", 0),
    ("PET SHOP*", "pets", 0),
    ("PETZ*", "pets", 0),
    ("COBASI*", "pets", 0),
    ("VETERINARIA*", "pets:veterinario", 0),
];

pub fn builtin_rules() -> Vec<Rule> {
    BUILTIN.iter().map(|&(p, c, pr)| Rule::new(p, c).with_priority(pr)).collect()
}

/// Categorias distintas usadas pelas regras padrão, em ordem alfabética.
pub fn builtin_categories() -> Vec<&'static str> {
    let mut cats: Vec<&str> = BUILTIN.iter().map(|&(_, c, _)| c).collect();
    cats.sort_unstable();
    cats.dedup();
    cats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;

    #[test]
    fn patterns_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for (p, _, _) in BUILTIN {
            assert!(seen.insert(*p), "padrão repetido: {p}");
        }
    }

    #[test]
    fn categories_are_lowercase_without_spaces() {
        for c in builtin_categories() {
            assert!(!c.contains(' ') && c.to_lowercase() == c, "{c}");
        }
    }

    #[test]
    fn common_descriptions() {
        let e = Engine::new(builtin_rules());
        assert_eq!(e.categorize("PADARIA BOM PAO LTDA"), Some("alimentacao:padaria"));
        assert_eq!(e.categorize("POSTO SHELL AV BRASIL"), Some("transporte:combustivel"));
        assert_eq!(e.categorize("UBER EATS PEDIDO 42"), Some("alimentacao:delivery"));
        assert_eq!(e.categorize("NETFLIX.COM"), Some("assinaturas:video"));
        assert_eq!(e.categorize("SUPERMERCADO DIA 123"), Some("mercado"));
        assert_eq!(e.categorize("ESTORNO COMPRA UBER"), Some("estornos"));
        assert_eq!(e.categorize("TRANSFERENCIA SEM REGRA"), None);
    }
}
