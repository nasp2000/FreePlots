// SPDX-License-Identifier: MIT
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
            "FreePlots Engine" => "Motor FreePlots",
            "Where the settings are stored…" => "Onde estão guardadas as definições…",
            "Third-party licences…" => "Licenças de terceiros…",
            "Show the tour" => "Ver o tour",
            "Graph options…" => "Opções do gráfico…",
            "Graph text…" => "Texto do gráfico…",
            "Batch descriptive statistics…" => "Estatísticas descritivas em lote…",
            "Batch descriptive statistics" => "Estatísticas descritivas em lote",
            "Folder:" => "Pasta:",
            "Folder with .csv files" => "Pasta com ficheiros .csv",
            "Choose the folder with the CSV files" => "Escolher a pasta com os ficheiros CSV",
            "Every .csv in the folder is summarised in one Session table; unreadable files are listed with the reason." => {
                "Cada .csv da pasta é resumido numa tabela da sessão; os ficheiros ilegíveis aparecem com o motivo."
            }
            "Enter the folder with the CSV files" => "Indique a pasta com os ficheiros CSV",
            "Select the column to summarise" => "Escolha a coluna a resumir",
            "No CSV files found in the folder" => "Não há ficheiros CSV na pasta",
            "Clear marks" => "Limpar marcas",
            "Add data labels…" => "Adicionar rótulos de dados…",
            "Axis scale…" => "Escala dos eixos…",
            "Data display…" => "Apresentação dos dados…",
            "Data display" => "Apresentação dos dados",
            "Symbols" => "Símbolos",
            "Lines" => "Linhas",
            "Bands / bars" => "Bandas / barras",
            "Show all" => "Mostrar tudo",
            "Unchecking an element hides it in the window and in the saved SVG/PDF; the settings are kept between sessions." => {
                "Desmarcar um elemento esconde-o na janela e no SVG/PDF guardado; as definições ficam entre sessões."
            }
            "Axis scale" => "Escala dos eixos",
            "Axis scale updated" => "Escala dos eixos actualizada",
            "Axis scale automatic" => "Escala dos eixos automática",
            "from" => "de",
            "to" => "a",
            "Minimum" => "Mínimo",
            "Maximum" => "Máximo",
            "Automatic" => "Automática",
            "Leave both fields of an axis empty for automatic limits; probabilities and panels keep their own scales." => {
                "Deixe os dois campos de um eixo vazios para limites automáticos; as probabilidades e os painéis mantêm as suas escalas."
            }
            "Add percentile lines…" => "Adicionar linhas de percentis…",
            "Panel layout…" => "Disposição dos painéis…",
            "Toggle gridlines" => "Mostrar/ocultar grelha",
            "Toggle report data" => "Mostrar/ocultar dados do relatório",
            "Report data shown" => "Dados do relatório visíveis",
            "Report data hidden" => "Dados do relatório ocultos",
            "Data labels" => "Rótulos de dados",
            "Label:" => "Rótulo:",
            "Y value" => "Valor de Y",
            "Point number" => "Número do ponto",
            "Remove labels" => "Remover rótulos",
            "Data labels added" => "Rótulos de dados adicionados",
            "Data labels removed" => "Rótulos de dados removidos",
            "Percentile lines" => "Linhas de percentis",
            "X values" => "Valores de X",
            "Y values" => "Valores de Y",
            "Percentiles:" => "Percentis:",
            "Panel layout" => "Disposição dos painéis",
            "Columns:" => "Colunas:",
            "Panels are filled left to right, top to bottom." => {
                "Os painéis são preenchidos da esquerda para a direita, de cima para baixo."
            }
            "Colour:" => "Cor:",
            "Black" => "Preto",
            "Red" => "Vermelho",
            "Blue" => "Azul",
            "Green" => "Verde",
            "Angle:" => "Ângulo:",
            "Gridlines shown" => "Grelha visível",
            "Gridlines hidden" => "Grelha oculta",
            "One label per point, drawn next to the marker." => {
                "Um rótulo por ponto, desenhado junto ao marcador."
            }
            "Comma-separated percentages between 0 and 100; each adds a labelled reference line." => {
                "Percentagens entre 0 e 100 separadas por vírgulas; cada uma acrescenta uma linha de referência com rótulo."
            }
            "Marks cleared" => "Marcas limpas",
            "Brush points: Ctrl+click a point" => "Ctrl+clique num ponto para o marcar",
            "Conditional formatting…" => "Formatação condicional…",
            "Conditional formatting" => "Formatação condicional",
            "Greater than" => "Maior que",
            "Less than" => "Menor que",
            "Between" => "Entre",
            "Equal to" => "Igual a",
            "Contains" => "Contém",
            "and" => "e",
            "Style:" => "Estilo:",
            "Red fill" => "Fundo vermelho",
            "Yellow fill" => "Fundo amarelo",
            "Green fill" => "Fundo verde",
            "Red text" => "Texto vermelho",
            "Bold" => "Negrito",
            "Add rule" => "Adicionar regra",
            "Clear rules" => "Limpar regras",
            "Remove" => "Remover",
            "Remove this rule" => "Remover esta regra",
            "No rules yet." => "Ainda sem regras.",
            "Close" => "Fechar",
            "Conditional rule added" => "Regra condicional adicionada",
            "Conditional rule removed" => "Regra condicional removida",
            "Conditional rules cleared" => "Regras condicionais limpas",
            "The first rule that matches a cell wins; the rules follow the column name and are kept between sessions." => {
                "Vale a primeira regra que corresponde à célula; as regras seguem o nome da coluna e ficam guardadas entre sessões."
            }
            "Add note…" => "Adicionar nota…",
            "Notes…" => "Notas…",
            "Notes" => "Notas",
            "Duplicate" => "Duplicar",
            "To front" => "Para a frente",
            "Rotate left" => "Rodar à esquerda",
            "Rotate right" => "Rodar à direita",
            "Note duplicated" => "Nota duplicada",
            "Note removed" => "Nota removida",
            "Note brought to front" => "Nota trazida para a frente",
            "Add note to the graph" => "Adicionar nota ao gráfico",
            "Text:" => "Texto:",
            "Note to place on the graph" => "Nota a colocar no gráfico",
            "Position:" => "Posição:",
            "Note added to the graph" => "Nota adicionada ao gráfico",
            "0 % is the top-left corner of the plot and 100 % the bottom-right; the text is kept between sessions." => {
                "0 % é o canto superior esquerdo do gráfico e 100 % o inferior direito; o texto fica guardado entre sessões."
            }
            "Graph text" => "Texto do gráfico",
            "Graph text updated" => "Texto do gráfico actualizado",
            "Typeface:" => "Tipo de letra:",
            "Text size:" => "Tamanho do texto:",
            "Sans" => "Sem serifa",
            "Serif" => "Com serifa",
            "Monospace" => "Monoespaçado",
            "Reset" => "Repor",
            "The size and typeface apply to every text drawn on the graph; the settings are kept for the next session." => {
                "O tamanho e o tipo de letra aplicam-se a todo o texto do gráfico; as definições ficam guardadas para a próxima sessão."
            }
            "Add reference lines…" => "Adicionar linha de referência…",
            "Copy graph as SVG" => "Copiar gráfico como SVG",
            "Save graph…" => "Guardar gráfico…",
            "Print graph…" => "Imprimir gráfico…",
            "Save as PNG…" => "Guardar como PNG…",
            "Save as JPEG…" => "Guardar como JPEG…",
            "Save Graph as PNG" => "Guardar gráfico como PNG",
            "Save Graph as JPEG" => "Guardar gráfico como JPEG",
            "Print Graph" => "Imprimir gráfico",
            "No graph to print" => "Não há gráfico para imprimir",
            "Save Graph As" => "Guardar gráfico como",
            "Destination SVG file:" => "Ficheiro SVG de destino:",
            "Graph edits reset" => "Edições do gráfico repostas",
            "Graph options updated" => "Opções do gráfico actualizadas",
            "Reset graph edits" => "Repor edições do gráfico",
            "Graph options" => "Opções do gráfico",
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
            "Recover unsaved work" => "Recuperar trabalho não guardado",
            "A recovery copy from an earlier session is available." => {
                "Está disponível uma cópia de recuperação de uma sessão anterior."
            }
            "Restore" => "Restaurar",
            "Discard" => "Descartar",
            "Later" => "Mais tarde",
            "Diagnostics…" => "Diagnósticos…",
            "Diagnostics" => "Diagnósticos",
            "Version and paths for a bug report:" => "Versão e caminhos para um relatório de erro:",
            "Copy diagnostics" => "Copiar diagnósticos",
            "Open data folder" => "Abrir pasta de dados",
            "Visit the public FreePlots repository" => "Visite o repositório público do FreePlots",
            "Check for updates" => "Verificar atualizações",
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
            "Ctrl and Shift combinations use the keyboard layout in use; the grid never records keys while a text field has focus." => {
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
            "Control chart help" => "Ajuda da carta de controlo",
            "Overview" => "Visão geral",
            "Example" => "Exemplo",
            "How to choose subgroups…" => "Como escolher os subgrupos…",
            "Overview, data and an example" => "Visão geral, dados e um exemplo",
            "The X̄–R and X̄–S charts follow the mean and the spread of each subgroup. \
             The limits come from the variation inside the subgroups, so a slow drift \
             does not widen them." => {
                "As cartas X̄–R e X̄–S acompanham a média e a dispersão de cada subgrupo. \
                 Os limites vêm da variação dentro dos subgrupos, por isso uma deriva lenta \
                 não os alarga."
            }
            "One numeric column holds the measurements. Either set a constant subgroup \
             size, which takes consecutive rows, or choose a column whose labels say \
             which subgroup each row belongs to. Labels may repeat in any order: the \
             chart keeps the order in which each label first appears. Rows with a \
             missing measurement or label are not used. Each subgroup needs 2 to 25 \
             observations, and the chart needs at least two subgroups." => {
                "Uma coluna numérica contém as medições. Defina um tamanho de subgrupo \
                 constante, que usa linhas consecutivas, ou escolha uma coluna cujas \
                 etiquetas dizem a que subgrupo pertence cada linha. As etiquetas podem \
                 repetir-se em qualquer ordem: a carta mantém a ordem em que cada etiqueta \
                 aparece pela primeira vez. As linhas com uma medição ou etiqueta em falta \
                 não são usadas. Cada subgrupo precisa de 2 a 25 observações e a carta \
                 precisa de pelo menos dois subgrupos."
            }
            "The phase column and the subgroup column are alternatives: a phase gives \
             each consecutive run its own limits, a subgroup column groups the rows \
             before the limits are computed." => {
                "A coluna de fases e a coluna de subgrupos são alternativas: uma fase dá \
                 limites próprios a cada sequência consecutiva; uma coluna de subgrupos \
                 agrupa as linhas antes de calcular os limites."
            }
            "A line measures three parts twice. Put the measurements in weight and the \
             part name in part, choose X̄–R as the chart and part as the subgroup \
             column. The Session reports one point per part, the size of each subgroup \
             and the limits computed from the pooled within-subgroup variation." => {
                "Uma linha mede três peças duas vezes. Coloque as medições em weight e o \
                 nome da peça em part, escolha X̄–R como carta e part como coluna de \
                 subgrupos. A Sessão reporta um ponto por peça, o tamanho de cada subgrupo \
                 e os limites calculados a partir da variação agrupada dentro dos subgrupos."
            }
            "Capability help" => "Ajuda da capacidade",
            "Capability compares the spread of the process with the specification limits. \
             The within estimate uses the variation inside the subgroups, so the indices \
             describe what the process could do once the shifts between subgroups are \
             removed." => {
                "A capacidade compara a dispersão do processo com os limites de \
                 especificação. A estimativa dentro usa a variação dentro dos subgrupos, por \
                 isso os índices descrevem o que o processo poderia fazer depois de removidas \
                 as mudanças entre subgrupos."
            }
            "One numeric column holds the measurements. Either set a constant subgroup \
             size, which takes consecutive rows, or choose a column whose labels say \
             which subgroup each row belongs to. Rows with a missing measurement or \
             label are not used. Each subgroup needs at least two observations and the \
             analysis needs at least two subgroups. The subgroup column applies to the \
             normal model." => {
                "Uma coluna numérica contém as medições. Defina um tamanho de subgrupo \
                 constante, que usa linhas consecutivas, ou escolha uma coluna cujas \
                 etiquetas dizem a que subgrupo pertence cada linha. As linhas com uma \
                 medição ou etiqueta em falta não são usadas. Cada subgrupo precisa de pelo \
                 menos duas observações e a análise precisa de pelo menos dois subgrupos. A \
                 coluna de subgrupos aplica-se ao modelo normal."
            }
            "A line measures three parts twice. Put the measurements in weight and the \
             part name in part, set the specification limits, and choose part as the \
             subgroup column. Cp and Cpk come from the pooled within-subgroup standard \
             deviation (n − g degrees of freedom); Pp and Ppk come from the overall \
             standard deviation." => {
                "Uma linha mede três peças duas vezes. Coloque as medições em weight e o \
                 nome da peça em part, defina os limites de especificação e escolha part \
                 como coluna de subgrupos. Cp e Cpk vêm do desvio-padrão agrupado dentro dos \
                 subgrupos (n − g graus de liberdade); Pp e Ppk vêm do desvio-padrão global."
            }
            "Capability Sixpack" => "Sixpack de capacidade",
            "Capability Sixpack…" => "Sixpack de capacidade…",
            "Capability Sixpack help" => "Ajuda do Sixpack de capacidade",
            "About the six panels…" => "Acerca dos seis painéis…",
            "The Sixpack shows one normal capability study as six panels. The control \
             chart and the histogram describe the process, the probability plot checks \
             the normal model, and the capability panel compares the spread with the \
             specification limits." => {
                "O Sixpack mostra um estudo de capacidade normal em seis painéis. A carta \
                 de controlo e o histograma descrevem o processo, o gráfico de probabilidade \
                 verifica o modelo normal e o painel de capacidade compara a dispersão com os \
                 limites de especificação."
            }
            "One numeric column holds the measurements. Set a constant subgroup size, or \
             choose a column whose labels say which subgroup each row belongs to. The \
             control chart uses individuals when the subgroup size is 1 and the subgroup \
             means otherwise. The Sixpack uses the normal model and does not apply \
             Box-Cox." => {
                "Uma coluna numérica contém as medições. Defina um tamanho de subgrupo \
                 constante ou escolha uma coluna cujas etiquetas dizem a que subgrupo \
                 pertence cada linha. A carta usa observações individuais quando o tamanho do \
                 subgrupo é 1 e, caso contrário, as médias dos subgrupos. O Sixpack usa o \
                 modelo normal e não aplica Box-Cox."
            }
            "A line measures three parts twice. Put the measurements in weight and the \
             part name in part, choose Capability Sixpack, set the specification limits \
             and part as the subgroup column. The panels show the histogram, the normal \
             probability plot, the X̄ and R charts, the last 25 points and the capability \
             indices." => {
                "Uma linha mede três peças duas vezes. Coloque as medições em weight e o \
                 nome da peça em part, escolha Sixpack de capacidade, defina os limites de \
                 especificação e part como coluna de subgrupos. Os painéis mostram o \
                 histograma, o gráfico de probabilidade normal, as cartas X̄ e R, os últimos \
                 25 pontos e os índices de capacidade."
            }
            "Attribute Capability" => "Capacidade de atributos",
            "Attribute Capability…" => "Capacidade de atributos…",
            "Attribute Capability help" => "Ajuda da capacidade de atributos",
            "About attribute capability…" => "Acerca da capacidade de atributos…",
            "Attribute capability turns counted defectives or defects into parts per \
             million. The binomial study pools the defectives over the inspected units \
             and reports the proportion defective, its PPM and the normal-equivalent \
             process Z. The Poisson study pools the defects over the units and reports \
             the defects per unit (DPU) and its PPM." => {
                "A capacidade de atributos converte defeituosos ou defeitos contados em \
                 partes por milhão. O estudo binomial agrega os defeituosos sobre as unidades \
                 inspeccionadas e reporta a proporção defeituosa, o PPM e o process Z \
                 equivalente normal. O estudo de Poisson agrega os defeitos sobre as unidades \
                 e reporta os defeitos por unidade (DPU) e o respectivo PPM."
            }
            "One column holds the defectives (binomial) or the defects (Poisson) per \
             subgroup. Supply the inspected units in a second column, or set a constant \
             size. In the binomial study the defectives cannot exceed the inspected \
             units. Rows with a missing count or size are excluded and counted. Both \
             reports use an exact confidence interval at the chosen level." => {
                "Uma coluna contém os defeituosos (binomial) ou os defeitos (Poisson) por \
                 subgrupo. Indique as unidades inspeccionadas numa segunda coluna ou defina um \
                 tamanho constante. No estudo binomial os defeituosos não podem exceder as \
                 unidades inspeccionadas. As linhas com uma contagem ou tamanho em falta são \
                 excluídas e contadas. Ambos os relatórios usam um intervalo de confiança \
                 exacto no nível escolhido."
            }
            "A lot inspects 240 units in 12 subgroups and records 72 defectives: the \
             average p is 3 %, or 30,000 PPM defective, and process Z is 1.88. For \
             defects, 24 defects over 40 units give DPU 0.60 and 600,000 PPM defects." => {
                "Um lote inspecciona 240 unidades em 12 subgrupos e regista 72 defeituosos: a \
                 proporção média é 3 %, ou 30 000 PPM defeituoso, e o process Z é 1,88. Para \
                 defeitos, 24 defeitos em 40 unidades dão DPU 0,60 e 600 000 PPM de defeitos."
            }
            "T Chart (Rare Events)" => "Carta T (eventos raros)",
            "T Chart (Rare Events)…" => "Carta T (eventos raros)…",
            "T Chart help" => "Ajuda da carta T",
            "About the T chart…" => "Acerca da carta T…",
            "The T chart monitors the time between successive events. When the events \
             are rare the intervals are skewed, so the limits are the 0.135 % and \
             99.865 % quantiles of the fitted distribution and the centre line is its \
             median. A short interval means the events are clustering; a long one means \
             the process slowed." => {
                "A carta T acompanha o tempo entre eventos sucessivos. Quando os eventos são \
                 raros os intervalos são assimétricos, por isso os limites são os quantis de \
                 0,135 % e 99,865 % da distribuição ajustada e a linha central é a sua mediana. \
                 Um intervalo curto significa que os eventos se estão a agrupar; um intervalo \
                 longo significa que o processo abrandou."
            }
            "One numeric column holds the positive time between successive events, in \
             order. Rows with a missing time are excluded and counted. The exponential \
             model uses the mean interval; the gamma model estimates the shape and scale \
             from the mean and the n−1 variance, so it needs intervals that are not all \
             equal." => {
                "Uma coluna numérica contém o tempo positivo entre eventos sucessivos, pela \
                 ordem. As linhas com um tempo em falta são excluídas e contadas. O modelo \
                 exponencial usa o intervalo médio; o modelo gama estima a forma e a escala a \
                 partir da média e da variância com n−1, por isso precisa de intervalos que não \
                 sejam todos iguais."
            }
            "Five failures arrive 10, 12, 8, 15 and 9 days apart. The mean is 10.8 days, \
             so the median is 7.49 days and the exponential limits are 0.015 and 71.36 \
             days. Choose the gamma model when the intervals vary less than an \
             exponential distribution would." => {
                "Cinco falhas chegam com 10, 12, 8, 15 e 9 dias de intervalo. A média é 10,8 \
                 dias, pelo que a mediana é 7,49 dias e os limites exponenciais são 0,015 e \
                 71,36 dias. Escolha o modelo gama quando os intervalos variam menos do que uma \
                 distribuição exponencial faria."
            }
            "About CUSUM…" => "Acerca do CUSUM…",
            "CUSUM help" => "Ajuda da carta CUSUM",
            "The CUSUM chart accumulates the standardised deviations from the centre \
             line on two sides. A small shift that a Shewhart chart misses makes the \
             sums drift, so the chart reacts to a sustained change of about one sigma." => {
                "A carta CUSUM acumula os desvios padronizados em relação à linha central, dos \
                 dois lados. Uma mudança pequena que uma carta de Shewhart não detecta faz as \
                 somas derivar, por isso a carta reage a uma alteração sustentada de cerca de \
                 um sigma."
            }
            "One numeric column holds the measurements. A subgroup size of 1 charts the \
             individuals; a larger subgroup charts the subgroup means. The reference \
             value k is half the shift to detect (0.5 sigma by default) and the decision \
             interval h is the distance that raises a signal (4 sigma by default). Both \
             are set in the dialog. Rows with a missing measurement are not used." => {
                "Uma coluna numérica contém as medições. Um tamanho de subgrupo de 1 traça as \
                 observações individuais; um subgrupo maior traça as médias dos subgrupos. O \
                 valor de referência k é metade da mudança a detectar (0,5 sigma por omissão) e \
                 o intervalo de decisão h é a distância que gera um sinal (4 sigma por omissão). \
                 Ambos são definidos no diálogo. As linhas com uma medição em falta não são \
                 usadas."
            }
            "A process centred at 10 with sigma 1 shifts to 11. With k = 0.5 and h = 4 \
             each point adds about 1 − 0.5 = 0.5 to the upper sum, which crosses the \
             decision interval after eight points; the lower sum stays at zero." => {
                "Um processo centrado em 10, com sigma 1, muda para 11. Com k = 0,5 e h = 4 \
                 cada ponto acrescenta cerca de 1 − 0,5 = 0,5 à soma superior, que atravessa o \
                 intervalo de decisão ao fim de oito pontos; a soma inferior fica em zero."
            }
            "Johnson Transformation" => "Transformação de Johnson",
            "Johnson Transformation…" => "Transformação de Johnson…",
            "Johnson Transformation help" => "Ajuda da transformação de Johnson",
            "About the Johnson transformation…" => "Acerca da transformação de Johnson…",
            "The Johnson transformation maps a non-normal measurement to a \
             standard-normal score. Three families cover different shapes: SL for data \
             bounded below, SU for unbounded data, and SB for data between two limits. \
             Choose Best (AIC) to fit all three and keep the best." => {
                "A transformação de Johnson converte uma medição não normal num score normal \
                 padrão. Três famílias cobrem formas diferentes: SL para dados limitados \
                 inferiormente, SU para dados ilimitados e SB para dados entre dois limites. \
                 Escolha Best (AIC) para ajustar as três e ficar com a melhor."
            }
            "One numeric column with at least five values. Missing values are excluded \
             from the fit and stay missing in the new column. The new column holds the \
             score z and must have a name the worksheet does not use yet. The report \
             shows the fitted parameters and the Anderson–Darling test before and after." => {
                "Uma coluna numérica com pelo menos cinco valores. Os valores em falta são \
                 excluídos do ajuste e ficam em falta na nova coluna. A nova coluna contém o \
                 score z e tem de ter um nome que a folha ainda não use. O relatório mostra os \
                 parâmetros ajustados e o teste de Anderson–Darling antes e depois."
            }
            "Six measurements that rise steeply give a skewed histogram. Choose Johnson \
             Transformation with Best (AIC): the command fits the best family, adds the \
             column weight_J and reports the A² before and after. The new column is ready \
             for the capability analysis or the control charts." => {
                "Seis medições que sobem depressa dão um histograma assimétrico. Escolha \
                 Transformação de Johnson com Best (AIC): o comando ajusta a melhor família, \
                 acrescenta a coluna weight_J e reporta o A² antes e depois. A nova coluna fica \
                 pronta para a análise de capacidade ou para as cartas de controlo."
            }
            "Predict" => "Prever",
            "Predict…" => "Prever…",
            "Prediction help" => "Ajuda da previsão",
            "About prediction…" => "Acerca da previsão…",
            "Predict evaluates a fitted linear model at new predictor values. It reports \
             the fitted response, the confidence interval for the mean response and the \
             prediction interval for a single new observation." => {
                "Prever avalia um modelo linear ajustado em valores novos dos preditores. \
                 Reporta a resposta ajustada, o intervalo de confiança da resposta média e o \
                 intervalo de previsão de uma nova observação."
            }
            "Choose the response and at least one numeric predictor — the same model the \
             regression report describes. Enter one or more new points, one row per \
             point, with one value per predictor in the order of the list, separated by \
             commas or spaces. The model is fitted to every complete row when the command \
             runs; rows with a missing value are excluded listwise and counted." => {
                "Escolha a resposta e pelo menos um preditor numérico — o mesmo modelo que o \
                 relatório de regressão descreve. Introduza um ou mais pontos novos, uma linha \
                 por ponto, com um valor por preditor pela ordem da lista, separados por \
                 vírgulas ou espaços. O modelo é ajustado a todas as linhas completas quando o \
                 comando corre; as linhas com um valor em falta são excluídas em lista e \
                 contadas."
            }
            "A model of weight on height and pressure predicts the weight at height 12 \
             and pressure 105. Enter 12, 105 as one row. The report gives the fitted \
             weight and both intervals; values far outside the fitted range are \
             extrapolations." => {
                "Um modelo do peso em função da altura e da pressão prevê o peso para altura 12 \
                 e pressão 105. Introduza 12, 105 numa linha. O relatório dá o peso ajustado e \
                 ambos os intervalos; valores muito fora do intervalo ajustado são \
                 extrapolações."
            }
            "Runs Test" => "Teste de runs",
            "Runs Test…" => "Teste de runs…",
            "Runs Test help" => "Ajuda do teste de runs",
            "About the runs test…" => "Acerca do teste de runs…",
            "The runs test checks whether a sequence is random. A run is a group of \
             consecutive observations on the same side of the comparison value. Too few \
             runs suggest clustering; too many suggest an alternating pattern. The \
             p-value is two-sided." => {
                "O teste de runs verifica se uma sequência é aleatória. Um run é um grupo de \
                 observações consecutivas do mesmo lado do valor de comparação. Poucos runs \
                 sugerem agrupamento; demasiados sugerem um padrão alternado. O p-valor é \
                 bilateral."
            }
            "One numeric column, read in worksheet order. The comparison value is \
             optional: blank uses the mean of the observations. Values above it and \
             values below or equal to it form the two sides, so a value equal to the \
             comparison value joins the lower side. Rows with a missing value are \
             removed and counted. The normal approximation is reliable with at least 11 \
             observations on each side." => {
                "Uma coluna numérica, lida pela ordem da folha. O valor de comparação é \
                 opcional: em branco usa a média das observações. Os valores acima dele e os \
                 valores abaixo ou iguais formam os dois lados, pelo que um valor igual ao de \
                 comparação junta-se ao lado inferior. As linhas com um valor em falta são \
                 removidas e contadas. A aproximação normal é fiável com pelo menos 11 \
                 observações de cada lado."
            }
            "A die is rolled 25 times and the scores are compared with 3.5: 4 and 6 are \
             above, 3, 2 and 1 below. The sequence has 10 runs against 13.32 expected, \
             Z = −1.38 and p = 0.168, so there is no evidence against randomness. An \
             alternating 1, 10, 1, 10 sequence gives 20 runs and a very small p-value." => {
                "Um dado é lançado 25 vezes e os resultados são comparados com 3,5: 4 e 6 \
                 ficam acima, 3, 2 e 1 abaixo. A sequência tem 10 runs contra 13,32 esperados, \
                 Z = −1,38 e p = 0,168, pelo que não há evidência contra a aleatoriedade. Uma \
                 sequência alternada 1, 10, 1, 10 dá 20 runs e um p-valor muito pequeno."
            }
            "1-Sample Z" => "Z de 1 amostra",
            "1-Sample Z…" => "Z de 1 amostra…",
            "1-Sample Poisson Rate" => "Taxa de Poisson de 1 amostra",
            "1-Sample Poisson Rate…" => "Taxa de Poisson de 1 amostra…",
            "2-Sample Poisson Rate" => "Taxa de Poisson de 2 amostras",
            "2-Sample Poisson Rate…" => "Taxa de Poisson de 2 amostras…",
            "Poisson Goodness-of-Fit" => "Ajuste à distribuição de Poisson",
            "Poisson Goodness-of-Fit…" => "Ajuste à distribuição de Poisson…",
            "Z and Poisson Tests help" => "Ajuda dos testes Z e de Poisson",
            "About these tests…" => "Acerca destes testes…",
            "The 1-sample Z test compares a mean with a known standard deviation. The \
             Poisson rate tests compare occurrence rates with a null rate or between two \
             samples. The goodness-of-fit test compares integer counts with a Poisson \
             distribution." => {
                "O teste Z de 1 amostra compara uma média com um desvio padrão conhecido. Os \
                 testes de taxa de Poisson comparam taxas de ocorrência com uma taxa nula ou \
                 entre duas amostras. O teste de ajuste compara contagens inteiras com uma \
                 distribuição de Poisson."
            }
            "One numeric column: measurements for the Z test, non-negative integer counts \
             for the Poisson tests. The rate tests need the total length of observation; \
             the two-sample test also needs a second counts column and its length. The \
             goodness-of-fit test takes an optional Poisson mean and estimates it from the \
             counts when it is blank. Missing values are excluded and counted." => {
                "Uma coluna numérica: medições para o teste Z, contagens inteiras não negativas \
                 para os testes de Poisson. Os testes de taxa precisam do comprimento total de \
                 observação; o teste de duas amostras precisa também de uma segunda coluna de \
                 contagens e do respectivo comprimento. O teste de ajuste aceita uma média de \
                 Poisson opcional e estima-a a partir das contagens quando está em branco. Os \
                 valores em falta são excluídos e contados."
            }
            "Examples" => "Exemplos",
            "Overview, data and examples" => "Visão geral, dados e exemplos",
            "1-Sample Z: weights with a known sigma of 2 tested against a mean of 10." => {
                "Z de 1 amostra: pesos com sigma conhecido de 2 testados contra uma média de 10."
            }
            "1-Sample Poisson Rate: 12 defects over a length of 4 tested against a rate \
             of 2." => {
                "Taxa de Poisson de 1 amostra: 12 defeitos num comprimento de 4 testados contra \
                 uma taxa de 2."
            }
            "2-Sample Poisson Rate: 12 occurrences in 3 units compared with 5 in 3 units." => {
                "Taxa de Poisson de 2 amostras: 12 ocorrências em 3 unidades comparadas com 5 em \
                 3 unidades."
            }
            "Poisson Goodness-of-Fit: 35 counts with an estimated mean of 2.4 give a \
             chi-square of 1.916 and p = 0.590." => {
                "Ajuste à distribuição de Poisson: 35 contagens com média estimada de 2,4 dão um \
                 qui-quadrado de 1,916 e p = 0,590."
            }
            "Title, subtitle, axis labels and footer" => {
                "Título, subtítulo, rótulos dos eixos e rodapé"
            }
            "Typeface and size of every text on the graph" => {
                "Tipo de letra e tamanho de todos os textos do gráfico"
            }
            "Show or hide symbols, lines and bars" => {
                "Mostrar ou ocultar símbolos, linhas e barras"
            }
            "Place a free note inside the plot" => {
                "Colocar uma nota livre dentro da área do gráfico"
            }
            "Duplicate, rotate, delete or reorder notes" => {
                "Duplicar, rodar, eliminar ou reordenar notas"
            }
            "Label each point with its value or number" => {
                "Rotular cada ponto com o valor ou o número"
            }
            "Vertical or horizontal line with a label" => "Linha vertical ou horizontal com rótulo",
            "Lines at chosen percentiles of the points" => {
                "Linhas nos percentis escolhidos dos pontos"
            }
            "Columns used by a multi-panel graph" => {
                "Colunas usadas por um gráfico com vários painéis"
            }
            "Automatic or manual X and Y limits" => "Limites X e Y automáticos ou manuais",
            "Show or hide the grid" => "Mostrar ou ocultar a grelha",
            "Show or hide the statistics block" => "Mostrar ou ocultar o bloco de estatísticas",
            "Save the graph as SVG" => "Guardar o gráfico em SVG",
            "Save a PNG image" => "Guardar uma imagem PNG",
            "Save a JPEG image" => "Guardar uma imagem JPEG",
            "Write a one-page PDF" => "Escrever um PDF de uma página",
            "Copy the SVG to the clipboard" => "Copiar o SVG para a área de transferência",
            "Remove the Ctrl+click marks" => "Remover as marcas de Ctrl+clique",
            "Restore the automatic labels and remove the edits" => {
                "Repor os rótulos automáticos e remover as edições"
            }
            "A−" => "A−",
            "A+" => "A+",
            "Smaller text" => "Texto mais pequeno",
            "Larger text" => "Texto maior",
            "Sample projects…" => "Projectos de exemplo…",
            "Samples" => "Exemplos",
            "Each sample opens a worksheet and runs the analysis, ready to explore." => {
                "Cada exemplo abre uma folha e executa a análise, pronta a explorar."
            }
            "Capability" => "Capacidade",
            "Measurement systems" => "Sistemas de medição",
            "Design of experiments" => "Planeamento de experiências",
            "Control charts" => "Cartas de controlo",
            "Graphs" => "Gráficos",
            "Quality tools" => "Ferramentas da qualidade",
            "Time series" => "Séries temporais",
            "Weibull capability (PPM)" => "Capacidade Weibull (PPM)",
            "Response surface (CCD)" => "Superfície de resposta (CCD)",
            "I-MR control chart" => "Carta de controlo I-MR",
            "X-bar - R chart (subgroups of 5)" => "Carta X-barra - R (subgrupos de 5)",
            "Attribute P chart" => "Carta P de atributos",
            "Two probability plots" => "Dois gráficos de probabilidade",
            "Three probability plots" => "Três gráficos de probabilidade",
            "Matrix plot (3 variables)" => "Matriz de gráficos (3 variáveis)",
            "Scatter with fit" => "Dispersão com ajuste",
            "Histogram of diameters" => "Histograma de diâmetros",
            "Pareto of defects" => "Pareto de defeitos",
            "Regression / calibration" => "Regressão / calibração",
            "Kaplan-Meier reliability" => "Fiabilidade Kaplan-Meier",
            "Time series decomposition" => "Decomposição de séries temporais",
            "Cpk/Ppk of 120 diameters, LSL 24.7 and USL 25.3." => {
                "Cpk/Ppk de 120 diâmetros, LSL 24,7 e USL 25,3."
            }
            "Positive lifetimes with LSL 4 and USL 12, fitted by Weibull." => {
                "Tempos de vida positivos com LSL 4 e USL 12, ajustados por Weibull."
            }
            "Crossed study: 10 parts, 3 operators, 3 trials." => {
                "Estudo cruzado: 10 peças, 3 operadores, 3 ensaios."
            }
            "Two-factor screening design with a real interaction." => {
                "Plano de triagem de dois fatores com interação real."
            }
            "Central composite design with a quadratic response." => {
                "Plano composto central com resposta quadrática."
            }
            "40 measurements with a small process shift." => {
                "40 medições com um pequeno desvio do processo."
            }
            "30 subgroups of five measurements." => "30 subgrupos de cinco medições.",
            "Defect counts from 25 samples of 100 units." => {
                "Contagens de defeitos em 25 amostras de 100 unidades."
            }
            "Two columns on separate probability panels." => {
                "Duas colunas em painéis de probabilidade separados."
            }
            "Three columns on separate probability panels." => {
                "Três colunas em painéis de probabilidade separados."
            }
            "Pairwise scatterplots with histograms on the diagonal." => {
                "Dispersões duas a duas com histogramas na diagonal."
            }
            "Correlation with a least-squares line." => "Correlação com reta de mínimos quadrados.",
            "Frequency histogram with a normal overlay." => {
                "Histograma de frequências com curva normal sobreposta."
            }
            "Defect categories sorted by frequency." => {
                "Categorias de defeitos ordenadas por frequência."
            }
            "Calibration line with measurement noise." => {
                "Reta de calibração com ruído de medição."
            }
            "Right-censored survival times." => "Tempos de sobrevivência com censura à direita.",
            "36 months with trend and seasonality." => "36 meses com tendência e sazonalidade.",
            // Labels used by the dialogs and the graph editor.
            "1 Proportion…" => "1 Proporção…",
            "1 Variance…" => "1 Variância…",
            "2 Proportions…" => "2 Proporções…",
            "2 Variances…" => "2 Variâncias…",
            "2-Level Factorial" => "Fatorial de 2 níveis",
            "2-Level Factorial…" => "Fatorial de 2 níveis…",
            "Add Date Column" => "Adicionar coluna de data",
            "Add Days" => "Adicionar dias",
            "Add Days…" => "Adicionar dias…",
            "Add Numeric Column" => "Adicionar coluna numérica",
            "Add Row" => "Adicionar linha",
            "Add Text Column" => "Adicionar coluna de texto",
            "Attribute Control Chart" => "Carta de controlo para atributos",
            "Bartlett Test…" => "Teste de Bartlett…",
            "Boxplot" => "Diagrama de caixa",
            "Browse…" => "Procurar…",
            "CSV Options" => "Opções CSV",
            "Cancel" => "Cancelar",
            "Chi-Square Goodness-of-Fit…" => "Ajustamento do qui-quadrado…",
            "Choose Worksheet" => "Escolher folha",
            "Clear Session" => "Limpar sessão",
            "Column:" => "Coluna:",
            "Create 2-Level Design" => "Criar plano de 2 níveis",
            "Create 2-Level Design…" => "Criar plano de 2 níveis…",
            "Date Difference" => "Diferença de datas",
            "Date Difference…" => "Diferença de datas…",
            "Date Format" => "Formato de data",
            "Delete" => "Eliminar",
            "Delete Row" => "Eliminar linha",
            "Descriptive Statistics by Column" => "Estatísticas descritivas por coluna",
            "Display Descriptive Statistics" => "Mostrar estatísticas descritivas",
            "Display Descriptive Statistics by…" => "Mostrar estatísticas descritivas por…",
            "EWMA…" => "EWMA…",
            "Exponential (censored)…" => "Exponencial (censurada)…",
            "Exponential Reliability" => "Fiabilidade exponencial",
            "Find and Replace" => "Localizar e substituir",
            "Format Cells" => "Formatar células",
            "Format Cells…" => "Formatar células…",
            "Gage Type 1" => "Gage Tipo 1",
            "Gage Type 1…" => "Gage Tipo 1…",
            "Generate" => "Gerar",
            "Grubbs Test…" => "Teste de Grubbs…",
            "Histogram" => "Histograma",
            "Insert Row Above" => "Inserir linha acima",
            "I–MR…" => "I–MR…",
            "Kaplan–Meier" => "Kaplan–Meier",
            "Kaplan–Meier…" => "Kaplan–Meier…",
            "Levene Test…" => "Teste de Levene…",
            "Lognormal (censored)…" => "Lognormal (censurada)…",
            "Lognormal Reliability" => "Fiabilidade lognormal",
            "No notes yet." => "Ainda sem notas.",
            "OK" => "OK",
            "One-Way…" => "Um fator…",
            "Open" => "Abrir",
            "Regression…" => "Regressão…",
            "Rename" => "Renomear",
            "Rename Column" => "Renomear coluna",
            "Response Optimizer" => "Otimizador de respostas",
            "Response Optimizer…" => "Otimizador de respostas…",
            "Response Surface" => "Superfície de resposta",
            "Response Surface…" => "Superfície de resposta…",
            "Run" => "Executar",
            "Save" => "Guardar",
            "Scatterplot" => "Diagrama de dispersão",
            "Scatterplot…" => "Diagrama de dispersão…",
            "Select First Numeric Column" => "Selecionar a primeira coluna numérica",
            "Sign Test…" => "Teste de sinais…",
            "Step-Stress ALT" => "ALT com tensão por degraus",
            "Step-Stress ALT…" => "ALT com tensão por degraus…",
            "Weibull (censored)…" => "Weibull (censurada)…",
            "Weibull Reliability" => "Fiabilidade Weibull",
            "X̄–R…" => "X̄–R…",
            "X̄–S…" => "X̄–S…",
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

    /// Labels whose English and Portuguese spellings are the same.
    const SAME_IN_BOTH: &[&str] = &[
        "A+",
        "A−",
        "ANOVA",
        "EWMA…",
        "Editor",
        "Friedman…",
        "Gage R&R",
        "Gage R&R…",
        "I–MR…",
        "Kaplan–Meier",
        "Kaplan–Meier…",
        "Kruskal-Wallis…",
        "Mann-Whitney…",
        "OK",
        "Português",
        "Tour",
        "X̄–R…",
        "X̄–S…",
        "Zoom",
    ];

    fn source_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("source directory") {
            let path = entry.expect("source entry").path();
            if path.is_dir() {
                source_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    fn labels_in(text: &str, labels: &mut Vec<String>) {
        let mut rest = text;
        while let Some(position) = rest.find(".label(") {
            let before = rest[..position].trim_end();
            if !(before.ends_with("catalog()") || before.ends_with("catalog")) {
                rest = &rest[position + ".label(".len()..];
                continue;
            }
            let after = &rest[position + ".label(".len()..];
            let trimmed = after.trim_start();
            if let Some(body) = trimmed.strip_prefix('"') {
                let mut value = String::new();
                let mut chars = body.chars();
                while let Some(character) = chars.next() {
                    match character {
                        '"' => break,
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                // A backslash-newline continues the string and
                                // drops the following indentation.
                                if escaped.is_whitespace() {
                                    while chars.clone().next().is_some_and(char::is_whitespace) {
                                        chars.next();
                                    }
                                } else {
                                    value.push(escaped);
                                }
                            }
                        }
                        _ => value.push(character),
                    }
                }
                labels.push(value);
            }
            rest = &rest[position + ".label(".len()..];
        }
    }

    #[test]
    fn every_interface_label_has_a_portuguese_entry() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        source_files(&root, &mut files);
        let catalog = UiCatalog::new(Lang::Portuguese);
        let mut missing = Vec::new();
        for path in files {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if name == "catalog.rs" || path.ends_with("app/tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("source file");
            let mut labels = Vec::new();
            labels_in(&text, &mut labels);
            for label in labels {
                if SAME_IN_BOTH.contains(&label.as_str()) {
                    continue;
                }
                if catalog.label(&label) == label {
                    missing.push(format!("{}: {label}", path.display()));
                }
            }
        }
        assert!(missing.is_empty(), "missing translations: {missing:#?}");
    }
}
