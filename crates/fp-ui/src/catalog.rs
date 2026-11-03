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
            "Compare two groups" => "Comparar dois grupos",
            "Compare three or more groups" => "Comparar três ou mais grupos",
            "Test a proportion" => "Testar uma proporção",
            "Check a process (control chart)" => "Verificar um processo (carta de controlo)",
            "Measure process capability" => "Medir a capacidade do processo",
            "Fit a regression" => "Ajustar uma regressão",
            "Design an experiment" => "Planear uma experiência",
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
            "Where the settings are stored…" => "Onde estão guardadas as definições…",
            "Third-party licences…" => "Licenças de terceiros…",
            "Show the tour" => "Ver o tour",
            "Edit graph labels…" => "Editar rótulos do gráfico…",
            "Add reference line…" => "Adicionar linha de referência…",
            "Copy graph as SVG" => "Copiar gráfico como SVG",
            "Reset graph edits" => "Repor edições do gráfico",
            "Graph labels" => "Rótulos do gráfico",
            "Title" => "Título",
            "Subtitle" => "Subtítulo",
            "X axis" => "Eixo X",
            "Y axis" => "Eixo Y",
            "Footer" => "Rodapé",
            "Empty hides the line" => "Vazio esconde a linha",
            "Apply" => "Aplicar",
            "Reference line" => "Linha de referência",
            "Vertical (x = value)" => "Vertical (x = valor)",
            "Horizontal (y = value)" => "Horizontal (y = valor)",
            "Value" => "Valor",
            "Label" => "Rótulo",
            "Add" => "Adicionar",
            "Tour" => "Tour",
            "Sample projects" => "Projectos de exemplo",
            "Filling line capability" => "Capacidade da linha de enchimento",
            "Gage R&R (3 operators)" => "Gage R&R (3 operadores)",
            "Screening experiment (2^2)" => "Experiência de triagem (2^2)",
            "Back" => "Anterior",
            "Next" => "Seguinte",
            "Finish" => "Terminar",
            "The worksheet" => "A folha de cálculo",
            "The toolbar" => "A barra de ferramentas",
            "The menus" => "Os menus",
            "The Session" => "A sessão",
            "The plots" => "Os gráficos",
            "Click any cell and type; Enter commits the value or a formula such as =SUM(C1R1:C1R10). Drag the column dividers to widen a column, and use the arrows at the bottom to switch between worksheets." => {
                "Clique numa célula e escreva; Enter confirma o valor ou uma fórmula como =SUM(C1R1:C1R10). Arraste os separadores para alargar a coluna e use as setas no fundo para mudar de folha."
            }
            "Open a CSV or Excel file, save the worksheet or the project, compute a column, undo and redo. The formula bar above the grid edits the selected cell." => {
                "Abra um CSV ou Excel, guarde a folha ou o projecto, calcule uma coluna, anule e repita. A barra de fórmulas acima da grelha edita a célula selecionada."
            }
            "Stat holds every analysis, Graph the plots and Assistant suggests one from a plain question. Hover over any menu item to read what it does." => {
                "O menu Estatística tem as análises, o Gráfico os gráficos e o Assistente sugere um a partir de uma pergunta simples. Passe o rato sobre qualquer item para ler o que faz."
            }
            "Every report appears here, in the same order as the analyses. Clear Session empties it; the logs travel with the exported PDF or HTML." => {
                "Todos os relatórios aparecem aqui, pela ordem das análises. Limpar sessão esvazia-a; os relatórios seguem no PDF ou HTML exportado."
            }
            "Plots open in their own window: right-click for the labels, reference lines and saving, and close it to keep working." => {
                "Os gráficos abrem numa janela própria: clique com o botão direito para os rótulos, linhas de referência e gravação; feche-a para continuar a trabalhar."
            }
            "This tour lives under Help, together with the keyboard shortcuts, the settings path, the MCP guide and the third-party notices." => {
                "Este tour vive na Ajuda, com os atalhos de teclado, o caminho das definições, o guia MCP e os avisos de terceiros."
            }
            "Third-party licences" => "Licenças de terceiros",
            "The notices that must accompany every copy" => {
                "Os avisos que acompanham todas as cópias"
            }
            "Independent project, not affiliated with or endorsed by any commercial statistical software vendor." => {
                "Projecto independente, sem afiliação nem aprovação de qualquer fornecedor de software estatístico comercial."
            }
            "These notices must accompany every copy of the program; they also ship as LICENSES.txt next to the executable." => {
                "Estes avisos acompanham todas as cópias do programa; seguem também como LICENSES.txt junto do executável."
            }
            "Where the settings are stored" => "Onde estão guardadas as definições",
            "Assistant for agents…" => "Assistente para agentes…",
            "Assistant for agents" => "Assistente para agentes",
            "FreePlots keeps these options in a small JSON file: language, theme, zoom, the Session panel, the grid lines and the native plot windows. The file is written as soon as one of them changes." => {
                "O FreePlots guarda estas opções num pequeno ficheiro JSON: idioma, tema, zoom, painel da sessão, linhas da grelha e janelas nativas dos gráficos. O ficheiro é escrito assim que uma delas muda."
            }
            "The program uses FreePlots-data next to the executable when it can write there, so a USB stick keeps its own settings; otherwise the user data directory is used." => {
                "O programa usa a pasta FreePlots-data ao lado do executável quando pode escrever lá, para uma pen USB manter as suas próprias definições; caso contrário usa a pasta de dados do utilizador."
            }
            "This build has no settings file (the command line does not keep preferences)." => {
                "Esta versão não tem ficheiro de definições (a linha de comandos não guarda preferências)."
            }
            "Copy path" => "Copiar caminho",
            "Theme" => "Tema",
            "Dark" => "Escuro",
            "Light" => "Claro",
            "Zoom" => "Zoom",
            "Session panel" => "Painel da sessão",
            "Grid lines" => "Linhas da grelha",
            "Native plot windows" => "Janelas nativas dos gráficos",
            "Shown" => "Visível",
            "Hidden" => "Oculto",
            "On" => "Ligado",
            "Off" => "Desligado",
            "The file could not be written:" => "Não foi possível escrever o ficheiro:",
            "Removing the file restores the defaults; it is plain JSON and can be edited by hand." => {
                "Apagar o ficheiro repõe os valores por omissão; é JSON simples e pode ser editado à mão."
            }
            "FreePlots embeds no model: you choose the MCP client (the AI assistant you already use). The client opens files, runs analyses and reads the Session through the tools below." => {
                "O FreePlots não inclui modelo: escolhes o cliente MCP (o assistente de IA que já usas). O cliente abre ficheiros, corre análises e lê a sessão através das ferramentas abaixo."
            }
            "Launch lines" => "Linhas de arranque",
            "Client configuration" => "Configuração do cliente",
            "Copy configuration" => "Copiar configuração",
            "An stdio entry for a client that starts FreePlots itself." => {
                "Uma entrada stdio para um cliente que arranca o FreePlots."
            }
            "Tools: open a CSV, describe, capability, control chart, regression, ANOVA, normality, probability plot, Session, status and screenshot. The live window also accepts menu paths, clicks, text and scripts." => {
                "Ferramentas: abrir CSV, descritivas, capacidade, carta de controlo, regressão, ANOVA, normalidade, gráfico de probabilidade, sessão, estado e screenshot. A janela ao vivo aceita também caminhos de menu, cliques, texto e scripts."
            }
            "The bridge is for a trusted network: it can change the open worksheet. Keep --token private when the address is not loopback." => {
                "O bridge é para uma rede de confiança: pode alterar a folha aberta. Mantém o --token privado quando o endereço não é loopback."
            }
            "Desktop tools over stdio (headless worksheet)" => {
                "Ferramentas de desktop por stdio (folha sem janela)"
            }
            "Same tools over HTTP on this machine" => {
                "As mesmas ferramentas por HTTP nesta máquina"
            }
            "Live window: start the bridge, then the proxy" => {
                "Janela ao vivo: arrancar o bridge e depois o proxy"
            }
            "Token for a bridge reachable from the network" => {
                "Token para um bridge acessível pela rede"
            }
            "Statistics and analysis software written in Rust by AI, accessible to everyone." => {
                "Software de análise e estatística escrito em Rust por IA, acessível a todos."
            }
            "MIT license" => "Licença MIT",
            // Help.
            "New worksheet, open and save as" => "Nova folha, abrir e guardar como",
            "Print the Session report" => "Imprimir o relatório da sessão",
            "Undo and redo" => "Anular e repetir",
            "Copy, cut and paste" => "Copiar, cortar e colar",
            "Find and replace" => "Localizar e substituir",
            "Fill the selection down and right" => {
                "Preencher a seleção para baixo e para a direita"
            }
            "Select the whole worksheet" => "Selecionar toda a folha",
            "Clear the selected cells" => "Limpar as células selecionadas",
            "Move or extend the selection" => "Mover ou estender a seleção",
            "Next column" => "Coluna seguinte",
            "Commit the edit and move down" => "Confirmar a edição e descer",
            "Cancel the edit or close the dialog" => "Cancelar a edição ou fechar a caixa",
            "Zoom in, out and reset" => "Aumentar, reduzir e repor o zoom",
            "Edit a cell" => "Editar uma célula",
            "Context menu of the cell, header or tab" => {
                "Menu da célula, do cabeçalho ou do separador"
            }
            "Ctrl and Shift combinations use the keyboard layout in use; the grid never captures keys while a text field has focus." => {
                "As combinações com Ctrl e Shift seguem o layout do teclado em uso; a grelha nunca capta teclas enquanto um campo de texto tem o foco."
            }
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
