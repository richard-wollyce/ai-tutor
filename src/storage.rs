//! Gerenciamento da base de conhecimento local compatível com o formato Ulpia.

use std::fs;
use std::path::Path;
use kb::base::Base;
use kb::memory::index_path;
use kb::store::Store;

/// Inicializa a estrutura da base de conhecimento Ulpia para o Wollyce
/// caso ainda não exista:
/// - agent.txt (declaração de identidade)
/// - MAP.md (mapa de roteamento do Ulpia)
/// - knowledge/ (diretório de notas didáticas)
/// - .kb/index.db (índice SQLite FTS5 do Ulpia)
pub fn init_wollyce_base(base_root: &Path) -> Result<(), String> {
    fs::create_dir_all(base_root).map_err(|e| e.to_string())?;

    let knowledge_dir = base_root.join("knowledge");
    fs::create_dir_all(&knowledge_dir).map_err(|e| e.to_string())?;

    // 1. agent.txt
    let agent_txt = base_root.join("agent.txt");
    if !agent_txt.exists() {
        let agent_content = "\
# Identidade do Tutor Wollyce no Ulpia
name = Wollyce
role = Tutor socrático e preceptor particular de aprendizagem ativa
ends = Wollyce para onde a pergunta não é sobre os materiais de estudo do aluno.
";
        fs::write(&agent_txt, agent_content).map_err(|e| e.to_string())?;
    }

    // 2. MAP.md
    let map_md = base_root.join("MAP.md");
    if !map_md.exists() {
        let map_content = "\
# Wollyce, mapa de conhecimento

**Search for:** wollyce, tutor, estudo, socratico, feynman, aprendizado ativo, retencao, revisao, exercicios, perguntas, trivium, quadrivium, livro, apostila

**Exists to:** Organizar e rotear os materiais de estudo ingeridos pelo aluno.

## Materiais Ingeridos
- [[trivium-fundamentos]]: Fundamentos do Trivium e as Artes Liberais
";
        fs::write(&map_md, map_content).map_err(|e| e.to_string())?;
    }

    // 3. Documento inicial de amostra (Trivium)
    let sample_file = knowledge_dir.join("trivium-fundamentos.md");
    if !sample_file.exists() {
        let sample_content = "\
# Fundamentos do Trivium e as Artes Liberais

**Search for:** trivium, quadrivium, artes liberais, gramatica, logica, dialetica, retorica, silogismo, verdade, argumento, comunicacao, pensar, falar, expressao, linguagem, falacia, premissa, conclusao

O Trivium constitui o fundamento clássico das sete Artes Liberais da tradição educacional ocidental. A palavra deriva do latim 'três caminhos' e organiza a mente humana para o aprendizado de qualquer outro domínio do conhecimento.

O Trivium divide-se rigorosamente em três disciplinas complementares:

1. **Gramática (A Arte de Nomear):** A estrutura da linguagem. Cuida da mecânica das palavras, da sintaxe, dos símbolos e da apreensão inicial dos fatos da realidade. Sem gramática, não há matéria-prima para o pensamento.
2. **Lógica ou Dialética (A Arte do Pensamento Correto):** A mecânica do raciocínio. Cuida da eliminação de contradições, da identificação de falácias e da construção de silogismos válidos a partir de premissas verdadeiras.
3. **Retórica (A Arte da Comunicação e Persuasão):** A transmissão e expressão elegante da verdade encontrada pela lógica e estruturada pela gramática, movendo outros homens em direção ao bem e à razão.

O Quadrivium (Aritmética, Geometria, Música e Astronomia) sucede o Trivium, estudando a matéria e o número no espaço e no tempo.
";
        fs::write(&sample_file, sample_content).map_err(|e| e.to_string())?;
    }

    // 4. Sincroniza o índice SQLite do Ulpia
    sync_index(base_root)?;

    Ok(())
}

/// Sincroniza o índice SQLite do Ulpia (`.kb/index.db`) com os arquivos de Markdown em disco.
pub fn sync_index(base_root: &Path) -> Result<(), String> {
    let base = Base::discover(base_root, false)
        .map_err(|e| format!("Falha ao inspecionar arquivos da base: {e}"))?;

    let db_path = index_path(base_root);
    let mut store = Store::open(&db_path)
        .map_err(|e| format!("Falha ao abrir banco de índice do Ulpia em {}: {e}", db_path.display()))?;

    store
        .sync(&base, "wollyce")
        .map_err(|e| format!("Falha ao sincronizar índice do Ulpia: {e}"))?;

    Ok(())
}
