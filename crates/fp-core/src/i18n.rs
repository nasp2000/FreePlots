//! Language catalogue for the Session report (D10).
//!
//! Every user-visible label produced by the engine lives here, so adding a
//! language is a catalogue change and not a change scattered through the
//! analysis code.

/// Supported interface languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// English (default).
    #[default]
    English,
    /// Portuguese (pt-PT).
    Portuguese,
}

impl Lang {
    /// Stable code used in the CLI and in project files.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Portuguese => "pt",
        }
    }

    /// Parses a language code (`en`, `pt`, `pt-PT`).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().split('-').next().unwrap_or("") {
            "en" => Some(Self::English),
            "pt" => Some(Self::Portuguese),
            _ => None,
        }
    }
}

/// Labels used by the Session report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Catalog {
    /// Language of the labels.
    pub lang: Lang,
}

impl Catalog {
    /// Catalogue for one language.
    #[must_use]
    pub fn new(lang: Lang) -> Self {
        Self { lang }
    }

    /// English catalogue.
    #[must_use]
    pub fn english() -> Self {
        Self::new(Lang::English)
    }

    /// "Estimation method" / "Método de estimação".
    #[must_use]
    pub fn estimation_method(self) -> &'static str {
        match self.lang {
            Lang::English => "Estimation method",
            Lang::Portuguese => "Método de estimação",
        }
    }

    /// "Observations (n)".
    #[must_use]
    pub fn observations(self) -> &'static str {
        match self.lang {
            Lang::English => "Observations (n)",
            Lang::Portuguese => "Observações (n)",
        }
    }

    /// "Excluded (non-finite)".
    #[must_use]
    pub fn excluded(self) -> &'static str {
        match self.lang {
            Lang::English => "Excluded (non-finite)",
            Lang::Portuguese => "Excluídos (não finitos)",
        }
    }

    /// "Estimated parameters".
    #[must_use]
    pub fn estimated_parameters(self) -> &'static str {
        match self.lang {
            Lang::English => "Estimated parameters",
            Lang::Portuguese => "Parâmetros estimados",
        }
    }

    /// "Statistics".
    #[must_use]
    pub fn statistics(self) -> &'static str {
        match self.lang {
            Lang::English => "Statistics",
            Lang::Portuguese => "Estatísticas",
        }
    }

    /// "Goodness of fit".
    #[must_use]
    pub fn goodness_of_fit(self) -> &'static str {
        match self.lang {
            Lang::English => "Goodness of fit",
            Lang::Portuguese => "Aderência ao modelo",
        }
    }

    /// "Method notes".
    #[must_use]
    pub fn method_notes(self) -> &'static str {
        match self.lang {
            Lang::English => "Method notes",
            Lang::Portuguese => "Notas de método",
        }
    }

    /// "WARNINGS".
    #[must_use]
    pub fn warnings(self) -> &'static str {
        match self.lang {
            Lang::English => "WARNINGS",
            Lang::Portuguese => "AVISOS",
        }
    }

    /// "End of report.".
    #[must_use]
    pub fn end_of_report(self) -> &'static str {
        match self.lang {
            Lang::English => "End of report.",
            Lang::Portuguese => "Fim do relatório.",
        }
    }
}

