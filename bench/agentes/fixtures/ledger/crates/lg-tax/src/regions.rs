//! Tabela de regiões fiscais (gerada a partir de `docs/regioes.csv`).
//!
//! Valores sintéticos para testes do razão; não usar como referência fiscal.

use lg_core::RoundingMode;

use crate::rate::Rate;

#[derive(Debug, PartialEq, Eq)]
pub struct Region {
    pub code: &'static str,
    pub name: &'static str,
    goods_ppm: i64,
    service_ppm: i64,
    pub rounding: RoundingMode,
}

impl Region {
    pub fn goods_rate(&self) -> Rate {
        Rate::from_ppm(self.goods_ppm)
    }

    pub fn service_rate(&self) -> Rate {
        Rate::from_ppm(self.service_ppm)
    }
}

pub static REGIONS: &[Region] = &[
    Region {
        code: "BR-AC",
        name: "Acre",
        goods_ppm: 190000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-AL",
        name: "Alagoas",
        goods_ppm: 190000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-AM",
        name: "Amazonas",
        goods_ppm: 200000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-AP",
        name: "Amapá",
        goods_ppm: 180000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-BA",
        name: "Bahia",
        goods_ppm: 205000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-CE",
        name: "Ceará",
        goods_ppm: 200000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-DF",
        name: "Distrito Federal",
        goods_ppm: 200000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-ES",
        name: "Espírito Santo",
        goods_ppm: 170000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-GO",
        name: "Goiás",
        goods_ppm: 190000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-MA",
        name: "Maranhão",
        goods_ppm: 220000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-MG",
        name: "Minas Gerais",
        goods_ppm: 180000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-MS",
        name: "Mato Grosso do Sul",
        goods_ppm: 170000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-MT",
        name: "Mato Grosso",
        goods_ppm: 170000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-PA",
        name: "Pará",
        goods_ppm: 190000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-PB",
        name: "Paraíba",
        goods_ppm: 200000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-PE",
        name: "Pernambuco",
        goods_ppm: 205000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-PI",
        name: "Piauí",
        goods_ppm: 210000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-PR",
        name: "Paraná",
        goods_ppm: 195000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-RJ",
        name: "Rio de Janeiro",
        goods_ppm: 220000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-RN",
        name: "Rio Grande do Norte",
        goods_ppm: 200000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-RO",
        name: "Rondônia",
        goods_ppm: 195000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-RR",
        name: "Roraima",
        goods_ppm: 200000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-RS",
        name: "Rio Grande do Sul",
        goods_ppm: 170000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-SC",
        name: "Santa Catarina",
        goods_ppm: 170000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-SE",
        name: "Sergipe",
        goods_ppm: 190000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-SP",
        name: "São Paulo",
        goods_ppm: 180000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "BR-TO",
        name: "Tocantins",
        goods_ppm: 200000,
        service_ppm: 50000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-AT",
        name: "Áustria",
        goods_ppm: 200000,
        service_ppm: 200000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-BE",
        name: "Bélgica",
        goods_ppm: 210000,
        service_ppm: 210000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-BG",
        name: "Bulgária",
        goods_ppm: 200000,
        service_ppm: 200000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-CY",
        name: "Chipre",
        goods_ppm: 190000,
        service_ppm: 190000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-CZ",
        name: "Tchéquia",
        goods_ppm: 210000,
        service_ppm: 210000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-DE",
        name: "Alemanha",
        goods_ppm: 190000,
        service_ppm: 190000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-DK",
        name: "Dinamarca",
        goods_ppm: 250000,
        service_ppm: 250000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-EE",
        name: "Estônia",
        goods_ppm: 220000,
        service_ppm: 220000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-ES",
        name: "Espanha",
        goods_ppm: 210000,
        service_ppm: 210000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-FI",
        name: "Finlândia",
        goods_ppm: 255000,
        service_ppm: 255000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-FR",
        name: "França",
        goods_ppm: 200000,
        service_ppm: 200000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-GR",
        name: "Grécia",
        goods_ppm: 240000,
        service_ppm: 240000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-HR",
        name: "Croácia",
        goods_ppm: 250000,
        service_ppm: 250000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-HU",
        name: "Hungria",
        goods_ppm: 270000,
        service_ppm: 270000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-IE",
        name: "Irlanda",
        goods_ppm: 230000,
        service_ppm: 230000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-IT",
        name: "Itália",
        goods_ppm: 220000,
        service_ppm: 220000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-LT",
        name: "Lituânia",
        goods_ppm: 210000,
        service_ppm: 210000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-LU",
        name: "Luxemburgo",
        goods_ppm: 170000,
        service_ppm: 170000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-LV",
        name: "Letônia",
        goods_ppm: 210000,
        service_ppm: 210000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-MT",
        name: "Malta",
        goods_ppm: 180000,
        service_ppm: 180000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-NL",
        name: "Países Baixos",
        goods_ppm: 210000,
        service_ppm: 210000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-PL",
        name: "Polônia",
        goods_ppm: 230000,
        service_ppm: 230000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-PT",
        name: "Portugal",
        goods_ppm: 230000,
        service_ppm: 230000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-RO",
        name: "Romênia",
        goods_ppm: 190000,
        service_ppm: 190000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-SE",
        name: "Suécia",
        goods_ppm: 250000,
        service_ppm: 250000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-SI",
        name: "Eslovênia",
        goods_ppm: 220000,
        service_ppm: 220000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "EU-SK",
        name: "Eslováquia",
        goods_ppm: 230000,
        service_ppm: 230000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "US-CA",
        name: "Califórnia",
        goods_ppm: 72500,
        service_ppm: 0,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "US-NY",
        name: "Nova York",
        goods_ppm: 40000,
        service_ppm: 0,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "US-TX",
        name: "Texas",
        goods_ppm: 62500,
        service_ppm: 0,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "US-WA",
        name: "Washington",
        goods_ppm: 65000,
        service_ppm: 0,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "US-FL",
        name: "Flórida",
        goods_ppm: 60000,
        service_ppm: 0,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "US-OR",
        name: "Oregon",
        goods_ppm: 0,
        service_ppm: 0,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "CA-ON",
        name: "Ontário",
        goods_ppm: 130000,
        service_ppm: 130000,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "CA-QC",
        name: "Quebec",
        goods_ppm: 149750,
        service_ppm: 149750,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "CA-BC",
        name: "Colúmbia Britânica",
        goods_ppm: 120000,
        service_ppm: 120000,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "JP",
        name: "Japão",
        goods_ppm: 100000,
        service_ppm: 100000,
        rounding: RoundingMode::Down,
    },
    Region {
        code: "CH",
        name: "Suíça",
        goods_ppm: 81000,
        service_ppm: 81000,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "GB",
        name: "Reino Unido",
        goods_ppm: 200000,
        service_ppm: 200000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "NO",
        name: "Noruega",
        goods_ppm: 250000,
        service_ppm: 250000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "AR",
        name: "Argentina",
        goods_ppm: 210000,
        service_ppm: 210000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "CL",
        name: "Chile",
        goods_ppm: 190000,
        service_ppm: 190000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "UY",
        name: "Uruguai",
        goods_ppm: 220000,
        service_ppm: 220000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "MX",
        name: "México",
        goods_ppm: 160000,
        service_ppm: 160000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "CO",
        name: "Colômbia",
        goods_ppm: 190000,
        service_ppm: 190000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "PE",
        name: "Peru",
        goods_ppm: 180000,
        service_ppm: 180000,
        rounding: RoundingMode::HalfEven,
    },
    Region {
        code: "AU",
        name: "Austrália",
        goods_ppm: 100000,
        service_ppm: 100000,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "NZ",
        name: "Nova Zelândia",
        goods_ppm: 150000,
        service_ppm: 150000,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "SG",
        name: "Singapura",
        goods_ppm: 90000,
        service_ppm: 90000,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "IN",
        name: "Índia",
        goods_ppm: 180000,
        service_ppm: 180000,
        rounding: RoundingMode::HalfUp,
    },
    Region {
        code: "ZA",
        name: "África do Sul",
        goods_ppm: 150000,
        service_ppm: 150000,
        rounding: RoundingMode::HalfUp,
    },
];

/// Procura por código (`BR-SP`, `EU-DE`, `US-CA`), sem diferenciar maiúsculas.
pub fn region(code: &str) -> Option<&'static Region> {
    REGIONS.iter().find(|r| r.code.eq_ignore_ascii_case(code.trim()))
}

/// Regiões cujo código começa com o prefixo dado (`BR-`, `EU-`).
pub fn regions_with_prefix(prefix: &str) -> Vec<&'static Region> {
    REGIONS.iter().filter(|r| r.code.starts_with(prefix)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup() {
        assert_eq!(region("br-sp").unwrap().name, "São Paulo");
        assert!(region("BR-XX").is_none());
        assert_eq!(regions_with_prefix("BR-").len(), 27);
    }

    #[test]
    fn rates_are_sane() {
        for r in REGIONS {
            assert!(r.goods_ppm >= 0 && r.goods_ppm < 1_000_000, "{}", r.code);
            assert!(r.service_ppm >= 0 && r.service_ppm < 1_000_000, "{}", r.code);
        }
    }
}
