//! The language catalogue of the desktop interface.
//!
//! The engine owns the language of the reports (`fp_core::i18n`); this
//! module owns the language of the menus and the dialogs. The catalogue maps
//! the English interface strings to their Portuguese counterparts and leaves
//! the strings it does not know untouched.

use fp_core::i18n::Lang;

/// Translates the interface strings of the desktop application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiCatalog {
    /// Language of the interface.
    pub language: Lang,
}

impl UiCatalog {
    /// Builds the catalogue for a language.
    #[must_use]
    pub fn new(language: Lang) -> Self {
        Self { language }
    }

    /// Translates a label of the interface.
    #[must_use]
    pub fn label(self, text: &str) -> &str {
        if self.language == Lang::English {
            return text;
        }
        match text {
            // Menus.
            "File" => "Ficheiro",
            "Edit" => "Editar",
            "Data" => "Dados",
            "Calc" => "Calcular",
            "Stat" => "Estatística",
            "Graph" => "Gráfico",
            "Editor" => "Editor",
            "Tools" => "Ferramentas",
            "Window" => "Janela",
            "Help" => "Ajuda",
            "Basic Statistics" => "Estatística básica",
            "Regression" => "Regressão",
            "ANOVA" => "ANOVA",
            "DOE" => "DOF",
            "Reliability" => "Fiabilidade",
            "Control Charts" => "Cartas de controlo",
            "Quality Tools" => "Ferramentas da qualidade",
            // File.
            "New Worksheet" => "Nova folha",
            "Open CSV…" => "Abrir CSV…",
            "Open Project…" => "Abrir projecto…",
            "Open Excel…" => "Abrir Excel…",
            "Save CSV As…" => "Guardar CSV como…",
            "Save Project As…" => "Guardar projecto como…",
            "Export Session PDF…" => "Exportar sessão em PDF…",
            "Export Session HTML…" => "Exportar sessão em HTML…",
            "Exit" => "Sair",
            // Edit.
            "Undo" => "Anular",
            "Redo" => "Repetir",
            "Copy" => "Copiar",
            "Select All" => "Seleccionar tudo",
            "Find…" => "Localizar…",
            "Preferences…" => "Preferências…",
            // Assistant and context menus.
            "Assistant" => "Assistente",
            "Keyboard shortcuts" => "Atalhos de teclado",
            "Dark mode" => "Modo escuro",
            "Choose an analysis…" => "Escolher uma análise…",
            "Describe my data" => "Descrever os meus dados",
            "Test normality" => "Testar a normalidade",
            "Control chart" => "Gráfico de controlo",
            "Process capability" => "Capacidade do processo",
            "Print…" => "Imprimir…",
            "Cut" => "Cortar",
            "Paste" => "Colar",
            "Insert row above" => "Inserir linha acima",
            "Insert row below" => "Inserir linha abaixo",
            "Delete row" => "Eliminar linha",
            "Add numeric column" => "Adicionar coluna numérica",
            "Add text column" => "Adicionar coluna de texto",
            "Delete column" => "Eliminar coluna",
            "Rename column…" => "Mudar o nome da coluna…",
            "Rename…" => "Mudar o nome…",
            "Close worksheet" => "Fechar folha",
            "New worksheet" => "Nova folha",
            // Data.
            "Import Excel…" => "Importar Excel…",
            "Export Excel…" => "Exportar Excel…",
            "Add Column" => "Adicionar coluna",
            "Delete Column" => "Eliminar coluna",
            "Rename Column…" => "Mudar o nome da coluna…",
            "Rename Worksheet…" => "Mudar o nome da folha…",
            "Clear Cells" => "Limpar células",
            "Recalculate" => "Recalcular",
            // Statistics.
            "Descriptive Statistics…" => "Estatística descritiva…",
            "Probability Plot…" => "Gráfico de probabilidade…",
            "Normality Test (AD)" => "Teste de normalidade (AD)",
            "Kolmogorov–Smirnov Test" => "Teste de Kolmogorov–Smirnov",
            "Ryan–Joiner Test" => "Teste de Ryan–Joiner",
            "D'Agostino–Pearson Test" => "Teste de D'Agostino–Pearson",
            "1-Sample t…" => "t de 1 amostra…",
            "2-Sample t…" => "t de 2 amostras…",
            "Paired t…" => "t pareado…",
            "Equivalence Test…" => "Teste de equivalência…",
            "Mann-Whitney…" => "Mann-Whitney…",
            "Wilcoxon Signed-Rank…" => "Wilcoxon de sinais ordenados…",
            "Kruskal-Wallis…" => "Kruskal-Wallis…",
            "Friedman…" => "Friedman…",
            "Mood's Median Test…" => "Teste da mediana de Mood…",
            "Correlation Matrix…" => "Matriz de correlação…",
            "Partial Correlation…" => "Correlação parcial…",
            "Box-Cox Transformation…" => "Transformação Box-Cox…",
            "Curve Fit…" => "Ajuste de curva…",
            "Chi-Square Test…" => "Teste qui-quadrado…",
            "Two-Way ANOVA…" => "ANOVA de dois factores…",
            "One-Way ANOVA…" => "ANOVA de um factor…",
            "Power and Sample Size…" => "Potência e tamanho amostral…",
            // Quality tools.
            "Capability Analysis…" => "Análise de capacidade…",
            "Distribution Comparison…" => "Comparação de distribuições…",
            "Gage R&R…" => "Gage R&R…",
            "Type 1 Gage Study…" => "Estudo Gage de tipo 1…",
            // Control charts.
            "I-MR Chart…" => "Carta I-MR…",
            "Xbar-R Chart…" => "Carta Xbar-R…",
            "Xbar-S Chart…" => "Carta Xbar-S…",
            "EWMA Chart…" => "Carta EWMA…",
            "CUSUM Chart…" => "Carta CUSUM…",
            "P/NP/C/U Chart…" => "Carta P/NP/C/U…",
            "G Chart (Rare Events)…" => "Carta g (eventos raros)…",
            "Run Chart…" => "Carta de sequências…",
            // Graph.
            "Show Graph" => "Mostrar gráfico",
            "Export Graph as SVG…" => "Exportar gráfico em SVG…",
            // Editor.
            "Undo Edit" => "Anular edição",
            "Redo Edit" => "Repetir edição",
            "Clear Sheet" => "Limpar folha",
            "Recalculate Sheet" => "Recalcular folha",
            // Tools.
            "Plugins…" => "Extensões…",
            "Language" => "Idioma",
            "English" => "Inglês",
            "Português" => "Português",
            "About FreePlots" => "Sobre o FreePlots",
            // Window.
            "Show Session" => "Mostrar sessão",
            "Hide Session" => "Esconder sessão",
            "Show Worksheet" => "Mostrar folha",
            "Hide Worksheet" => "Esconder folha",
            // Dialog titles.
            "Probability Plot" => "Gráfico de probabilidade",
            "Descriptive Statistics" => "Estatística descritiva",
            "Capability Analysis" => "Análise de capacidade",
            "Gage R&R" => "Gage R&R",
            "Type 1 Gage Study" => "Estudo Gage de tipo 1",
            "Control Chart" => "Carta de controlo",
            "Attribute Chart" => "Carta de atributos",
            "Run Chart" => "Carta de sequências",
            "One-Way ANOVA" => "ANOVA de um factor",
            "Two-Way ANOVA" => "ANOVA de dois factores",
            "Curve Fit" => "Ajuste de curva",
            "Correlation Matrix" => "Matriz de correlação",
            "Partial Correlation" => "Correlação parcial",
            "Box-Cox Transformation" => "Transformação Box-Cox",
            "Tolerance Interval" => "Intervalo de tolerância",
            "Gage Linearity and Bias" => "Linearidade e desvio do sistema de medição",
            "Binary Logistic Regression" => "Regressão logística binária",
            "Poisson Regression" => "Regressão de Poisson",
            "Ordinal Logistic Regression" => "Regressão logística ordinal",
            "Ordinal Logistic Regression…" => "Regressão logística ordinal…",
            "Stepwise Regression" => "Regressão passo a passo",
            "Stepwise Regression…" => "Regressão passo a passo…",
            "Poisson Regression…" => "Regressão de Poisson…",
            "Binary Logistic Regression…" => "Regressão logística binária…",
            "Gage Linearity…" => "Linearidade do sistema…",
            "Box-Behnken Design" => "Delineamento Box-Behnken",
            "Plackett-Burman Design" => "Delineamento Plackett-Burman",
            "Plackett-Burman Design…" => "Delineamento Plackett-Burman…",
            "Time Series" => "Séries temporais",
            "Sort Worksheet" => "Ordenar folha de dados",
            "Sort…" => "Ordenar…",
            "Transpose" => "Transpor",
            "Stack Columns" => "Empilhar colunas",
            "Stack…" => "Empilhar…",
            "Unstack Column" => "Desempilhar coluna",
            "Unstack…" => "Desempilhar…",
            "Subset Worksheet" => "Subconjunto da folha",
            "Subset…" => "Subconjunto…",
            "Keys:" => "Chaves:",
            "Condition:" => "Condição:",
            "Value:" => "Valor:",
            "Multivariate" => "Multivariada",
            "Principal Component Analysis" => "Análise em componentes principais",
            "K-Means Clustering" => "Agrupamento k-médias",
            "Hotelling T² Chart" => "Carta T² de Hotelling",
            "Acceptance Sampling" => "Amostragem de aceitação",
            "Pareto Chart" => "Gráfico de Pareto",
            "Interval Plot" => "Gráfico de intervalos",
            "Matrix Plot" => "Gráfico de matriz",
            "Dotplot" => "Gráfico de pontos",
            "Stem-and-Leaf" => "Caule-e-folhas",
            "Time Series Plot" => "Gráfico de séries temporais",
            "Time Series Plot…" => "Gráfico de séries temporais…",
            "Stem-and-Leaf…" => "Caule-e-folhas…",
            "Empirical CDF" => "FDC empírica",
            "Empirical CDF…" => "FDC empírica…",
            "Dotplot…" => "Gráfico de pontos…",
            "Matrix Plot…" => "Gráfico de matriz…",
            "Interval Plot…" => "Gráfico de intervalos…",
            "Calculator" => "Calculadora",
            "Accelerated Life Testing" => "Ensaios de vida acelerada",
            "Accelerated Life Testing…" => "Ensaios de vida acelerada…",
            "Calculator…" => "Calculadora…",
            "Pareto Chart…" => "Gráfico de Pareto…",
            "Taguchi Design" => "Delineamento Taguchi",
            "Taguchi Design…" => "Delineamento Taguchi…",
            "Taguchi Analysis" => "Análise Taguchi",
            "Taguchi Analysis…" => "Análise Taguchi…",
            "Acceptance Sampling…" => "Amostragem de aceitação…",
            "Hotelling T² Chart…" => "Carta T² de Hotelling…",
            "Attribute Agreement" => "Concordância de atributos",
            "Attribute Agreement…" => "Concordância de atributos…",
            "K-Means Clustering…" => "Agrupamento k-médias…",
            "Principal Components…" => "Componentes principais…",
            "Moving Average…" => "Média móvel…",
            "Decomposition…" => "Decomposição…",
            "Exponential Smoothing…" => "Suavização exponencial…",
            "Box-Behnken Design…" => "Delineamento Box-Behnken…",
            "Tolerance Interval…" => "Intervalo de tolerância…",
            "Chi-Square Test for Independence" => "Teste qui-quadrado de independência",
            "Power and Sample Size" => "Potência e tamanho amostral",
            "Equivalence Test" => "Teste de equivalência",
            "Mann-Whitney Test" => "Teste de Mann-Whitney",
            "Wilcoxon Signed-Rank Test" => "Teste de Wilcoxon de sinais ordenados",
            "Kruskal-Wallis Test" => "Teste de Kruskal-Wallis",
            "Friedman Test" => "Teste de Friedman",
            "Mood's Median Test" => "Teste da mediana de Mood",
            "G Chart (Rare Events)" => "Carta g (eventos raros)",
            // Help.
            "User Guide" => "Guia do utilizador",
            "About" => "Acerca",
            _ => text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_english_catalogue_leaves_the_strings_untouched() {
        let catalog = UiCatalog::new(Lang::English);
        assert_eq!(catalog.label("File"), "File");
    }

    #[test]
    fn the_portuguese_catalogue_translates_the_menus() {
        let catalog = UiCatalog::new(Lang::Portuguese);
        assert_eq!(catalog.label("File"), "Ficheiro");
        assert_eq!(catalog.label("Basic Statistics"), "Estatística básica");
        // An unknown string is left untouched.
        assert_eq!(catalog.label("Something else"), "Something else");
    }
}