impl Catalog {
    /// Translates a label of the report.
    ///
    /// The labels of the statistics blocks and of the tables are written in
    /// English by the analysis modules; this method maps the ones the engine
    /// uses to the language of the catalogue and leaves the others untouched.
    #[must_use]
    pub fn label(self, text: &str) -> &str {
        if self.lang == Lang::English {
            return text;
        }
        match text {
            "Term" => "Termo",
            "Coef" => "Coef.",
            "SE Effect" => "EP efeito",
            "Effect" => "Efeito",
            "T" => "t",
            "P" => "p",
            "Source" => "Fonte",
            "DF" => "GL",
            "SS" => "SQ",
            "MS" => "QM",
            "F" => "F",
            "Group" => "Grupo",
            "Level" => "Nível",
            "Count" => "N",
            "Mean" => "Média",
            "StDev" => "Desvio padrão",
            "Variance" => "Variância",
            "Median" => "Mediana",
            "Minimum" => "Mínimo",
            "Maximum" => "Máximo",
            "Range" => "Amplitude",
            "Sum" => "Soma",
            "Column" => "Coluna",
            "Value" => "Valor",
            "Values" => "Valores",
            "Test" => "Teste",
            "Point" => "Ponto",
            "Signal" => "Sinal",
            "Chart" => "Gráfico",
            "Center" => "Centro",
            "Center line" => "Linha central",
            "LCL" => "LIC",
            "UCL" => "LSC",
            "Points" => "Pontos",
            "Parameter" => "Parâmetro",
            "Estimate" => "Estimativa",
            "Lower" => "Inferior",
            "Upper" => "Superior",
            "Target" => "Alvo",
            "Difference" => "Diferença",
            "Sample" => "Amostra",
            "Samples" => "Amostras",
            "Method" => "Método",
            "Analysis" => "Análise",
            "Statistic" => "Estatística",
            "Probability" => "Probabilidade",
            "Confidence" => "Confiança",
            "Censored" => "Censurados",
            "Events" => "Eventos",
            "Survival" => "Sobrevivência",
            "Type" => "Tipo",
            "Missing" => "Em falta",
            "Distinct" => "Distintos",
            "Weight" => "Peso",
            "Part" => "Peça",
            "Operator" => "Operador",
            "Response" => "Resposta",
            "Factor" => "Fator",
            "Factors" => "Fatores",
            "Block" => "Bloco",
            "Blocks" => "Blocos",
            "Factorial model" => "Modelo fatorial",
            "Error" => "Erro",
            "Total" => "Total",
            "Model" => "Modelo",
            "DF Model" => "GL modelo",
            "Adjusted R²" => "R² ajustado",
            "Residual standard error" => "Desvio padrão residual",
            "R²" => "R²",
            "First quartile (Q1)" => "Primeiro quartil (Q1)",
            "Third quartile (Q3)" => "Terceiro quartil (Q3)",
            "Interquartile range" => "Amplitude interquartis",
            "Standard deviation (n-1)" => "Desvio padrão (n-1)",
            "Standard deviation (n−1)" => "Desvio padrão (n−1)",
            "Standard error of the mean" => "Erro padrão da média",
            "95% CI for the mean (lower)" => "IC 95% da média (inferior)",
            "95% CI for the mean (upper)" => "IC 95% da média (superior)",
            "Data policy" => "Política de dados",
            "Statement" => "Afirmação",
            "Assumption" => "Pressuposto",
            "Quantity" => "Quantidade",
            "Lower limit" => "Limite inferior",
            "Upper limit" => "Limite superior",
            "Tolerance factor" => "Factor de tolerância",
            "Tolerance factor k" => "Factor de tolerância k",
            "Tukey HSD" => "Tukey HSD",
            "P (LSD)" => "p (LSD)",
            "P (Games-Howell)" => "p (Games-Howell)",
            "Reference" => "Referência",
            "Observation" => "Observação",
            "Subgroup" => "Subgrupo",
            "Rank" => "Posição",
            "Shape (Weibull)" => "Forma (Weibull)",
            "Shape SE" => "EP da forma",
            "Intercept" => "Interseto",
            "Intercept SE" => "EP do interseto",
            "Arrhenius slope" => "Declive de Arrhenius",
            "Slope SE" => "EP do declive",
            "Use stress" => "Tensão de utilização",
            "Life at use" => "Vida na utilização",
            "B10 at use" => "B10 na utilização",
            "Acceleration factor" => "Factor de aceleração",
            "Characteristic life" => "Vida característica",
            "B10 life" => "Vida B10",
            "Stress" => "Tensão",
            "New column" => "Nova coluna",
            "Expression" => "Expressão",
            "Above" => "Acima",
            "Below" => "Abaixo",
            "Statistic (smaller tail)" => "Estatística (cauda menor)",
            "Hypothesised median" => "Mediana hipotética",
            "Percent" => "Percentagem",
            "Leaf unit" => "Unidade da folha",
            "Stems" => "Caules",
            "Leaves" => "Folhas",
            "Band half-width" => "Semi-largura da banda",
            "Largest jump" => "Maior salto",
            "First quartile" => "Primeiro quartil",
            "Third quartile" => "Terceiro quartil",
            "Cumulative probability" => "Probabilidade acumulada",
            "F(x)" => "F(x)",
            "Pair" => "Par",
            "Pearson r" => "r de Pearson",
            "Pooled standard deviation" => "Desvio padrão agrupado",
            "Cumulative %" => "Percentagem acumulada",
            "Vital few (80%)" => "Poucos vitais (80%)",
            "Largest share (%)" => "Maior quota (%)",
            "Categories" => "Categorias",
            "Category" => "Categoria",
            "Thresholds" => "Limiares",
            "Contribution %" => "Contribuição %",
            "Replicates per run" => "Repetições por ensaio",
            "SS (S/N)" => "SQ (S/N)",
            "Error (pooled)" => "Erro (agrupado)",
            "Grand mean" => "Média global",
            "Best level" => "Melhor nível",
            "Delta" => "Amplitude",
            "S/N ratio" => "Razão S/N",
            "Strongest factor" => "Fator mais forte",
            "Levels" => "Níveis",
            "Sample size" => "Tamanho da amostra",
            "Acceptance number" => "Número de aceitação",
            "Lot size" => "Tamanho do lote",
            "Producer risk" => "Risco do produtor",
            "Consumer risk" => "Risco do consumidor",
            "P(accept) at AQL" => "P(aceitar) no NQA",
            "P(accept) at LTPD" => "P(aceitar) no LTPD",
            "P(accept) at 1%" => "P(aceitar) a 1%",
            "P(accept) at 2%" => "P(aceitar) a 2%",
            "P(accept) at 5%" => "P(aceitar) a 5%",
            "AOQL" => "NQA",
            "AOQL rate" => "Taxa do NQA",
            "Defective rate" => "Taxa de defeituosos",
            "P(accept)" => "P(aceitar)",
            "Plan" => "Plano",
            "Variables" => "Variáveis",
            "Status" => "Estado",
            "Largest T²" => "T² máximo",
            "Points out of control" => "Pontos fora de controlo",
            "F critical" => "F crítico",
            "F denominator df" => "GL do denominador de F",
            "out of control" => "fora de controlo",
            "in control" => "sob controlo",
            "Component" => "Componente",
            "Cluster" => "Grupo",
            "Appraiser" => "Avaliador",
            "Appraiser 1" => "Avaliador 1",
            "Appraiser 2" => "Avaliador 2",
            "Parts" => "Peças",
            "Ratings" => "Avaliações",
            "Agreement" => "Concordância",
            "Kappa" => "Kappa",
            "Expected" => "Esperado",
            "Mean within kappa" => "Kappa médio intra-avaliador",
            "Within appraiser" => "Intra-avaliador",
            "Between appraisers" => "Inter-avaliadores",
            "Clusters" => "Grupos",
            "Size" => "Dimensão",
            "Distance" => "Distância",
            "Within sum of squares" => "Soma de quadrados intra-grupos",
            "Between sum of squares" => "Soma de quadrados inter-grupos",
            "Explained proportion" => "Proporção explicada",
            "Iterations" => "Iterações",
            "Smallest cluster" => "Menor grupo",
            "Largest cluster" => "Maior grupo",
            "Standardisation" => "Padronização",
            "Initialisation" => "Inicialização",
            "Sums of squares" => "Somas de quadrados",
            "Eigenvalue" => "Valor próprio",
            "Proportion" => "Proporção",
            "Cumulative" => "Acumulada",
            "Variable" => "Variável",
            "Row" => "Linha",
            "Loadings" => "Cargas",
            "Moving average" => "Média móvel",
            "Trend" => "Tendência",
            "Seasonal" => "Sazonal",
            "Adjusted" => "Ajustado",
            "Seasonally adjusted" => "Ajustado sazonalmente",
            "Seasonal index" => "Índice sazonal",
            "Seasonal period" => "Período sazonal",
            "Seasonal strength" => "Intensidade sazonal",
            "Seasonal indices" => "Índices sazonais",
            "Position" => "Posição",
            "Window" => "Janela",
            "Fitted" => "Ajustado",
            "Forecast" => "Previsão",
            "Forecasts" => "Previsões",
            "Period" => "Período",
            "Alpha" => "Alfa",
            "Beta" => "Beta",
            "Gamma" => "Gama",
            "MAD" => "DMA",
            "MSD" => "DMQ",
            "MAPE" => "EPMA",
            "SSE" => "SQR",
            "Constant" => "Constante",
            "Lambda 95% CI lower" => "Limite inferior do IC 95% de lambda",
            "Lambda 95% CI upper" => "Limite superior do IC 95% de lambda",
            "Odds ratio" => "Razão de possibilidades",
            "Rate ratio" => "Razão de taxas",
            "Total counts" => "Total de contagens",
            "Null deviance" => "Desvio nulo",
            "Residual df" => "GL residuais",
            "Pearson chi-square" => "Qui-quadrado de Pearson",
            "Dispersion" => "Dispersão",
            "Events (y = 1)" => "Acontecimentos (y = 1)",
            "Null log-likelihood" => "Log-verosimilhança nula",
            "Log-likelihood" => "Log-verosimilhança",
            "Deviance" => "Desvio",
            "Likelihood ratio" => "Razão de verosimilhanças",
            "LR p-value" => "p da razão de verosimilhanças",
            "Predicted 0" => "Previsto 0",
            "Predicted 1" => "Previsto 1",
            "Observed" => "Observado",
            "Risk group" => "Grupo de risco",
            "Probability range" => "Intervalo de probabilidade",
            "Expected 1" => "Esperado 1",
            "Observed 1" => "Observado 1",
            "Measurements" => "Medições",
            "Bias" => "Desvio",
            "Mean bias" => "Desvio médio",
            "Slope" => "Declive",
            "Linearity" => "Linearidade",
            "Process variation" => "Variação do processo",
            "Reference parts" => "Peças de referência",
            "Exact test" => "Teste exacto",
            "Fisher exact p (two-sided)" => "p exacto de Fisher (bilateral)",
            "Fisher exact p (lower)" => "p exacto de Fisher (inferior)",
            "Fisher exact p (upper)" => "p exacto de Fisher (superior)",
            "P (Tukey)" => "p (Tukey)",
            "P (Bonferroni)" => "p (Bonferroni)",
            "Quartiles" => "Quartis",
            "Mean interval" => "Intervalo da média",
            "Coding" => "Codificação",
            "Estimator" => "Estimador",
            "Limits" => "Limites",
            "Tests" => "Testes",
            "Control limits" => "Limites de controlo",
            "Reference value" => "Valor de referência",
            "Decision interval" => "Intervalo de decisão",
            "Subgroups" => "Subgrupos",
            "Subgroup size" => "Tamanho do subgrupo",
            "Subgroups used" => "Subgrupos usados",
            "Sigma estimate" => "Estimativa de sigma",
            "Terms" => "Termos",
            "F p-value" => "p do F",
            "Points outside limits" => "Pontos fora dos limites",
            "Mean subgroup size" => "Tamanho médio do subgrupo",
            "Overdispersion factor (sigma_z)" => "Fator de sobredispersão (sigma_z)",
            "Overdispersion" => "Sobredispersão",
            "Base" => "Base",
            "Fold-over" => "Dobra",
            "Scope" => "Âmbito",
            "Replication" => "Replicação",
            "Incomplete subgroup" => "Subgrupo incompleto",
            "Missing values" => "Valores em falta",
            "Rows" => "Linhas",
            "Columns" => "Colunas",
            _ => text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogues_cover_both_languages() {
        let en = Catalog::english();
        let pt = Catalog::new(Lang::Portuguese);
        assert_eq!(en.observations(), "Observations (n)");
        assert_eq!(pt.observations(), "Observações (n)");
        assert_eq!(pt.warnings(), "AVISOS");
        assert_ne!(en.estimation_method(), pt.estimation_method());
    }

    #[test]
    fn labels_are_translated_and_unknown_ones_are_kept() {
        let en = Catalog::english();
        let pt = Catalog::new(Lang::Portuguese);
        assert_eq!(en.label("Term"), "Term");
        assert_eq!(pt.label("Term"), "Termo");
        assert_eq!(pt.label("Center line"), "Linha central");
        assert_eq!(pt.label("Something else"), "Something else");
    }

    #[test]
    fn language_codes_round_trip() {
        assert_eq!(Lang::parse("en"), Some(Lang::English));
        assert_eq!(Lang::parse("pt-PT"), Some(Lang::Portuguese));
        assert_eq!(Lang::Portuguese.code(), "pt");
        assert!(Lang::parse("fr").is_none());
    }
}
