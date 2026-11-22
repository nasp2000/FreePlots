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
            "Nested ANOVA…" => "ANOVA encaixada…",
            "Nested ANOVA" => "ANOVA encaixada",
            "Survival Analysis…" => "Análise de sobrevivência…",
            "Survival Analysis" => "Análise de sobrevivência",
            "Survival analysis help" => "Ajuda da análise de sobrevivência",
            "About survival analysis…" => "Acerca da análise de sobrevivência…",
            "Time column:" => "Coluna de tempo:",
            "Censoring column:" => "Coluna de censura:",
            "Censoring value:" => "Valor de censura:",
            "Mission time:" => "Tempo de missão:",
            "None (all events)" => "Nenhuma (todas as falhas)",
            "The censoring value marks right-censored observations; the mission time \
             gives the reliability and the hazard of every fitted model." => {
                "O valor de censura marca observações censuradas à direita; o tempo de missão \
                 dá a fiabilidade e a taxa de falha de cada modelo ajustado."
            }
            "The survival analysis fits the exponential, Weibull and lognormal life \
             models to right-censored data and compares them by the maximized \
             log-likelihood, the AIC and the BIC. It also gives the percentile lives \
             B1, B5, B10 and B50 of every model and the reliability and hazard at a \
             mission time, and draws the fitted curves over the Kaplan–Meier estimate." => {
                "A análise de sobrevivência ajusta os modelos de vida exponencial, Weibull e \
                 lognormal a dados censurados à direita e compara-os pela verosimilhança \
                 maximizada, pelo AIC e pelo BIC. Dá também os percentis de vida B1, B5, B10 e \
                 B50 de cada modelo e a fiabilidade e a taxa de falha num tempo de missão, e \
                 desenha as curvas ajustadas sobre a estimativa de Kaplan–Meier."
            }
            "Choose the numeric time column, an optional censoring column and the \
             censoring value: an observation whose censoring column equals that value \
             is treated as right-censored, every other one as a failure. Without a \
             censoring column every observation is a failure. Times must be positive \
             and at least one failure is needed. The mission time must be positive; \
             the dialog starts it at the median of the times." => {
                "Escolha a coluna numérica do tempo, uma coluna de censura opcional e o valor \
                 de censura: uma observação cuja coluna de censura seja igual a esse valor é \
                 tratada como censurada à direita e todas as outras como falhas. Sem coluna de \
                 censura todas as observações são falhas. Os tempos têm de ser positivos e é \
                 precisa pelo menos uma falha. O tempo de missão tem de ser positivo; o \
                 diálogo começa-o na mediana dos tempos."
            }
            "Twelve times with the last three censored give an exponential mean of \
             82/9; its B10 is about 0.96, its B50 about 6.32 and its reliability at \
             5 is about 0.58. The AIC and the BIC order the three models, and the \
             smallest value is the best compromise between fit and complexity." => {
                "Doze tempos com os últimos três censurados dão uma média exponencial de \
                 82/9; o B10 é cerca de 0,96, o B50 cerca de 6,32 e a fiabilidade em 5 cerca de \
                 0,58. O AIC e o BIC ordenam os três modelos, e o menor valor é o melhor \
                 compromisso entre ajuste e complexidade."
            }
            "CART and Random Forest…" => "CART e floresta aleatória…",
            "CART and Random Forest" => "CART e floresta aleatória",
            "Tree models help" => "Ajuda dos modelos de árvore",
            "About tree models…" => "Acerca dos modelos de árvore…",
            "Random forest" => "Floresta aleatória",
            "Predictors:" => "Preditores:",
            "Maximum depth:" => "Profundidade máxima:",
            "Minimum leaf:" => "Folha mínima:",
            "Trees:" => "Árvores:",
            "Seed:" => "Semente:",
            "Numeric predictors only; a text response becomes a classification tree and \
             a numeric one a regression tree." => {
                "Apenas preditores numéricos; uma resposta de texto torna-se uma árvore de \
                 classificação e uma numérica uma árvore de regressão."
            }
            "CART grows one binary tree: every node splits a numeric predictor at the \
             midpoint that most reduces the impurity — the within-node sum of squares \
             for a numeric response or the Gini index for a categorical one. The random \
             forest grows many trees on bootstrap samples, each split choosing from a \
             random subset of the predictors, and reports the out-of-bag error and the \
             variable importance." => {
                "O CART cresce uma árvore binária: cada nó divide um preditor numérico no \
                 ponto médio que mais reduz a impureza — a soma de quadrados dentro do nó para \
                 uma resposta numérica ou o índice de Gini para uma categórica. A floresta \
                 aleatória cresce muitas árvores em amostras bootstrap, com cada divisão a \
                 escolher um subconjunto aleatório dos preditores, e reporta o erro out-of-bag \
                 e a importância das variáveis."
            }
            "Choose the response and at least one numeric predictor. A numeric \
             response gives a regression tree (leaves are means); a text response \
             gives a classification tree (leaves are the majority class, with a \
             confusion matrix). The growth stops at the maximum depth, at the minimum \
             number of observations per leaf and when no split reduces the impurity. \
             The forest uses the square root of the number of predictors per split and \
             the seed makes the run reproducible. For CART, the validation folds give a \
             k-fold cross-validated R squared and RMSE (or accuracy); the forest reports \
             the out-of-bag error." => {
                "Escolha a resposta e pelo menos um preditor numérico. Uma resposta numérica \
                 dá uma árvore de regressão (as folhas são médias); uma resposta de texto dá \
                 uma árvore de classificação (as folhas são a classe maioritária, com matriz de \
                 confusão). O crescimento para na profundidade máxima, no número mínimo de \
                 observações por folha e quando nenhuma divisão reduz a impureza. A floresta usa \
                 a raiz quadrada do número de preditores por divisão e a semente torna a \
                 execução reprodutível. Para o CART, as dobras de validação dão o R quadrado \
                 e o RMSE (ou a exactidão) por validação cruzada; a floresta reporta o erro \
                 out-of-bag."
            }
            "A response that jumps from 10 to 20 at the middle of the predictor gives a \
             two-leaf tree whose only split is on that predictor, with R² near one. A \
             forest on a response driven by one predictor and a noise predictor ranks \
             the first far above the second and reports the out-of-bag error." => {
                "Uma resposta que salta de 10 para 20 a meio do preditor dá uma árvore de duas \
                 folhas cuja única divisão é nesse preditor, com R² próximo de um. Uma floresta \
                 sobre uma resposta explicada por um preditor e um preditor de ruído coloca o \
                 primeiro muito acima do segundo e reporta o erro out-of-bag."
            }
            "Nonlinear Regression…" => "Regressão não linear…",
            "Nonlinear Regression" => "Regressão não linear",
            "Nonlinear regression help" => "Ajuda da regressão não linear",
            "About nonlinear regression…" => "Acerca da regressão não linear…",
            "Predictor:" => "Preditor:",
            "Asymptotic" => "Assintótica",
            "Logistic (3 parameters)" => "Logística (3 parâmetros)",
            "Weibull growth" => "Crescimento de Weibull",
            "Power" => "Potência",
            "Nonlinear regression fits a curve that is not a straight line in its \
             parameters: the engine minimises the residual sum of squares with \
             Levenberg–Marquardt and reports the estimates, their standard errors and \
             the fitted curve over the observations." => {
                "A regressão não linear ajusta uma curva que não é linear nos parâmetros: o \
                 motor minimiza a soma de quadrados dos resíduos com Levenberg–Marquardt e \
                 reporta as estimativas, os respetivos erros padrão e a curva ajustada sobre \
                 as observações."
            }
            "Choose a numeric response, a numeric predictor and one of the models: \
             Michaelis–Menten (a saturating hyperbola), asymptotic regression, a \
             three-parameter logistic, Gompertz, Weibull growth and a power curve. The \
             equation of the chosen model is written above the estimates. The power \
             model needs positive predictor values and the Weibull model non-negative \
             ones. The starting values come from the range of the data and the iteration \
             stops when the parameters move by less than 1e-9 relative or the sum of \
             squares by less than 1e-12." => {
                "Escolha uma resposta numérica, um preditor numérico e um dos modelos: \
                 Michaelis–Menten (uma hipérbole saturante), regressão assintótica, logística \
                 de três parâmetros, Gompertz, crescimento de Weibull e uma curva de potência. \
                 A equação do modelo escolhido é escrita acima das estimativas. O modelo de \
                 potência exige preditores positivos e o de Weibull não negativos. Os valores \
                 iniciais vêm do intervalo dos dados e a iteração para quando os parâmetros \
                 variam menos de 1e-9 em termos relativos ou a soma de quadrados menos de \
                 1e-12."
            }
            "Enzyme rates against substrate concentration follow Michaelis–Menten: \
             twelve observations give V about 212.7 and K about 0.064, with R² near \
             0.96. A curve with no noise is recovered to the third digit, and a model \
             outside the range of the data warns that the iteration did not converge." => {
                "As velocidades enzimáticas contra a concentração de substrato seguem \
                 Michaelis–Menten: doze observações dão V cerca de 212,7 e K cerca de 0,064, com \
                 R² próximo de 0,96. Uma curva sem ruído é recuperada até ao terceiro dígito, e \
                 um modelo fora do intervalo dos dados avisa que a iteração não convergiu."
            }
            "MANOVA help" => "Ajuda do MANOVA",
            "Nested ANOVA help" => "Ajuda da ANOVA encaixada",
            "About MANOVA…" => "Acerca do MANOVA…",
            "About nested ANOVA…" => "Acerca da ANOVA encaixada…",
            "Responses:" => "Respostas:",
            "Group:" => "Grupo:",
            "Subgroup:" => "Subgrupo:",
            "One factor and at least two numeric responses; the four statistics come \
             with Rao's approximate F tests." => {
                "Um factor e pelo menos duas respostas numéricas; as quatro estatísticas vêm \
                 com os testes F aproximados de Rao."
            }
            "Two levels in a balanced design: the same number of subgroups per group \
             and of observations per cell; the components use the random-effects \
             estimators." => {
                "Dois níveis num desenho equilibrado: o mesmo número de subgrupos por grupo \
                 e de observações por célula; os componentes usam os estimadores de efeitos \
                 aleatórios."
            }
            "The multivariate analysis of variance compares the means of several \
             response variables across the levels of one factor. The hypothesis and \
             error matrices give the four usual statistics — Wilks' lambda, Pillai's \
             trace, the Hotelling–Lawley trace and Roy's greatest root — each with an \
             approximate F test, so the report shows whether the groups differ on the \
             responses taken together." => {
                "A análise de variância multivariada compara as médias de várias variáveis \
                 de resposta entre os níveis de um factor. As matrizes de hipótese e de erro \
                 dão as quatro estatísticas usuais — lambda de Wilks, traço de Pillai, traço de \
                 Hotelling–Lawley e raiz maior de Roy — cada uma com um teste F aproximado, pelo \
                 que o relatório mostra se os grupos diferem nas respostas em conjunto."
            }
            "Choose one factor and at least two numeric response variables. The \
             hypothesis matrix H uses the g − 1 degrees of freedom between the groups \
             and the error matrix E uses n − g; the eigenvalues of H(E + H)⁻¹ are \
             computed from the Cholesky factor of E. Rao's F has s = min(p, q), \
             m = (|p − q| − 1)/2 and n = (v − p − 1)/2 degrees of freedom in the usual \
             parameterisation, exact with two groups and approximate otherwise." => {
                "Escolha um factor e pelo menos duas variáveis de resposta numéricas. A \
                 matriz de hipótese H usa os g − 1 graus de liberdade entre grupos e a matriz \
                 de erro E usa n − g; os valores próprios de H(E + H)⁻¹ vêm do factor de \
                 Cholesky de E. O F de Rao usa s = min(p, q), m = (|p − q| − 1)/2 e \
                 n = (v − p − 1)/2 graus de liberdade na parametrização usual, exacto com dois \
                 grupos e aproximado nos restantes casos."
            }
            "Two responses with three groups of two observations, separated along both \
             responses, give a small Wilks' lambda and a large Pillai trace; the four \
             tests agree. A single response belongs to the one-way ANOVA instead, and a \
             factor with one level cannot be tested." => {
                "Duas respostas com três grupos de duas observações, separados nas duas \
                 respostas, dão um lambda de Wilks pequeno e um traço de Pillai grande; os \
                 quatro testes concordam. Uma única resposta pertence à ANOVA a um factor, e um \
                 factor com um nível não pode ser testado."
            }
            "In a nested design every subgroup belongs to one group: the model is \
             y = µ + group + subgroup(group) + error. The analysis splits the variation \
             into the group term, the subgroup term inside the groups and the error, and \
             the F tests are hierarchical — the group term is tested against the \
             subgroup mean square and the subgroup term against the error." => {
                "Num desenho encaixado cada subgrupo pertence a um grupo: o modelo é \
                 y = µ + grupo + subgrupo(grupo) + erro. A análise reparte a variação entre o \
                 termo de grupo, o termo de subgrupo dentro dos grupos e o erro, e os testes F \
                 são hierárquicos — o termo de grupo é testado contra o quadrado médio do \
                 subgrupo e o termo de subgrupo contra o erro."
            }
            "Choose a numeric response, the group column and the subgroup column. The \
             design must be balanced: the same number of subgroups per group and the \
             same number of observations per cell. The variance components come from \
             the random-effects estimators σ²(error) = MS_error, σ²(subgroup) = \
             (MS_subgroup − MS_error)/n and σ²(group) = (MS_group − MS_subgroup)/(b·n); \
             a negative estimate is reported as zero." => {
                "Escolha uma resposta numérica, a coluna do grupo e a do subgrupo. O desenho \
                 tem de ser equilibrado: o mesmo número de subgrupos por grupo e de observações \
                 por célula. Os componentes de variância vêm dos estimadores de efeitos \
                 aleatórios σ²(erro) = MS_erro, σ²(subgrupo) = (MS_subgrupo − MS_erro)/n e \
                 σ²(grupo) = (MS_grupo − MS_subgrupo)/(b·n); uma estimativa negativa é \
                 reportada como zero."
            }
            "Two groups, two subgroups each and two observations per cell, with cell \
             means 4, 6, 10 and 12 and a unit spread inside each cell, give a group sum \
             of squares of 72, a subgroup sum of squares of 8 and an error sum of \
             squares of 8; the components are 17, 1 and 2, so the groups explain most \
             of the variance." => {
                "Dois grupos, dois subgrupos cada e duas observações por célula, com médias \
                 de célula 4, 6, 10 e 12 e um desvio unitário dentro de cada célula, dão uma \
                 soma de quadrados de grupo de 72, de subgrupo de 8 e de erro de 8; os \
                 componentes são 17, 1 e 2, pelo que os grupos explicam a maior parte da \
                 variância."
            }
            "2×2 Crossover Equivalence…" => "Equivalência cruzada 2×2…",
            "2×2 Crossover Equivalence" => "Equivalência cruzada 2×2",
            "2×2 crossover equivalence help" => "Ajuda da equivalência cruzada 2×2",
            "About 2×2 crossover equivalence…" => "Acerca da equivalência cruzada 2×2…",
            "Subject:" => "Sujeito:",
            "Sequence:" => "Sequência:",
            "Period:" => "Período:",
            "Treatment:" => "Tratamento:",
            "Lower limit:" => "Limite inferior:",
            "Upper limit:" => "Limite superior:",
            "Confidence:" => "Confiança:",
            "One row per observation; the two sequences run the treatments in opposite \
             orders and the TOST decides equivalence inside the limits." => {
                "Uma linha por observação; as duas sequências aplicam os tratamentos em ordem \
                 oposta e o TOST decide a equivalência dentro dos limites."
            }
            "A 2×2 crossover gives every subject both treatments, one in each period, \
             and the sequences run them in opposite orders. The within-subject \
             difference removes the subject effect; its mean over the two sequences \
             estimates the treatment difference, and the two one-sided tests decide \
             whether that difference is inside the equivalence limits. The analysis of \
             variance shows the sequence (the differential carryover), the period and \
             the treatment rows." => {
                "Um cruzamento 2×2 dá os dois tratamentos a cada sujeito, um em cada período, e \
                 as sequências aplicam-nos em ordem oposta. A diferença intra-sujeito remove o \
                 efeito do sujeito; a sua média nas duas sequências estima a diferença entre \
                 tratamentos, e os dois testes unilaterais decidem se essa diferença está dentro \
                 dos limites de equivalência. A análise de variância mostra as linhas da \
                 sequência (o carryover diferencial), do período e do tratamento."
            }
            "Use one row per observation with the response, the subject, the sequence, \
             the period and the treatment. Every subject needs both periods with the two \
             treatments in the order of its sequence, and the two sequences must be \
             opposite. The reference is the alphabetically first treatment unless you \
             choose another in Reference, and the difference is test − reference. The \
             limits may be asymmetric; the confidence level of the decision reports the \
             (1 − 2α) interval, so 95 % gives the 90 % interval." => {
                "Use uma linha por observação com a resposta, o sujeito, a sequência, o período \
                 e o tratamento. Cada sujeito precisa dos dois períodos com os dois tratamentos \
                 na ordem da sua sequência, e as duas sequências têm de ser opostas. A referência \
                 é a primeira etiqueta por ordem alfabética, salvo se escolher outra em \
                 Referência, e a diferença é teste − referência. Os limites podem ser \
                 assimétricos; o nível de confiança da decisão reporta o intervalo (1 − 2α), \
                 pelo que 95 % dá o intervalo a 90 %."
            }
            "Eight subjects, four in each sequence, with a period effect of one and a \
             treatment difference of 0.2, give an estimate of 0.2 and a period row that \
             is significant. A carryover that delays the second period of one sequence \
             inflates the estimate and shows up in the sequence row, because the two \
             periods cannot separate it from the treatment × period interaction." => {
                "Oito sujeitos, quatro em cada sequência, com um efeito de período de um e uma \
                 diferença de tratamentos de 0,2, dão uma estimativa de 0,2 e uma linha de \
                 período significativa. Um carryover que atrasa o segundo período de uma \
                 sequência inflaciona a estimativa e aparece na linha da sequência, porque os \
                 dois períodos não o separam da interacção tratamento × período."
            }
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
            "Analysis of Means…" => "Análise de médias…",
            "Analysis of Means" => "Análise de médias",
            "Analysis of means help" => "Ajuda da análise de médias",
            "About analysis of means…" => "Acerca da análise de médias…",
            "The analysis of means compares every level mean with the weighted grand \
             mean through simultaneous decision limits. A level outside the limits is \
             a real difference from the overall mean; the limits hold together at the \
             stated alpha, so the chart answers the same question as the F test and \
             shows which levels differ." => {
                "A análise de médias compara cada média de nível com a média global ponderada \
                 através de limites de decisão simultâneos. Um nível fora dos limites é uma \
                 diferença real face à média geral; os limites valem em conjunto para o alfa \
                 indicado, pelo que o gráfico responde à mesma pergunta do teste F e mostra \
                 quais os níveis que diferem."
            }
            "Choose a numeric response and a factor with at least two levels of two or \
             more observations. The limits are the weighted grand mean plus or minus \
             h times the standard error √(MSE·(N − nᵢ)/(N·nᵢ)); with equal sizes \
             that is √(MSE·(k − 1)/N). The critical value h is the studentized \
             maximum modulus quantile for k levels and N − k degrees of freedom, \
             computed by numerical integration of its distribution." => {
                "Escolha uma resposta numérica e um factor com pelo menos dois níveis de dois \
                 ou mais valores. Os limites são a média global ponderada mais ou menos h \
                 vezes o erro padrão √(MSE·(N − nᵢ)/(N·nᵢ)); com tamanhos iguais é \
                 √(MSE·(k − 1)/N). O valor crítico h é o quantil da studentized maximum \
                 modulus para k níveis e N − k graus de liberdade, calculado por integração \
                 numérica da sua distribuição."
            }
            "Three levels with three observations each, means 11, 14 and 21 and MSE \
             one, at five percent: h is about 3.19 and the limits are 15.33 ± 1.51. \
             The level with mean 21 lies above the upper limit and the level with mean \
             11 below the lower limit, so both differ from the grand mean." => {
                "Três níveis com três observações cada, médias 11, 14 e 21 e MSE igual a um, \
                 a cinco por cento: h é cerca de 3,19 e os limites são 15,33 ± 1,51. O nível \
                 com média 21 fica acima do limite superior e o nível com média 11 abaixo do \
                 limite inferior, pelo que ambos diferem da média global."
            }
            "One-Way ANOVA…" => "ANOVA de um factor…",
            "Power and Sample Size…" => "Potência e tamanho amostral…",
            // Quality tools.
            "Capability Analysis…" => "Análise de capacidade…",
            "Distribution Comparison…" => "Comparação de distribuições…",
            "Variables Acceptance Sampling…" => "Amostragem de aceitação por variáveis…",
            "Variables Acceptance Sampling" => "Amostragem de aceitação por variáveis",
            "Variables sampling help" => "Ajuda da amostragem por variáveis",
            "About variables sampling…" => "Acerca da amostragem por variáveis…",
            "Variables acceptance sampling inspects a measured characteristic instead of \
             counting defectives. With one specification limit and a known sigma it \
             derives the sample size n and the critical distance k from the acceptable \
             and rejectable quality levels and the two risks, draws the operating \
             characteristic curve and accepts or rejects a sample." => {
                "A amostragem de aceitação por variáveis inspecciona uma característica medida em                  vez de contar defeituosos. Com um limite de especificação e sigma conhecido,                  deriva o tamanho da amostra n e a distância crítica k dos níveis de qualidade                  aceitável e rejeitável e dos dois riscos, desenha a curva característica de                  operação e aceita ou rejeita uma amostra."
            }
            "Enter the specification limit, the known sigma, the AQL, the RQL, the \
             producer risk and the consumer risk. Optionally choose a numeric column: the \
             first n observations are averaged and the lot is accepted when the mean is at \
             least k standard deviations from the limit. Compare plans can be written as \
             n:k pairs." => {
                "Introduza o limite de especificação, o sigma conhecido, o AQL, o RQL, o risco do                  produtor e o risco do consumidor. Opcionalmente escolha uma coluna numérica: as                  primeiras n observações são mediadas e o lote é aceite quando a média está a pelo                  menos k desvios padrão do limite. Os planos de comparação podem ser escritos como                  pares n:k."
            }
            "An upper limit of 10, sigma 1, AQL 0.01, RQL 0.05, producer risk 0.05 and \
             consumer risk 0.10 give n = 19 and k = 1.949; a sample mean of 8 is accepted \
             and 9.8 is rejected. A 40-item plan with k = 2.1 is steeper and accepts less \
             at the RQL." => {
                "Um limite superior de 10, sigma 1, AQL 0,01, RQL 0,05, risco do produtor 0,05 e                  risco do consumidor 0,10 dão n = 19 e k = 1,949; uma média de 8 é aceite e 9,8 é                  rejeitada. Um plano de 40 itens com k = 2,1 é mais inclinado e aceita menos no                  RQL."
            }
            "Individual Distribution Identification" => "Identificação individual de distribuições",
            "Individual Distribution Identification…" => {
                "Identificação individual de distribuições…"
            }
            "Individual distribution identification help" => {
                "Ajuda da identificação individual de distribuições"
            }
            "About individual distribution identification…" => {
                "Acerca da identificação individual de distribuições…"
            }
            "Individual distribution identification compares fourteen distributions on \
             the same sample, with the Anderson–Darling test, AIC/BIC and one probability \
             plot per family. Box-Cox and Johnson transformations can be stored as new \
             worksheet columns for later analyses." => {
                "A identificação individual de distribuições compara catorze distribuições na \
                 mesma amostra, com o teste de Anderson–Darling, AIC/BIC e um gráfico de \
                 probabilidade por família. As transformações Box-Cox e Johnson podem ser \
                 guardadas como novas colunas da folha para análises posteriores."
            }
            "Choose one numeric column; missing values are excluded and counted. Normal, \
             logistic, extreme value and smallest extreme value accept any data; the other \
             families need positive values. The Box-Cox and Johnson columns are written \
             with the names in the dialog, and rows with a missing value stay missing." => {
                "Escolha uma coluna numérica; os valores em falta são excluídos e contados. A \
                 normal, a logística, o valor extremo e o valor extremo mínimo aceitam quaisquer \
                 dados; as restantes famílias exigem valores positivos. As colunas Box-Cox e \
                 Johnson são escritas com os nomes indicados no diálogo, e as linhas com valor em \
                 falta continuam em falta."
            }
            "A right-skewed life column is fitted by the Weibull and the lognormal; the \
             table ranks them by AIC and the panels show the probability plot of each \
             family. Turning on Store Box-Cox column writes life_boxcox and turning on \
             Store Johnson column writes life_johnson, both with the fitted score." => {
                "Uma coluna de vida com assimetria à direita é ajustada pelo Weibull e pelo \
                 lognormal; a tabela ordena-os por AIC e os painéis mostram o gráfico de \
                 probabilidade de cada família. Ao activar Store Box-Cox column é escrita \
                 life_boxcox e ao activar Store Johnson column é escrita life_johnson, ambas com \
                 a pontuação ajustada."
            }
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
            "Show the graph" => "Mostrar o gráfico",
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
            "Gage R&R help" => "Ajuda do Gage R&R",
            "About Gage R&R…" => "Acerca do Gage R&R…",
            "The crossed Gage R&R estimates repeatability (the same operator measuring \
             the same part), reproducibility (different operators) and their sum, the \
             Gage R&R variation. It reports the percentages of study variation, the \
             number of distinct categories and, with specification limits, the \
             percentage of tolerance." => {
                "O Gage R&R cruzado estima a repetibilidade (o mesmo operador a medir a mesma \
                 peça), a reprodutibilidade (operadores diferentes) e a sua soma, a variação \
                 Gage R&R. Reporta as percentagens da variação de estudo, o número de \
                 categorias distintas e, com limites de especificação, a percentagem de \
                 tolerância."
            }
            "Choose a numeric measurement and the part and operator columns of a balanced \
             crossed design: every part is measured by every operator with the same \
             number of replicates. Optional lower and upper specification limits add \
             %Tolerance and the published 10 %/30 % acceptance bands (acceptable, \
             marginal, unacceptable), applied to the study variation and to the \
             tolerance." => {
                "Escolha uma medição numérica e as colunas de peça e operador de um \
                 delineamento cruzado equilibrado: cada peça é medida por cada operador com o \
                 mesmo número de repetições. Os limites inferior e superior de especificação, \
                 opcionais, acrescentam %Tolerance e as faixas publicadas de 10 %/30 % \
                 (aceitável, marginal, inaceitável), aplicadas à variação de estudo e à \
                 tolerância."
            }
            "Ten parts measured by three operators, three times each, give the variance \
             components and %Gage R&R; with limits 9.9 and 10.1 the same study also gives \
             %Tolerance. A %Gage R&R of 8 % is acceptable, 20 % is marginal and 35 % is \
             unacceptable under the published guideline." => {
                "Dez peças medidas por três operadores, três vezes cada, dão os componentes de \
                 variância e o %Gage R&R; com limites 9,9 e 10,1 o mesmo estudo dá também \
                 %Tolerance. Um %Gage R&R de 8 % é aceitável, 20 % é marginal e 35 % é \
                 inaceitável segundo a orientação publicada."
            }
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
            "Best Subsets…" => "Melhores subconjuntos…",
            "Partial Least Squares…" => "Mínimos quadrados parciais…",
            "Partial Least Squares" => "Mínimos quadrados parciais",
            "Partial least squares help" => "Ajuda dos mínimos quadrados parciais",
            "About partial least squares…" => "Acerca dos mínimos quadrados parciais…",
            "Partial least squares (PLS1) fits one numeric response from several \
             correlated predictors through a small number of latent components. It is \
             useful when the predictors are many or collinear and an ordinary regression \
             would be unstable." => {
                "Os mínimos quadrados parciais (PLS1) ajustam uma resposta numérica a partir de \
                 vários preditores correlacionados através de um pequeno número de componentes \
                 latentes. São úteis quando os preditores são muitos ou colineares e uma regressão \
                 ordinária seria instável."
            }
            "Choose one numeric response and at least one numeric predictor. Rows with a \
             missing response or predictor are removed listwise; a constant predictor is \
             an error. Set the number of components (at most the number of predictors)." => {
                "Escolha uma resposta numérica e pelo menos um preditor numérico. As linhas com                  resposta ou preditor em falta são removidas em lista; um preditor constante é um                  erro. Defina o número de componentes (no máximo o número de preditores)."
            }
            "With y = 2 + 3·x1 − 1.5·x2 and a third predictor equal to x1 + x2, two \
             components recover the coefficients; one component already predicts most of \
             y through the dominant direction and handles the collinearity." => {
                "Com y = 2 + 3·x1 − 1,5·x2 e um terceiro preditor igual a x1 + x2, dois                  componentes recuperam os coeficientes; um componente já prevê a maior parte de y                  através da direção dominante e trata a colinearidade."
            }
            "Best Subsets Regression" => "Regressão por melhores subconjuntos",
            "Best subsets regression help" => "Ajuda da regressão por melhores subconjuntos",
            "About best subsets…" => "Acerca dos melhores subconjuntos…",
            "Best subsets fits every non-empty subset of the candidate predictors and \
             ranks the models by R², adjusted R², Mallows' Cp and the residual standard \
             error s. It is a screening step before fitting a final regression." => {
                "Os melhores subconjuntos ajustam todos os subconjuntos não vazios dos preditores \
                 candidatos e ordenam os modelos por R², R² ajustado, Cp de Mallows e erro padrão \
                 residual s. É um passo de triagem antes de ajustar a regressão final."
            }
            "Choose one numeric response and at least one numeric predictor, up to 12. \
             Rows with a missing value in the response or in any candidate predictor are \
             removed listwise, so every model uses the same observations. A subset with \
             collinear predictors is skipped; a collinear full model is an error because \
             it is the Cp reference." => {
                "Escolha uma resposta numérica e pelo menos um preditor numérico, até 12. As \
                 linhas com valor em falta na resposta ou em qualquer preditor candidato são \
                 removidas em lista, para que todos os modelos usem as mesmas observações. Um \
                 subconjunto com preditores colineares é ignorado; um modelo completo colinear é \
                 um erro porque é a referência do Cp."
            }
            "With a response built from x1 and x2 and an unrelated x3, the best adjusted \
             R² and the smallest Cp both choose the model with x1 and x2; its Cp is close \
             to 3, the number of parameters. Adding x3 raises R² only slightly and lowers \
             the adjusted R²." => {
                "Com uma resposta construída a partir de x1 e x2 e um x3 não relacionado, o \
                 melhor R² ajustado e o menor Cp escolhem ambos o modelo com x1 e x2; o seu Cp \
                 fica próximo de 3, o número de parâmetros. Acrescentar x3 sobe o R² apenas um \
                 pouco e baixa o R² ajustado."
            }
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
            "Generalized Variance Chart" => "Carta de variância generalizada",
            "Generalized Variance Chart…" => "Carta de variância generalizada…",
            "Multivariate EWMA Chart" => "Carta EWMA multivariada",
            "Multivariate EWMA Chart…" => "Carta EWMA multivariada…",
            "Multivariate charts help" => "Ajuda das cartas multivariadas",
            "Multivariate Charts…" => "Cartas multivariadas…",
            "About multivariate charts…" => "Acerca das cartas multivariadas…",
            "The multivariate charts monitor two or more correlated variables together. \
             Hotelling's T² watches the mean vector, the generalized variance chart \
             watches the joint variability, and the multivariate EWMA is more sensitive \
             to small shifts in the mean." => {
                "As cartas multivariadas monitorizam em conjunto duas ou mais variáveis \
                 correlacionadas. O T² de Hotelling vigia o vector de médias, a carta de \
                 variância generalizada vigia a variabilidade conjunta e a EWMA multivariada é \
                 mais sensível a pequenos desvios da média."
            }
            "Choose two or more numeric columns, separated by commas, and the subgroup \
             size; rows are taken in order and an incomplete last subgroup is dropped. \
             The T² and the multivariate EWMA use the confidence level; the generalized \
             variance chart uses the 3-sigma b1/b2 limits and needs a subgroup larger \
             than the number of variables. The multivariate EWMA also uses λ." => {
                "Escolha duas ou mais colunas numéricas, separadas por vírgulas, e o tamanho do \
                 subgrupo; as linhas são tomadas por ordem e o último subgrupo incompleto é \
                 descartado. O T² e a EWMA multivariada usam o nível de confiança; a carta de \
                 variância generalizada usa os limites b1/b2 a três sigma e exige um subgrupo \
                 maior do que o número de variáveis. A EWMA multivariada usa ainda λ."
            }
            "With two variables and subgroups of three, the T² chart flags a subgroup \
             whose mean vector leaves the F limit; the generalized variance chart flags \
             a subgroup whose covariance determinant leaves the b1/b2 limits; λ = 0.2 \
             smooths the mean vector for the EWMA chart." => {
                "Com duas variáveis e subgrupos de três, a carta T² assinala um subgrupo cujo \
                 vector de médias sai do limite F; a carta de variância generalizada assinala um \
                 subgrupo cujo determinante da covariância sai dos limites b1/b2; λ = 0,2 alisa \
                 o vector de médias da carta EWMA."
            }
            "Acceptance Sampling" => "Amostragem de aceitação",
            "Evaluate a plan" => "Avaliar um plano",
            "Plan a scheme" => "Planear um esquema",
            "Single" => "Simples",
            "Double" => "Dupla",
            "Sample size:" => "Tamanho da amostra:",
            "Acceptance:" => "Aceitação:",
            "First acceptance:" => "Aceitação da primeira amostra:",
            "First rejection:" => "Rejeição da primeira amostra:",
            "Second acceptance:" => "Aceitação da segunda amostra:",
            "Lot size:" => "Tamanho do lote:",
            "AQL %:" => "NQA %:",
            "Producer risk %:" => "Risco do produtor %:",
            "Consumer risk %:" => "Risco do consumidor %:",
            "0 = large lot (binomial)" => "0 = lote grande (binomial)",
            "0 = no risk report" => "0 = sem relatório de riscos",
            "The smallest single-sampling scheme that meets both risks." => {
                "O menor esquema de amostragem simples que cumpre os dois riscos."
            }
            "Accept at c₁, reject at r₁, and use the second sample in between." => {
                "Aceitar em c₁, rejeitar em r₁ e usar a segunda amostra nos casos intermédios."
            }
            "Pareto Chart" => "Gráfico de Pareto",
            "Interval Plot" => "Gráfico de intervalos",
            "Matrix Plot" => "Gráfico de matriz",
            "More Graphs" => "Mais gráficos",
            "Bar Chart…" => "Gráfico de barras…",
            "Line Plot…" => "Gráfico de linhas…",
            "Area Chart…" => "Gráfico de área…",
            "Heatmap…" => "Mapa de calor…",
            "Bubble Plot…" => "Gráfico de bolhas…",
            "Individual Value Plot…" => "Gráfico de valores individuais…",
            "Marginal Plot…" => "Gráfico marginal…",
            "Parallel Coordinates…" => "Coordenadas paralelas…",
            "Correlogram…" => "Correlograma…",
            "Graph Gallery" => "Galeria de gráficos",
            "Graph gallery help" => "Ajuda da galeria de gráficos",
            "About these graphs…" => "Acerca destes gráficos…",
            "The graph gallery collects nine descriptive graphs: bar, line, area, \
             heatmap, bubble, individual value, marginal, parallel coordinates and \
             correlogram. They describe the worksheet; no model is fitted." => {
                "A galeria de gráficos reúne nove gráficos descritivos: barras, linhas, área, \
                 mapa de calor, bolhas, valores individuais, marginal, coordenadas paralelas e \
                 correlograma. Descrevem a folha de cálculo; não é ajustado nenhum modelo."
            }
            "Choose one numeric or text column for the categories, one or more numeric \
             columns, an x column and, for the bubble plot, a size column. Rows with a \
             missing value in the used columns are excluded. The heatmap and bubble plot \
             encode their magnitude in value classes because the drawing primitives use \
             a single fill tone and a fixed marker radius." => {
                "Escolha uma coluna numérica ou de texto para as categorias, uma ou mais colunas \
                 numéricas, uma coluna x e, para o gráfico de bolhas, uma coluna de tamanho. As \
                 linhas com valor em falta nas colunas usadas são excluídas. O mapa de calor e o \
                 gráfico de bolhas codificam a grandeza em classes de valor porque as primitivas \
                 de desenho usam um único tom de preenchimento e um raio de marcador fixo."
            }
            "A weight column by batch gives a bar chart of the count; time and weight \
             give a line plot; weight, height and batch give a bubble plot with three \
             size classes. The parallel-coordinates plot draws one line per row and the \
             correlogram annotates every scatter with its correlation." => {
                "Uma coluna de peso por lote dá um gráfico de barras da contagem; tempo e peso dão \
                 um gráfico de linhas; peso, altura e lote dão um gráfico de bolhas com três \
                 classes de tamanho. O gráfico de coordenadas paralelas desenha uma linha por \
                 linha e o correlograma anota cada dispersão com a sua correlação."
            }
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
            "Autocorrelation…" => "Autocorrelação…",
            "Cross Correlation…" => "Correlação cruzada…",
            "Unit Root Test…" => "Teste de raiz unitária…",
            "Differences…" => "Diferenças…",
            "Trend Analysis…" => "Análise de tendência…",
            "Time Series Diagnostics" => "Diagnóstico de séries temporais",
            "Time series diagnostics help" => "Ajuda do diagnóstico de séries temporais",
            "About time series diagnostics…" => "Acerca do diagnóstico de séries temporais…",
            "Series:" => "Série:",
            "Command:" => "Comando:",
            "Second series:" => "Segunda série:",
            "Maximum lag:" => "Desfasamento máximo:",
            "Lags:" => "Desfasamentos:",
            "Order:" => "Ordem:",
            "Lag:" => "Desfasamento:",
            "Operation:" => "Operação:",
            "Model:" => "Modelo:",
            "None" => "Nenhum",
            "Constant" => "Constante",
            "Constant and trend" => "Constante e tendência",
            "Quadratic" => "Quadrático",
            "Exponential" => "Exponencial",
            "Growth" => "Crescimento",
            "Difference" => "Diferença",
            "Lag" => "Desfasamento",
            "Autocorrelation" => "Autocorrelação",
            "Cross-correlation" => "Correlação cruzada",
            "Unit root" => "Raiz unitária",
            "Differences and lags" => "Diferenças e desfasamentos",
            "Trend" => "Tendência",
            "ACF and PACF with the white-noise band and the Ljung–Box statistic." => {
                "FAC e FACP com a banda de ruído branco e a estatística de Ljung–Box."
            }
            "Correlation of the first series with the second at lags −m…m; a positive \
             lag pairs x(t) with y(t+k)." => {
                "Correlação da primeira série com a segunda em desfasamentos −m…m; um \
                 desfasamento positivo emparelha x(t) com y(t+k)."
            }
            "Augmented Dickey–Fuller test with the chosen deterministic terms and \
             augmentation lags." => {
                "Teste de Dickey–Fuller aumentado com os termos determinísticos e os \
                 desfasamentos escolhidos."
            }
            "The new column keeps the rows aligned and leaves a missing value where \
             the observation does not exist." => {
                "A coluna nova mantém as linhas alinhadas e deixa um valor em falta onde a \
                 observação não existe."
            }
            "The exponential and growth models need positive values; their \
             coefficients belong to the linearised fit." => {
                "Os modelos exponencial e de crescimento exigem valores positivos; os seus \
                 coeficientes pertencem ao ajuste linearizado."
            }
            "The diagnostics read the series in row order. Autocorrelation gives the \
             ACF, the PACF (Durbin–Levinson) and the Ljung–Box statistic. \
             Cross-correlation correlates two series at lags −m…m. The unit-root test is \
             the augmented Dickey–Fuller regression, decided against the asymptotic \
             critical values. Differences and lags write a new column with the rows \
             aligned. Trend analysis fits linear, quadratic, exponential or growth \
             models to the series." => {
                "O diagnóstico lê a série pela ordem das linhas. A autocorrelação dá a FAC, a \
                 FACP (Durbin–Levinson) e a estatística de Ljung–Box. A correlação cruzada \
                 correlaciona duas séries em desfasamentos −m…m. O teste de raiz unitária é a \
                 regressão de Dickey–Fuller aumentada, decidida pelos valores críticos \
                 assintóticos. As diferenças e os desfasamentos escrevem uma coluna nova com as \
                 linhas alinhadas. A análise de tendência ajusta modelos linear, quadrático, \
                 exponencial ou de crescimento à série."
            }
            "Choose a numeric series and the command. Autocorrelation and \
             cross-correlation need the maximum lag (at most n − 2); the unit-root test \
             takes the augmentation lags and the deterministic terms (none, constant or \
             constant and trend); differences take the order (1 or 2) and the lag; the \
             trend models need at least four positive values for the exponential and \
             growth fits. Missing values are dropped, and a difference or lag keeps a \
             missing value instead of inventing an observation." => {
                "Escolha uma série numérica e o comando. A autocorrelação e a correlação cruzada \
                 precisam do desfasamento máximo (no máximo n − 2); o teste de raiz unitária \
                 recebe os desfasamentos e os termos determinísticos (nenhum, constante ou \
                 constante e tendência); as diferenças recebem a ordem (1 ou 2) e o \
                 desfasamento; os modelos de tendência precisam de pelo menos quatro valores \
                 positivos nos ajustes exponencial e de crescimento. Os valores em falta são \
                 descartados, e uma diferença ou desfasamento mantém um valor em falta em vez de \
                 inventar uma observação."
            }
            "A series that follows another by two observations has its largest \
             cross-correlation at lag 2. A random walk is not rejected by the unit-root \
             test, while a mean-reverting series is. Differencing a series once removes \
             the linear trend, and the exponential trend of a series that doubles every \
             period has a growth of 100 %." => {
                "Uma série que segue outra com dois desfasamentos tem a maior correlação \
                 cruzada no desfasamento 2. Um passeio aleatório não é rejeitado pelo teste de \
                 raiz unitária, enquanto uma série com reversão à média é. Diferenciar a série \
                 uma vez remove a tendência linear, e a tendência exponencial de uma série que \
                 duplica em cada período tem um crescimento de 100 %."
            }
            "ARIMA help" => "Ajuda do ARIMA",
            "About ARIMA…" => "Acerca do ARIMA…",
            "ARIMA models a series with autoregressive (AR) and moving-average (MA) \
             terms after differencing it d times. The report gives the coefficients, the \
             information criteria, the forecasts and their intervals; the best order \
             searches p, d and q from 0 to 2 and keeps the smallest AICc." => {
                "O ARIMA modela uma série com termos autorregressivos (AR) e de médias móveis \
                 (MA) depois de a diferenciar d vezes. O relatório dá os coeficientes, os \
                 critérios de informação, as previsões e os intervalos; a melhor ordem procura p, \
                 d e q de 0 a 2 e mantém o menor AICc."
            }
            "Choose one numeric column; non-finite values are excluded. Set the AR, \
             differencing and MA orders, or let the best-order search choose them. The \
             forecast periods are between 1 and 120. The estimation is conditional and \
             the intervals ignore parameter uncertainty." => {
                "Escolha uma coluna numérica; os valores não finitos são excluídos. Defina as \
                 ordens AR, de diferenciação e MA, ou deixe a procura da melhor ordem escolhê-las. \
                 Os períodos de previsão estão entre 1 e 120. A estimação é condicional e os \
                 intervalos ignoram a incerteza dos parâmetros."
            }
            "A series with y(t) = 0.6·y(t−1) + noise is fitted by ARIMA(1,0,0); the AR \
             coefficient is near 0.6 and the forecasts decay towards the mean. A random \
             walk needs one difference: the forecast starts at the last value and the \
             interval widens with the horizon." => {
                "Uma série com y(t) = 0,6·y(t−1) + ruído é ajustada por ARIMA(1,0,0); o \
                 coeficiente AR fica próximo de 0,6 e as previsões decaem para a média. Um passeio \
                 aleatório precisa de uma diferença: a previsão começa no último valor e o \
                 intervalo alarga com o horizonte."
            }
            "Box-Behnken Design…" => "Delineamento Box-Behnken…",
            "General Factorial Design…" => "Delineamento fatorial geral…",
            "The general factorial design runs every combination of the levels of two to \
             six factors. Each factor may have two to ten levels, coded uniformly from \
             −1 to +1, so a two-level factor uses −1/+1 and a three-level factor \
             −1/0/+1." => {
                "O delineamento fatorial geral executa todas as combinações dos níveis de dois a \
                 seis factores. Cada factor pode ter de dois a dez níveis, codificados \
                 uniformemente de −1 a +1, pelo que um factor de dois níveis usa −1/+1 e um de \
                 três níveis −1/0/+1."
            }
            "Name the factors and give one level count per factor, separated by commas. \
             Set the replicates, the randomised run order and the seed. The generated \
             sheet has StdOrder, RunOrder and one coded column per factor." => {
                "Indique os factores e um número de níveis por factor, separados por vírgulas. \
                 Defina as réplicas, a ordem aleatória das corridas e a semente. A folha gerada \
                 tem StdOrder, RunOrder e uma coluna codificada por factor."
            }
            "Two factors with three and two levels give six runs; adding one replicate \
             doubles them. For many two-level factors use the Plackett–Burman design \
             instead; definitive screening designs are outside this increment." => {
                "Dois factores com três e dois níveis dão seis corridas; acrescentar uma réplica \
                 duplica-as. Para muitos factores de dois níveis use o delineamento de \
                 Plackett–Burman; os desenhos definitivos de screening ficam fora deste incremento."
            }
            "General Factorial Design" => "Delineamento fatorial geral",
            "General factorial help" => "Ajuda do fatorial geral",
            "About general factorial designs…" => "Acerca dos delineamentos fatoriais gerais…",
            "Analyze Factorial Design…" => "Analisar delineamento fatorial…",
            "Analyze Factorial Design" => "Analisar delineamento fatorial",
            "Response:" => "Resposta:",
            "Factors:" => "Factores:",
            "General factorial (all levels)" => "Fatorial geral (todos os níveis)",
            "Screening (two-level, Lenth PSE)" => "Triagem (dois níveis, PSE de Lenth)",
            "Every factor must be coded −1/+1 with observations at both levels; the \
             effects are decided with Lenth's pseudo standard error, the Pareto \
             chart and the normal plot." => {
                "Cada factor tem de estar codificado −1/+1 com observações nos dois níveis; os \
                 efeitos são decididos com o erro padrão pseudo de Lenth, o gráfico de Pareto e o \
                 gráfico normal."
            }
            "Two to six factors with two to ten levels each; the report gives the \
             sequential and adjusted sums of squares, the interaction terms and the \
             marginal means." => {
                "Dois a seis factores com dois a dez níveis cada; o relatório dá as somas de \
                 quadrados sequenciais e ajustadas, os termos de interacção e as médias marginais."
            }
            "About factorial analysis…" => "Acerca da análise fatorial…",
            "Factorial analysis help" => "Ajuda da análise fatorial",
            "The factorial analysis fits one model to a designed experiment. The \
             general factorial takes factors with two to ten levels and reports every \
             main effect and interaction with its sequential and adjusted sums of \
             squares, the marginal means of each level and the residuals. The screening \
             analysis takes a two-level design and decides which effects stand out, \
             with Lenth's pseudo standard error and the two effect charts." => {
                "A análise fatorial ajusta um modelo a uma experiência delineada. O fatorial \
                 geral aceita factores com dois a dez níveis e reporta cada efeito principal e \
                 interacção com as somas de quadrados sequenciais e ajustadas, as médias \
                 marginais de cada nível e os resíduos. A análise de triagem aceita um \
                 delineamento de dois níveis e decide que efeitos se destacam, com o erro padrão \
                 pseudo de Lenth e os dois gráficos de efeitos."
            }
            "Choose a numeric response and the factor columns. In the general factorial \
             each factor level enters as +1, the last level as −1 and the rest as 0, so \
             a factor with k levels uses k − 1 degrees of freedom; the Seq SS adds the \
             terms in order and the Adj SS drops each term from the full model. The \
             screening needs factors coded −1/+1 with observations at both levels: the \
             effect is the mean at +1 minus the mean at −1, the PSE is 1.5 times the \
             median of the absolute effects with m/3 degrees of freedom, ME = t(0.975; \
             m/3)·PSE decides one effect at a time and SME = t(1 − 0.05/(2m); m/3)·PSE \
             decides all of them together." => {
                "Escolha uma resposta numérica e as colunas dos factores. No fatorial geral cada \
                 nível entra como +1, o último nível como −1 e os restantes como 0, pelo que um \
                 factor com k níveis usa k − 1 graus de liberdade; a SQ sequencial acrescenta os \
                 termos por ordem e a SQ ajustada retira cada termo do modelo completo. A triagem \
                 exige factores codificados −1/+1 com observações nos dois níveis: o efeito é a \
                 média em +1 menos a média em −1, o PSE é 1,5 vezes a mediana dos efeitos \
                 absolutos com m/3 graus de liberdade, ME = t(0,975; m/3)·PSE decide um efeito de \
                 cada vez e SME = t(1 − 0,05/(2m); m/3)·PSE decide todos em conjunto."
            }
            "Two factors with three and two levels and two replicates give twelve runs; \
             the terms are A (2 df), B (1 df) and A*B (2 df). A twelve-run \
             Plackett–Burman design with one effect far above the others has that effect \
             beyond the simultaneous margin, while the small effects stay inside the \
             individual margin." => {
                "Dois factores com três e dois níveis e duas réplicas dão doze corridas; os \
                 termos são A (2 gl), B (1 gl) e A*B (2 gl). Um delineamento de Plackett–Burman \
                 com doze corridas e um efeito muito acima dos outros tem esse efeito além da \
                 margem simultânea, enquanto os efeitos pequenos ficam dentro da margem individual."
            }
            "Mixture Design…" => "Delineamento de mistura…",
            "Optimal Design…" => "Delineamento ótimo…",
            "Optimal Design" => "Delineamento ótimo",
            "Optimal design help" => "Ajuda dos desenhos ótimos",
            "About optimal designs…" => "Acerca dos desenhos ótimos…",
            "Evaluate" => "Avaliar",
            "Optimal designs choose the runs of a first-order experiment from a candidate \
             set. The D criterion maximises the determinant of XᵀX, so the coefficients \
             are estimated with the smallest joint variance; the distance criterion \
             maximises the minimum separation of the runs (maximin)." => {
                "Os desenhos ótimos escolhem as corridas de uma experiência de primeira ordem a                  partir de um conjunto de candidatos. O critério D maximiza o determinante de XᵀX,                  para que os coeficientes sejam estimados com a menor variância conjunta; o                  critério de distância maximiza a separação mínima das corridas (maximin)."
            }
            "Name two to six factors; the candidates are the −1/+1 factorial plus the \
             centre. Create writes a new sheet with the requested runs. Augment adds runs \
             to the current coded design, keeping its runs. Evaluate reports the \
             D-efficiency, the condition number and the minimum/maximum distance of the \
             current design." => {
                "Indique dois a seis factores; os candidatos são o fatorial −1/+1 mais o centro.                  Criar escreve uma folha nova com as corridas pedidas. Aumentar acrescenta corridas                  ao desenho codificado actual, mantendo as existentes. Avaliar reporta a                  eficiência D, o número de condição e a distância mínima/máxima do desenho actual."
            }
            "Three factors with eight runs give the full factorial, D-efficiency one. \
             Augmenting the four runs of a two-factor design with two more completes the \
             corners, and the evaluation shows the condition number falling towards one." => {
                "Três factores com oito corridas dão o fatorial completo, com eficiência D igual a                  um. Aumentar as quatro corridas de um desenho de dois factores com mais duas                  completa os cantos, e a avaliação mostra o número de condição a aproximar-se de um."
            }
            "Mixture Design" => "Delineamento de mistura",
            "Mixture design help" => "Ajuda dos desenhos de mistura",
            "About mixture designs…" => "Acerca dos desenhos de mistura…",
            "A mixture design varies the proportions of components that always sum to \
             one. The simplex centroid lists the pure components, the binary blends and \
             so on to the overall centroid; the simplex lattice of degree m uses every \
             composition in multiples of 1/m; the extreme vertices are the corners of the \
             region allowed by the component bounds." => {
                "Um desenho de mistura varia as proporções de componentes que somam sempre um. O                  centróide simplex lista os componentes puros, as misturas binárias e assim por                  diante até ao centróide global; a rede simplex de grau m usa todas as composições                  em múltiplos de 1/m; os vértices extremos são os cantos da região permitida pelos                  limites dos componentes."
            }
            "List two to ten components and choose the design. The lattice degree is 1 \
             to 5. Optional lower and upper bounds filter the centroid/lattice points \
             and define the extreme-vertices region; leave them blank for 0 and 1. Set \
             the replicates and the randomisation seed; the generated sheet has StdOrder, \
             RunOrder and one proportion column per component." => {
                "Liste dois a dez componentes e escolha o desenho. O grau da rede é de 1 a 5. Os                  limites inferior e superior, opcionais, filtram os pontos do centróide/rede e                  definem a região dos vértices extremos; deixe-os em branco para 0 e 1. Defina as                  réplicas e a semente de randomização; a folha gerada tem StdOrder, RunOrder e uma                  coluna de proporção por componente."
            }
            "Three components give seven centroid runs (three pure, three 50/50 blends \
             and the 1/3 centroid) and six degree-2 lattice runs. With bounds 0.1 to 0.7 \
             the extreme vertices keep every component inside the range and still sum \
             to one." => {
                "Três componentes dão sete corridas centróide (três puras, três misturas 50/50 e o                  centróide 1/3) e seis corridas da rede de grau 2. Com limites de 0,1 a 0,7 os                  vértices extremos mantêm cada componente dentro do intervalo e continuam a somar                  um."
            }
            "Tolerance Interval…" => "Intervalo de tolerância…",
            "Chi-Square Test for Independence" => "Teste qui-quadrado de independência",
            "Power and Sample Size" => "Potência e tamanho amostral",
            "Data and Calculations…" => "Dados e cálculos…",
            "Data and Calculations" => "Dados e cálculos",
            "Data and calculations help" => "Ajuda de dados e cálculos",
            "About data and calculations…" => "Acerca de dados e cálculos…",
            "Tasks, conventions and an example" => "Tarefas, convenções e um exemplo",
            "Task:" => "Tarefa:",
            "Reference:" => "Referência:",
            "Validation folds:" => "Dobras de validação:",
            "Name 1:" => "Nome 1:",
            "Name 2:" => "Nome 2:",
            "Name 3:" => "Nome 3:",
            "First 1:" => "Primeiro 1:",
            "First 2:" => "Primeiro 2:",
            "First 3:" => "Primeiro 3:",
            "Last 1:" => "Último 1:",
            "Last 2:" => "Último 2:",
            "Last 3:" => "Último 3:",
            "Levels 1:" => "Níveis 1:",
            "Levels 2:" => "Níveis 2:",
            "Levels 3:" => "Níveis 3:",
            "Each factor is a name, the first and last level and the number \
             of equally spaced levels; the first factor varies fastest." => {
                "Cada factor é um nome, o primeiro e o último nível e o número de níveis \
                 igualmente espaçados; o primeiro factor varia mais rápido."
            }
            "Statistic:" => "Estatística:",
            "New column:" => "Nova coluna:",
            "Start:" => "Início:",
            "Step:" => "Passo:",
            "Length:" => "Comprimento:",
            "Repeats:" => "Repetições:",
            "Factor column:" => "Coluna do factor:",
            "Drop the first level (reference)" => "Omitir o primeiro nível (referência)",
            "Distribution:" => "Distribuição:",
            "Mean:" => "Média:",
            "Minimum:" => "Mínimo:",
            "Rate:" => "Taxa:",
            "Std dev:" => "Desvio padrão:",
            "Maximum:" => "Máximo:",
            "Values:" => "Valores:",
            "Resamples:" => "Reamostras:",
            "First sample:" => "Primeira amostra:",
            "Second sample:" => "Segunda amostra:",
            "Permutations:" => "Permutações:",
            "Tasks" => "Tarefas",
            "Conventions" => "Convenções",
            "Appends <column>_z, centred on the mean and divided by the \
             sample standard deviation." => {
                "Acrescenta <coluna>_z, centrada na média e dividida pelo desvio padrão \
                 amostral."
            }
            "Each row is name, first level, last level and number of \
             equally spaced levels." => {
                "Cada linha é nome, primeiro nível, último nível e número de níveis \
                 igualmente espaçados."
            }
            "Each selected column becomes a row; each row a column." => {
                "Cada coluna escolhida torna-se uma linha; cada linha uma coluna."
            }
            "Aᵀ·B: both blocks have the same rows; the result has one row per \
             column of the first block and one column per column of the \
             second." => {
                "Aᵀ·B: os dois blocos têm as mesmas linhas; o resultado tem uma linha por \
                 coluna do primeiro bloco e uma coluna por coluna do segundo."
            }
            "Each resample copies the whole rows with replacement; the table \
             has a Sample column and every selected column, and the Session \
             receives the percentile interval of the mean." => {
                "Cada reamostra copia as linhas inteiras com reposição; a tabela tem uma \
                 coluna Sample e todas as colunas escolhidas, e a Session recebe o intervalo \
                 percentílico da média."
            }
            "The dialog works on the active worksheet. Standardize, the indicator \
             columns and the row statistics add columns to it (in a new sheet), while \
             the patterned, mesh, random, bootstrap, transposed and matrix results \
             open a new sheet of their own. The column statistics and the \
             randomization test report to the Session window." => {
                "O diálogo trabalha na folha activa. A padronização, as colunas indicadoras e \
                 as estatísticas por linha acrescentam colunas (numa folha nova), enquanto os \
                 padrões, a mesh, os dados aleatórios, o bootstrap, a transposição e o produto \
                 de matrizes abrem uma folha própria. As estatísticas por coluna e o teste de \
                 randomização reportam à janela Session."
            }
            "Standardize appends z = (x − mean)/s with the sample standard deviation. \
             Column statistics give N, N*, sum, mean, standard error, standard \
             deviation, quartiles and extremes. Row statistics combine the selected \
             columns row by row, skipping missing cells. Patterned data repeat an \
             arithmetic sequence. Mesh data build the full grid of one to three \
             numeric factors. Indicators code a categorical column as 0/1 columns, \
             with or without the first level. Transpose swaps rows and columns, and \
             the matrix product multiplies the transpose of the first block by the \
             second. Random data draw Normal, Uniform or Exponential values. The \
             bootstrap resamples whole rows with replacement, and the randomization \
             test permutes the group labels of two columns." => {
                "A padronização acrescenta z = (x − média)/s com o desvio padrão amostral. As \
                 estatísticas por coluna dão N, N*, soma, média, erro-padrão, desvio padrão, \
                 quartis e extremos. As estatísticas por linha combinam as colunas escolhidas \
                 linha a linha, ignorando células em falta. Os dados padrão repetem uma \
                 progressão aritmética. A mesh constrói a grelha completa de um a três \
                 factores numéricos. Os indicadores codificam uma coluna categórica em \
                 colunas 0/1, com ou sem o primeiro nível. A transposição troca linhas e \
                 colunas, e o produto de matrizes multiplica a transposta do primeiro bloco \
                 pelo segundo. Os dados aleatórios geram valores Normal, Uniforme ou \
                 Exponencial. O bootstrap reamostra linhas inteiras com reposição, e o teste \
                 de randomização permuta as etiquetas de grupo de duas colunas."
            }
            "Every random task takes a seed and uses SplitMix64, so the same seed gives \
             the same sheet and the same p-value. The randomization p-value is \
             (extreme + 1)/(permutations + 1) and the test is two-sided on the \
             difference in means. A missing or non-finite cell is left out of the \
             statistics and stays missing when a column is standardized." => {
                "Cada tarefa aleatória recebe uma semente e usa SplitMix64, pelo que a mesma \
                 semente dá a mesma folha e o mesmo valor-p. O valor-p do teste de \
                 randomização é (extremos + 1)/(permutações + 1) e o teste é bilateral sobre a \
                 diferença de médias. Uma célula em falta ou não finita fica fora das \
                 estatísticas e continua em falta quando a coluna é padronizada."
            }
            "Standardising a column from 1 to 5 gives z from about −1.265 to 1.265, with \
             mean zero and standard deviation one. A mesh of two factors with three and \
             two levels has six rows. Indicators of three levels with the reference \
             dropped add two columns. A bootstrap of five rows and three resamples has \
             fifteen rows, and two clearly separated samples give a randomization \
             p-value near zero." => {
                "Padronizar uma coluna de 1 a 5 dá z entre cerca de −1,265 e 1,265, com média \
                 zero e desvio padrão um. Uma mesh de dois factores com três e dois níveis tem \
                 seis linhas. Indicadores de três níveis com a referência omitida acrescentam \
                 duas colunas. Um bootstrap de cinco linhas e três reamostras tem quinze \
                 linhas, e duas amostras claramente separadas dão um valor-p de randomização \
                 próximo de zero."
            }
            "Power and sample size help" => "Ajuda da potência e tamanho amostral",
            "About power and sample size…" => "Acerca da potência e tamanho amostral…",
            "Procedures, effect sizes and an example" => {
                "Procedimentos, tamanhos de efeito e um exemplo"
            }
            "Effect sizes" => "Tamanhos de efeito",
            "Linear model" => "Modelo linear",
            "Effect size f²:" => "Tamanho de efeito f²:",
            "Tested terms:" => "Termos em teste:",
            "Model terms:" => "Termos do modelo:",
            "The power of a test is the probability of rejecting the null hypothesis \
             when the effect is the one you care about. A sample size of zero asks for \
             the smallest size that reaches the requested power; a positive size \
             reports the power of that size. Every procedure follows a published \
             distribution: the t tests use the non-central t, the proportion test the \
             normal approximation, the correlation test Fisher's transformation and the \
             F tests the non-central F." => {
                "A potência de um teste é a probabilidade de rejeitar a hipótese nula quando \
                 o efeito é o que importa. Um tamanho amostral zero pede o menor tamanho que \
                 atinge a potência pretendida; um tamanho positivo reporta a potência desse \
                 tamanho. Cada procedimento segue uma distribuição publicada: os testes t usam \
                 a t não central, o teste de proporções a aproximação normal, o teste de \
                 correlação a transformação de Fisher e os testes F a F não central."
            }
            "The t tests take a standardised difference in units of the standard \
             deviation; the proportion test the true proportion and the null one; the \
             correlation test the population correlation; the ANOVA a standardised \
             difference between the group that differs and the others; the overall \
             regression the population R²; and the linear model Cohen's f² of the terms \
             under test, f² = (R² full − R² reduced)/(1 − R² full)." => {
                "Os testes t recebem uma diferença padronizada em unidades do desvio padrão; \
                 o teste de proporções a proporção verdadeira e a nula; o teste de correlação a \
                 correlação populacional; a ANOVA uma diferença padronizada entre o grupo que \
                 difere e os restantes; a regressão global o R² populacional; e o modelo linear \
                 o f² de Cohen dos termos em teste, f² = (R² completo − R² reduzido)/(1 − R² \
                 completo)."
            }
            "The linear model runs the partial F test: the tested terms q and the model \
             terms p give the numerator and denominator degrees of freedom, and the \
             non-centrality is f² times the sample size. With q equal to p it is the \
             overall regression test; with q equal to one it is the t test of a single \
             coefficient. Use the tested terms for the R² increase you want to detect \
             and the model terms for everything the model estimates." => {
                "O modelo linear executa o teste F parcial: os termos em teste q e os termos do \
                 modelo p dão os graus de liberdade do numerador e do denominador, e a \
                 não-centralidade é f² vezes o tamanho amostral. Com q igual a p é o teste de \
                 regressão global; com q igual a um é o teste t de um único coeficiente. Use os \
                 termos em teste para o aumento de R² que quer detectar e os termos do modelo \
                 para tudo o que o modelo estima."
            }
            "A standardised difference of 0.5 needs 34 observations for 80 % power in \
             a one-sample test, and 17 per group for a two-sample test with a \
             difference of 1. A linear model with f² = 0.15, three tested terms and \
             five model terms needs about 80 observations for 80 % power at 5 %." => {
                "Uma diferença padronizada de 0,5 precisa de 34 observações para 80 % de \
                 potência num teste de uma amostra, e 17 por grupo num teste de duas amostras \
                 com diferença de 1. Um modelo linear com f² = 0,15, três termos em teste e \
                 cinco termos no modelo precisa de cerca de 80 observações para 80 % de \
                 potência a 5 %."
            }
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
            "Graphical Summary" => "Resumo gráfico",
            "Graphical Summary…" => "Resumo gráfico…",
            "Store Descriptive Statistics" => "Guardar estatísticas descritivas",
            "Store Descriptive Statistics…" => "Guardar estatísticas descritivas…",
            "Summary help" => "Ajuda do resumo",
            "About these commands…" => "Acerca destes comandos…",
            "The graphical summary stacks a histogram, a boxplot and the 95% confidence \
             intervals of the mean, the median and the standard deviation. Store \
             Descriptive Statistics writes the selected statistics as new worksheet \
             columns, one row per variable." => {
                "O resumo gráfico empilha um histograma, uma caixa de bigodes e os intervalos de \
                 confiança a 95 % da média, da mediana e do desvio padrão. Guardar estatísticas \
                 descritivas escreve as estatísticas escolhidas em novas colunas da folha, com \
                 uma linha por variável."
            }
            "Choose one numeric column for the summary. For the store command, select one \
             or more numeric columns and the statistics: the table gets a Variable column \
             and one column per statistic at the end of the worksheet, below the existing \
             rows. Missing values are excluded and counted." => {
                "Escolha uma coluna numérica para o resumo. Para o comando de gravação, escolha \
                 uma ou mais colunas numéricas e as estatísticas: a tabela ganha uma coluna \
                 Variable e uma coluna por estatística no fim da folha, abaixo das linhas \
                 existentes. Os valores em falta são excluídos e contados."
            }
            "Five weights 10, 11, 12, 13 and 14 give a mean of 12 and a standard \
             deviation of 1.581. Storing N, Mean and StDev adds three columns with one \
             row for the variable." => {
                "Cinco pesos 10, 11, 12, 13 e 14 dão uma média de 12 e um desvio padrão de \
                 1,581. Guardar N, Mean e StDev acrescenta três colunas com uma linha para a \
                 variável."
            }
            "Covariance" => "Covariância",
            "Covariance…" => "Covariância…",
            "Tally" => "Contagens",
            "Tally…" => "Contagens…",
            "Covariance and tally help" => "Ajuda da covariância e das contagens",
            "The covariance matrix measures how pairs of numeric columns vary together; \
             the tally counts the unique values of one or more columns with their \
             percentages." => {
                "A matriz de covariâncias mede como pares de colunas numéricas variam em \
                 conjunto; as contagens listam os valores únicos de uma ou mais colunas com as \
                 respectivas percentagens."
            }
            "Choose at least two numeric columns for the covariance matrix, separated by \
             commas; each cell uses the rows that are complete in both columns. Tally \
             accepts numeric, text and date columns; each selected column gets a table \
             with the count, percent, cumulative count and cumulative percent. Missing \
             values are excluded and counted." => {
                "Escolha pelo menos duas colunas numéricas para a matriz de covariâncias, \
                 separadas por vírgulas; cada célula usa as linhas completas nas duas colunas. \
                 As contagens aceitam colunas numéricas, de texto e de data; cada coluna \
                 escolhida ganha uma tabela com contagem, percentagem, contagem acumulada e \
                 percentagem acumulada. Os valores em falta são excluídos e contados."
            }
            "x = 1, 2, 3, 4 and y = 2, 4, 6, 8 give Var(x) = 1.667, Cov(x, y) = 3.333 \
             and Var(y) = 6.667. A grade column A, A, B, A, B tallies A with 3 (60 %) \
             and B with 2 (40 %)." => {
                "x = 1, 2, 3, 4 e y = 2, 4, 6, 8 dão Var(x) = 1,667, Cov(x, y) = 3,333 e \
                 Var(y) = 6,667. Uma coluna de notas A, A, B, A, B conta A com 3 (60 %) e B com \
                 2 (40 %)."
            }
            "Probability Distribution Plot" => "Gráfico de distribuição de probabilidade",
            "Probability Distribution Plot…" => "Gráfico de distribuição de probabilidade…",
            "Probability Distribution Plot help" => {
                "Ajuda do gráfico de distribuição de probabilidade"
            }
            "About this plot…" => "Acerca deste gráfico…",
            "The probability distribution plot draws the theoretical density or \
             cumulative probability of a distribution you specify. No worksheet data are \
             used. You can shade the area between two values to read the probability." => {
                "O gráfico de distribuição de probabilidade desenha a densidade ou a \
                 probabilidade acumulada teórica de uma distribuição escolhida. Não são usados \
                 dados da folha. Pode sombrear a área entre dois valores para ler a \
                 probabilidade."
            }
            "Choose one of the ten distributions and enter its parameters; the labels \
             change with the family. Choose Density or Cumulative probability. \
             Optionally enter two values of x to shade the area between them; the report \
             gives the shaded probability F(b) − F(a)." => {
                "Escolha uma das dez distribuições e introduza os parâmetros; os rótulos mudam \
                 com a família. Escolha Density ou Cumulative probability. Opcionalmente \
                 introduza dois valores de x para sombrear a área entre eles; o relatório dá a \
                 probabilidade sombreada F(b) − F(a)."
            }
            "A standard normal (mean 0, standard deviation 1) peaks at 0.399. Shading \
             from −1 to 1 covers 68.27 % of the area; the cumulative curve reaches 0.5 \
             at the mean." => {
                "Uma normal padrão (média 0, desvio padrão 1) tem o pico em 0,399. Sombrear de \
                 −1 a 1 cobre 68,27 % da área; a curva acumulada atinge 0,5 na média."
            }
            "Acceptance Chart" => "Carta de aceitação",
            "Acceptance Chart…" => "Carta de aceitação…",
            "Acceptance chart help" => "Ajuda da carta de aceitação",
            "About acceptance charts…" => "Acerca das cartas de aceitação…",
            "The acceptance chart plots the mean of each subgroup against limits \
             derived from the specification. It accepts a process whose mean is inside \
             the acceptance limits, rejects a process whose mean is beyond a rejection \
             limit, and asks for another subgroup in between." => {
                "A carta de aceitação desenha a média de cada subgrupo contra limites \
                 derivados da especificação. Aceita um processo cuja média está dentro dos \
                 limites de aceitação, rejeita um processo cuja média ultrapassa um limite de \
                 rejeição e pede outro subgrupo entre os dois."
            }
            "One numeric column holds the measurements and at least one specification \
             limit is required. The accepted proportion p1 and the rejectable proportion \
             p2 are the proportions allowed outside a specification limit; the producer \
             risk α and the consumer risk β set how often each decision may be wrong. \
             Leave sigma blank to estimate it from the average subgroup range, or enter \
             a known value. A subgroup size of 1 charts the individuals." => {
                "Uma coluna numérica contém as medições e é exigido pelo menos um limite de \
                 especificação. A proporção aceitável p1 e a proporção rejeitável p2 são as \
                 proporções permitidas fora de um limite de especificação; o risco do produtor \
                 α e o risco do consumidor β definem a frequência com que cada decisão pode \
                 errar. Deixe sigma em branco para o estimar a partir da amplitude média dos \
                 subgrupos, ou introduza um valor conhecido. Um tamanho de subgrupo de 1 \
                 representa os indivíduos."
            }
            "With LSL 0 and USL 10, sigma 1, p1 0.001, p2 0.05, α 0.05 and β 0.10, the \
             upper acceptance limit is 8.555 and the upper rejection limit is 9.637. A \
             subgroup mean of 9.0 continues, 9.8 rejects and 5.0 accepts." => {
                "Com LSL 0 e USL 10, sigma 1, p1 0,001, p2 0,05, α 0,05 e β 0,10, o limite \
                 superior de aceitação é 8,555 e o limite superior de rejeição é 9,637. Uma \
                 média de subgrupo de 9,0 continua, 9,8 rejeita e 5,0 aceita."
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
            "Factorial Plots" => "Gráficos factoriais",
            "Factorial Plots…" => "Gráficos factoriais…",
            "Factorial plots help" => "Ajuda dos gráficos factoriais",
            "About factorial plots…" => "Acerca dos gráficos factoriais…",
            "The factorial plots show the main effect of every factor, the interaction \
             of every pair and, for two or three factors, the cube of the corner means. \
             They describe a coded two-level factorial before a formal model is chosen." => {
                "Os gráficos factoriais mostram o efeito principal de cada factor, a interacção \
                 de cada par e, para dois ou três factores, o cubo das médias dos cantos. \
                 Descrevem um factorial de dois níveis codificado antes de se escolher o modelo \
                 formal."
            }
            "Choose a numeric response and at least one factor column already coded to \
             low = −1 and high = +1. The plots use the observed marginal mean of every \
             factor combination; rows with a value outside the two levels are not used. \
             At most five factors are accepted." => {
                "Escolha uma resposta numérica e pelo menos uma coluna de factor já codificada \
                 com baixo = −1 e alto = +1. Os gráficos usam a média marginal observada de cada \
                 combinação de factores; as linhas com um valor fora dos dois níveis não são \
                 usadas. São aceites no máximo cinco factores."
            }
            "A 2² design with y = 10 + 2A + 3B + AB gives a main effect of +2 for A and \
             +6 for B; the interaction panel shows the two B lines diverging, and the \
             cube corners list the mean of each of the four combinations." => {
                "Um delineamento 2² com y = 10 + 2A + 3B + AB dá um efeito principal de +2 para A \
                 e +6 para B; o painel de interacção mostra as duas linhas de B a divergir, e os \
                 cantos do cubo listam a média de cada uma das quatro combinações."
            }
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
            "General Linear Model…" => "Modelo linear geral…",
            "General Linear Model" => "Modelo linear geral",
            "General linear model help" => "Ajuda do modelo linear geral",
            "About the general linear model…" => "Acerca do modelo linear geral…",
            "The general linear model fits one numeric response from categorical \
             factors, numeric covariates and two-way interactions. Factors use reference \
             coding and every term gets sequential (Type I) and adjusted (Type III) sums \
             of squares with an F test." => {
                "O modelo linear geral ajusta uma resposta numérica a partir de factores \
                 categóricos, covariáveis numéricas e interacções de duas vias. Os factores usam \
                 codificação de referência e cada termo recebe somas de quadrados sequenciais \
                 (Tipo I) e ajustadas (Tipo III) com um teste F."
            }
            "Choose the response, the factors and the covariates. Factors are the text or \
             date columns; covariates are the numeric columns. Write each interaction \
             as A*B, separated by commas, using names already declared as factors or \
             covariates. Rows missing the response, a factor label or a covariate are \
             removed listwise." => {
                "Escolha a resposta, os factores e as covariáveis. Os factores são as colunas de \
                 texto ou de data; as covariáveis são as colunas numéricas. Escreva cada \
                 interacção como A*B, separada por vírgulas, usando nomes já declarados como \
                 factores ou covariáveis. As linhas sem resposta, rótulo de factor ou covariável \
                 são removidas em lista."
            }
            "A response measured on two machines (factor) at several speeds (covariate), \
             with the Machine*Speed interaction, gives an ANOVA table with Machine, \
             Speed and Machine*Speed; the interaction tests whether the speed slopes \
             differ between the machines." => {
                "Uma resposta medida em duas máquinas (factor) a várias velocidades (covariável), \
                 com a interacção Machine*Speed, dá uma tabela ANOVA com Machine, Speed e \
                 Machine*Speed; a interacção testa se os declives da velocidade diferem entre as \
                 máquinas."
            }
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
        "ARIMA",
        "ARIMA…",
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
        "CART",
        "Factor:",
        "Linear",
        "LTPD %:",
        "Gompertz",
        "MANOVA",
        "MANOVA…",
        "Michaelis–Menten",
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
