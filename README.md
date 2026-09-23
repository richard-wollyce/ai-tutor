# Wollyce AI Tutor

Preceptor socrático particular e tutor de aprendizagem ativa desenvolvido em Rust, com recuperação de conhecimento determinística local-first, interface desktop e controle rígido de consumo de tokens.

---

## 🏛️ Arquitetura e Filosofia

O **Wollyce** combina metodologias das Ciências da Aprendizagem com uma camada de recuperação em arquivos Markdown locais indexados por SQLite FTS5 (BM25), eliminando alucinações e garantindo que o tutor responda com base nos materiais de estudo reais do aluno.

### Metodologias Pedagógicas Dinâmicas
O tutor analisa as perguntas e aplica dinamicamente uma estratégia de ensino:
* **Técnica Feynman:** Explicação intuitiva de conceitos sem jargões.
* **Heurística de George Pólya:** Resolução passo a passo de algoritmos e problemas práticos.
* **Active Recall (Peter Brown):** Desafio cognitivo para retenção ativa de conceitos.
* **Andaimes Cognitivos e ZPD (Lev Vygotsky):** Aprendizado gradual e estruturado.
* **Interrogação Elaborativa (John Dunlosky):** Investigação de causas, mecanismos e conexões.
* **Maiêutica Socrática (Platão):** Reflexão crítica e desconstrução de premissas.

### Segurança e Local-First
* **Chaves de API no Chaveiro do SO:** No macOS, utiliza o `Keychain` nativo (`security`); no Windows, DPAPI (`CryptProtectData`).
* **Zero Servidores Externos:** Banco de dados SQLite embutido via `rusqlite` bundled.
* **Recuperação Determinística:** A biblioteca de notas é indexada e auditável sem depender de embeddings vetoriais opacos.

---

## 📂 Estrutura do Projeto

```text
ai-tutor/
├── Cargo.toml               # Manifesto principal da aplicação
├── src/                     # Núcleo da aplicação Wollyce
│   ├── main.rs              # Inicialização do servidor e abertura do navegador
│   ├── tutor.rs             # Seleção de metodologia e orquestração socrática
│   ├── server.rs            # Servidor HTTP local (porta 4242)
│   ├── storage.rs           # Gerenciamento e sincronização da base de conhecimento
│   ├── metering.rs          # Medição de tokens e custos em SQLite
│   ├── ingest.rs            # Ingestão e sanitização de notas (MD/PDF)
│   └── security.rs          # Guardrails contra prompt injection
├── ui/
│   └── index.html           # Interface web limpa incorporada ao binário
├── tests/
│   └── integration_test.rs  # Testes de integração automatizados
├── crates/                  # Motores internos
│   ├── kb/                  # Motor de recuperação determinística e SQLite
│   └── model-call/          # Cliente de provedores de IA e armazenamento seguro de chaves
└── data/
    └── wollyce_storage/     # Notas de estudo e base de conhecimento do aluno
```

---

## 🚀 Como Executar

### Pré-requisitos
* Rust (instalado via `rustup`):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

### Executar a Aplicação
Execute sempre a partir da raiz do projeto:

```bash
cargo run --release
```

A aplicação iniciará o servidor local e abrirá automaticamente no navegador em:
**`http://127.0.0.1:4242`**

### Testes
Para rodar a suíte completa de testes:

```bash
cargo test
```

---

## 🔑 Configuração de Provedores de IA

O Wollyce suporta Gemini, Anthropic (Claude) e OpenAI:
* No macOS, configure a chave via variável de ambiente (`GEMINI_API_KEY`, `ANTHROPIC_API_KEY`, ou `OPENAI_API_KEY`) ou registre no Chaveiro do sistema via `model-call`.
* Por padrão, a aplicação seleciona a primeira chave disponível na ordem: **Gemini > Claude > OpenAI**.
